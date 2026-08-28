//! Agrégat `Provider` : prestataire de dépannage (FR-003, Story 1.6).
//!
//! **Le KYC n'est pas fait, et le type l'impose.** FR-003 exige la validation
//! du numéro à la Banque-Carrefour des Entreprises, le contrôle de l'état de
//! faillite et la collecte d'une attestation d'assurance. Rien de tout cela
//! n'est possible ici : l'API de la BCE, le stockage objet chiffré et
//! l'antivirus sont hors du périmètre vitrine.
//!
//! Un prestataire naît donc `PENDING_KYC` et le seul chemin vers `ACTIVE`
//! passe par `valider_kyc`, qui **exige la preuve du contrôle** sous forme de
//! `PreuveKyc`. Ce type ne se construit que par
//! `PreuveKyc::depuis_verification_bce`, qui n'a aujourd'hui aucun appelant
//! légitime — ou par `PreuveKyc::demonstration`, dont le nom dit ce qu'elle
//! vaut et que seul un binaire hors ligne emploie.
//!
//! Le but n'est pas d'empêcher quiconque de tricher : c'est qu'un prestataire
//! actif sans contrôle réel soit **visible dans le code** plutôt que caché
//! derrière un booléen.

use chrono::{DateTime, Utc};
use klaar_catalog::CodeCatalogue;
use klaar_shared_kernel::Geo;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use crate::numero_bce::NumeroBce;

/// Longueur maximale de la raison sociale.
pub const RAISON_SOCIALE_MAX: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatutProvider {
    /// Inscrit, en attente du contrôle BCE. Ne reçoit aucune Demande.
    EnAttenteKyc,
    Actif,
    /// Écarté du matching sans être effacé : contrôle échu, incident, ou
    /// suspension à sa propre demande.
    Suspendu,
    /// Contrôle refusé par l'exploitation (FR-038).
    ///
    /// **Distinct de `Suspendu`.** Un suspendu a été actif et pourra l'être à
    /// nouveau ; un refusé n'est jamais entré. Les confondre ferait apparaître
    /// dans les statistiques de sanction des entreprises qui n'ont jamais
    /// travaillé.
    Refuse,
    /// L'entreprise a retiré sa demande d'inscription avant décision
    /// (FR-038 `@edge`).
    ///
    /// **Ce n'est pas un refus.** Personne n'a rien jugé : lui donner le statut
    /// « refusé » inscrirait dans son dossier une décision qui n'a pas été
    /// prise.
    Retire,
}

impl StatutProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::EnAttenteKyc => "PENDING_KYC",
            Self::Actif => "ACTIVE",
            Self::Suspendu => "SUSPENDED",
            Self::Refuse => "REJECTED",
            Self::Retire => "WITHDRAWN",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "PENDING_KYC" => Some(Self::EnAttenteKyc),
            "ACTIVE" => Some(Self::Actif),
            "SUSPENDED" => Some(Self::Suspendu),
            "REJECTED" => Some(Self::Refuse),
            "WITHDRAWN" => Some(Self::Retire),
            _ => None,
        }
    }
}

/// Preuve qu'un contrôle d'identité d'entreprise a eu lieu.
///
/// Type opaque, sans constructeur littéral : on ne peut pas en fabriquer une
/// en écrivant `PreuveKyc { .. }` ailleurs dans le code. Activer un
/// prestataire suppose d'en tenir une, et il n'y a que deux façons d'en
/// obtenir — dont une qui s'appelle `demonstration`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreuveKyc {
    origine: OrigineKyc,
    verifie_le: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrigineKyc {
    /// Contrôle réel auprès de la Banque-Carrefour des Entreprises.
    Bce,
    /// Un humain de l'exploitation a examiné les pièces (FR-038).
    ///
    /// **Ce n'est pas la BCE.** L'origine le dit, plutôt que de faire passer
    /// une lecture de documents pour une interrogation de registre : le jour où
    /// l'adaptateur BCE existera, la différence devra rester lisible dans les
    /// dossiers déjà validés.
    RevueOps,
    /// Aucun contrôle. Réservé aux jeux de démonstration, et conservé en base
    /// pour qu'un prestataire non contrôlé reste identifiable après coup.
    Demonstration,
}

impl OrigineKyc {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bce => "BCE",
            Self::RevueOps => "OPS_REVIEW",
            Self::Demonstration => "DEMONSTRATION",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "BCE" => Some(Self::Bce),
            "OPS_REVIEW" => Some(Self::RevueOps),
            "DEMONSTRATION" => Some(Self::Demonstration),
            _ => None,
        }
    }
}

impl PreuveKyc {
    /// Preuve d'un contrôle réel auprès de la BCE.
    ///
    /// **Aucun appelant à ce jour** : l'adaptateur qui interrogerait la BCE
    /// n'existe pas. La fonction est là pour que le jour où il existera, rien
    /// d'autre n'ait à changer.
    pub fn depuis_verification_bce(verifie_le: DateTime<Utc>) -> Self {
        Self {
            origine: OrigineKyc::Bce,
            verifie_le,
        }
    }

    /// Preuve d'un examen par l'exploitation (FR-038).
    ///
    /// **Un humain a lu les pièces.** C'est moins qu'une interrogation de la
    /// BCE et bien plus que rien ; l'origine le dit, et les dossiers validés
    /// ainsi resteront distinguables quand l'adaptateur BCE arrivera.
    pub fn depuis_revue_ops(verifie_le: DateTime<Utc>) -> Self {
        Self {
            origine: OrigineKyc::RevueOps,
            verifie_le,
        }
    }

    /// Preuve de démonstration : **aucun contrôle n'a eu lieu**.
    ///
    /// Son nom est son avertissement. Elle laisse une trace en base
    /// (`origine_kyc = 'DEMONSTRATION'`), pour qu'un prestataire non contrôlé
    /// reste reconnaissable longtemps après que la commande qui l'a créé a été
    /// oubliée.
    pub fn demonstration(verifie_le: DateTime<Utc>) -> Self {
        Self {
            origine: OrigineKyc::Demonstration,
            verifie_le,
        }
    }

    pub fn origine(&self) -> OrigineKyc {
        self.origine
    }

    pub fn verifie_le(&self) -> DateTime<Utc> {
        self.verifie_le
    }
}

// `Eq` tombe avec l'arrivée d'un `f64` : `NaN` n'est égal à rien, pas même à
// lui-même, et le prétendre serait faux. `PartialEq` suffit aux comparaisons
// que les tests et les routes font réellement.
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderError {
    RaisonSocialeVide,
    RaisonSocialeTropLongue {
        longueur: usize,
    },
    AucuneCompetence,
    /// Rayon d'intervention hors des bornes utiles (Story 3.7).
    RayonHorsBornes {
        metres: f64,
    },
}

impl ProviderError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::RaisonSocialeVide => "COMPANY_NAME_EMPTY",
            Self::RaisonSocialeTropLongue { .. } => "COMPANY_NAME_TOO_LONG",
            Self::AucuneCompetence => "SKILLS_REQUIRED",
            Self::RayonHorsBornes { .. } => "SERVICE_RADIUS_OUT_OF_RANGE",
        }
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RaisonSocialeVide => write!(f, "raison sociale vide"),
            Self::RaisonSocialeTropLongue { longueur } => write!(
                f,
                "raison sociale de {longueur} caractères, maximum {RAISON_SOCIALE_MAX}"
            ),
            Self::AucuneCompetence => write!(f, "au moins une compétence est requise"),
            Self::RayonHorsBornes { metres } => write!(
                f,
                "rayon de {metres} m, attendu entre {RAYON_INTERVENTION_MIN} et {RAYON_INTERVENTION_MAX}"
            ),
        }
    }
}

impl std::error::Error for ProviderError {}

/// Rayon d'intervention minimal qu'un prestataire peut se fixer, en mètres.
///
/// En dessous d'un kilomètre, il ne serait trouvé par presque personne et
/// conclurait que le service ne marche pas. Mieux vaut refuser le réglage que
/// livrer un compte qui semble fonctionner et ne sonne jamais.
pub const RAYON_INTERVENTION_MIN: f64 = 1_000.0;

/// Rayon d'intervention maximal, en mètres.
///
/// Vingt kilomètres depuis n'importe quel point de la Région de
/// Bruxelles-Capitale la couvrent entièrement : au-delà, le réglage
/// n'atteindrait personne de plus et donnerait l'illusion d'un choix.
pub const RAYON_INTERVENTION_MAX: f64 = 20_000.0;

/// Rayon d'intervention par défaut, en mètres.
///
/// Le maximum, et non une valeur médiane : par défaut, le prestataire ne
/// s'impose aucune limite propre, et c'est le rayon du tour de diffusion qui
/// décide seul. Un défaut plus serré retirerait silencieusement du service des
/// prestataires qui n'ont rien demandé.
pub const RAYON_INTERVENTION_DEFAUT: f64 = RAYON_INTERVENTION_MAX;

#[derive(Debug, Clone, PartialEq)]
pub struct Provider {
    pub id: Uuid,
    /// Compte utilisateur du prestataire : il se connecte comme tout le monde.
    pub utilisateur_id: Uuid,
    pub numero_bce: NumeroBce,
    pub raison_sociale: String,
    /// Point de départ des interventions, d'où se calcule la distance.
    pub base: Geo,
    pub statut: StatutProvider,
    /// Comment son statut a été obtenu.
    pub origine_kyc: Option<OrigineKyc>,
    /// Quand il l'a été. FR-012 en fait un critère de score : un contrôle
    /// vieux d'un an ne dit plus grand-chose de l'état de l'entreprise.
    pub kyc_verifie_le: Option<DateTime<Utc>>,
    /// Secteurs dans lesquels il intervient.
    pub competences: Vec<CodeCatalogue>,
    /// En service ou en pause.
    ///
    /// Distinct du statut : « je suis en congé » n'est pas une radiation, et
    /// confondre les deux ferait d'une pause une sanction.
    pub disponible: bool,
    /// Distance au-delà de laquelle il ne se déplace pas, en mètres.
    ///
    /// C'est **sa** limite, indépendante du rayon du tour de diffusion. Les
    /// deux s'appliquent : le tour dit jusqu'où la Demande cherche, celui-ci
    /// dit jusqu'où le prestataire accepte d'aller.
    pub rayon_intervention_metres: f64,
    pub cree_le: DateTime<Utc>,
}

impl Provider {
    /// Inscrit un prestataire, en attente de contrôle.
    ///
    /// Aucun paramètre ne permet de le créer actif : c'est l'invariant de
    /// FR-003, et le seul chemin vers `ACTIVE` passe par `valider_kyc`, qui
    /// réclame une `PreuveKyc`.
    pub fn inscrire(
        utilisateur_id: Uuid,
        numero_bce: NumeroBce,
        raison_sociale: &str,
        base: Geo,
        competences: Vec<CodeCatalogue>,
        maintenant: DateTime<Utc>,
    ) -> Result<Self, ProviderError> {
        let raison = raison_sociale.trim();
        if raison.is_empty() {
            return Err(ProviderError::RaisonSocialeVide);
        }
        let longueur = raison.chars().count();
        if longueur > RAISON_SOCIALE_MAX {
            return Err(ProviderError::RaisonSocialeTropLongue { longueur });
        }
        // Un prestataire sans compétence ne reçoit rien : l'inscrire ainsi
        // produirait un compte qui semble fonctionner et qui n'est jamais
        // sollicité, ce que personne ne rattache à sa cause.
        if competences.is_empty() {
            return Err(ProviderError::AucuneCompetence);
        }

        let mut competences = competences;
        competences.sort();
        competences.dedup();

        Ok(Self {
            id: Uuid::new_v4(),
            utilisateur_id,
            numero_bce,
            raison_sociale: raison.to_string(),
            base,
            statut: StatutProvider::EnAttenteKyc,
            origine_kyc: None,
            kyc_verifie_le: None,
            competences,
            // Un prestataire fraîchement inscrit n'est pas en service : il
            // attend son contrôle, et le mettre en service lui promettrait des
            // Demandes qu'il ne recevra pas.
            disponible: false,
            rayon_intervention_metres: RAYON_INTERVENTION_DEFAUT,
            cree_le: maintenant,
        })
    }

    /// Active le prestataire, sur preuve d'un contrôle.
    ///
    /// Le type de la preuve est ce qui rend l'activation traçable : `origine`
    /// est conservée, si bien qu'un prestataire actif sans contrôle réel se
    /// retrouve par une requête, longtemps après.
    pub fn valider_kyc(&mut self, preuve: PreuveKyc) {
        self.statut = StatutProvider::Actif;
        self.origine_kyc = Some(preuve.origine());
        self.kyc_verifie_le = Some(preuve.verifie_le());
    }

    pub fn suspendre(&mut self) {
        self.statut = StatutProvider::Suspendu;
    }

    /// Se met en service ou en pause (Story 3.7).
    ///
    /// N'agit que sur la disponibilité, jamais sur le statut : une pause n'est
    /// pas une radiation, et un prestataire suspendu qui se remet « en
    /// service » ne redevient pas sollicitable pour autant. C'est
    /// `peut_etre_sollicite` qui combine les deux.
    pub fn definir_disponibilite(&mut self, disponible: bool) {
        self.disponible = disponible;
    }

    /// Fixe la distance au-delà de laquelle il ne se déplace pas.
    pub fn definir_rayon_intervention(&mut self, metres: f64) -> Result<(), ProviderError> {
        if !metres.is_finite()
            || !(RAYON_INTERVENTION_MIN..=RAYON_INTERVENTION_MAX).contains(&metres)
        {
            return Err(ProviderError::RayonHorsBornes { metres });
        }
        self.rayon_intervention_metres = metres;
        Ok(())
    }

    /// Vrai si une Demande à cette distance entre dans son rayon.
    ///
    /// Inclusif au bord, comme le rayon du tour : quelqu'un qui annonce dix
    /// kilomètres accepte une Demande à dix kilomètres.
    pub fn se_deplace_jusqu_a(&self, distance_metres: f64) -> bool {
        distance_metres <= self.rayon_intervention_metres
    }

    /// Vrai si le prestataire peut recevoir des Demandes.
    ///
    /// Le statut **et** la disponibilité : un prestataire actif mais en pause
    /// ne reçoit rien, et un prestataire suspendu qui se déclare en service non
    /// plus. L'occupation, elle, n'est pas ici — elle se lit dans les Missions
    /// en cours, que le domaine ne détient pas.
    pub fn peut_etre_sollicite(&self) -> bool {
        self.statut == StatutProvider::Actif && self.disponible
    }

    pub fn couvre(&self, secteur: &CodeCatalogue) -> bool {
        self.competences.contains(secteur)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn instant() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    fn bce() -> NumeroBce {
        let corps = 1234567u64;
        NumeroBce::parse(&format!("{corps:08}{:02}", 97 - (corps % 97))).unwrap()
    }

    fn secteur(code: &str) -> CodeCatalogue {
        CodeCatalogue::parse(code).unwrap()
    }

    fn prestataire(competences: Vec<CodeCatalogue>) -> Result<Provider, ProviderError> {
        Provider::inscrire(
            Uuid::new_v4(),
            bce(),
            "Plomberie Dupont SRL",
            Geo::new(50.8467, 4.3525).unwrap(),
            competences,
            instant(),
        )
    }

    #[test]
    fn happy_un_prestataire_neuf_attend_son_controle() {
        let p = prestataire(vec![secteur("plomberie")]).unwrap();
        assert_eq!(p.statut.as_str(), "PENDING_KYC");
        assert_eq!(p.origine_kyc, None);
        assert!(!p.peut_etre_sollicite());
    }

    #[test]
    fn happy_la_validation_active_et_garde_son_origine() {
        let mut p = prestataire(vec![secteur("plomberie")]).unwrap();
        p.valider_kyc(PreuveKyc::depuis_verification_bce(instant()));
        assert_eq!(p.statut, StatutProvider::Actif);
        // Actif ne veut pas dire sollicitable : il faut encore qu'il se mette
        // en service (Story 3.7). Valider un contrôle ne décide pas à sa place
        // qu'il veut travailler tout de suite.
        assert!(!p.peut_etre_sollicite());
        p.definir_disponibilite(true);
        assert!(p.peut_etre_sollicite());
        assert_eq!(p.origine_kyc, Some(OrigineKyc::Bce));
        assert_eq!(p.kyc_verifie_le, Some(instant()));
    }

    #[test]
    fn happy_les_competences_sont_triees_et_dedoublonnees() {
        // Deux fois le même secteur produirait deux notifications pour une
        // seule Demande.
        let p = prestataire(vec![
            secteur("serrurerie"),
            secteur("plomberie"),
            secteur("plomberie"),
        ])
        .unwrap();
        assert_eq!(p.competences.len(), 2);
        assert!(p.couvre(&secteur("plomberie")));
        assert!(p.couvre(&secteur("serrurerie")));
        assert!(!p.couvre(&secteur("auto")));
    }

    #[test]
    fn negative_refuse_une_raison_sociale_vide_ou_trop_longue() {
        let vide = Provider::inscrire(
            Uuid::new_v4(),
            bce(),
            "   ",
            Geo::new(50.8467, 4.3525).unwrap(),
            vec![secteur("plomberie")],
            instant(),
        );
        assert_eq!(vide.unwrap_err().code(), "COMPANY_NAME_EMPTY");

        let longue = Provider::inscrire(
            Uuid::new_v4(),
            bce(),
            &"a".repeat(RAISON_SOCIALE_MAX + 1),
            Geo::new(50.8467, 4.3525).unwrap(),
            vec![secteur("plomberie")],
            instant(),
        );
        assert_eq!(longue.unwrap_err().code(), "COMPANY_NAME_TOO_LONG");
    }

    #[test]
    fn negative_refuse_un_prestataire_sans_competence() {
        // Il ne recevrait rien : son compte semblerait fonctionner sans jamais
        // être sollicité, et personne ne rattacherait le symptôme à sa cause.
        assert_eq!(prestataire(vec![]).unwrap_err().code(), "SKILLS_REQUIRED");
    }

    #[test]
    fn negative_un_statut_inconnu_ne_se_relit_pas() {
        assert_eq!(StatutProvider::parse("VERIFIE"), None);
        assert_eq!(StatutProvider::parse("active"), None);
        assert_eq!(OrigineKyc::parse("bce"), None);
    }

    #[test]
    fn edge_la_raison_sociale_est_debarrassee_de_ses_espaces_de_bord() {
        let p = Provider::inscrire(
            Uuid::new_v4(),
            bce(),
            "  Plomberie Dupont SRL  ",
            Geo::new(50.8467, 4.3525).unwrap(),
            vec![secteur("plomberie")],
            instant(),
        )
        .unwrap();
        assert_eq!(p.raison_sociale, "Plomberie Dupont SRL");
    }

    #[test]
    fn edge_un_prestataire_suspendu_n_est_plus_sollicite() {
        let mut p = prestataire(vec![secteur("plomberie")]).unwrap();
        p.valider_kyc(PreuveKyc::depuis_verification_bce(instant()));
        p.suspendre();
        assert!(!p.peut_etre_sollicite());
        // L'origine du contrôle survit à la suspension : elle dit comment il a
        // été activé, pas s'il l'est encore.
        assert_eq!(p.origine_kyc, Some(OrigineKyc::Bce));
    }

    #[test]
    fn security_aucun_chemin_ne_cree_un_prestataire_deja_actif() {
        // C'est l'invariant de FR-003. Ce test attrape l'ajout d'un paramètre
        // `statut` à `inscrire`, qui est la façon dont ce genre de garde
        // disparaît.
        for competences in [
            vec![secteur("plomberie")],
            vec![secteur("auto"), secteur("livraison")],
        ] {
            let p = prestataire(competences).unwrap();
            assert_eq!(p.statut, StatutProvider::EnAttenteKyc);
            assert!(!p.peut_etre_sollicite());
        }
    }

    #[test]
    fn security_une_activation_de_demonstration_reste_reconnaissable() {
        // Le coeur du dispositif : un prestataire actif sans contrôle réel doit
        // se retrouver par une requête, longtemps après que la commande qui l'a
        // créé a été oubliée.
        let mut p = prestataire(vec![secteur("plomberie")]).unwrap();
        p.valider_kyc(PreuveKyc::demonstration(instant()));
        assert_eq!(p.statut, StatutProvider::Actif);
        assert_eq!(p.origine_kyc, Some(OrigineKyc::Demonstration));
        assert_ne!(p.origine_kyc, Some(OrigineKyc::Bce));
    }

    #[test]
    fn security_la_preuve_ne_se_fabrique_pas_par_un_litteral() {
        // `PreuveKyc` a des champs privés : `PreuveKyc { origine, verifie_le }`
        // ne compile pas hors de ce module. Les deux seules portes sont
        // nommées, et l'une d'elles s'appelle `demonstration`.
        //
        // Ce test ne peut pas vérifier une non-compilation ; il fixe l'intention
        // et échouera si quelqu'un rend les champs publics et change la forme
        // de construction.
        let preuve = PreuveKyc::demonstration(instant());
        assert_eq!(preuve.origine(), OrigineKyc::Demonstration);
        assert_eq!(preuve.verifie_le(), instant());
    }

    #[test]
    fn security_deux_prestataires_ne_partagent_pas_d_identifiant() {
        let a = prestataire(vec![secteur("plomberie")]).unwrap();
        let b = prestataire(vec![secteur("plomberie")]).unwrap();
        assert_ne!(a.id, b.id);
    }
}

#[cfg(test)]
mod tests_disponibilite {
    use super::*;
    use chrono::TimeZone;

    fn instant() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    fn prestataire() -> Provider {
        let corps = 1_234_567u64;
        Provider::inscrire(
            Uuid::new_v4(),
            NumeroBce::parse(&format!("{corps:08}{:02}", 97 - (corps % 97))).unwrap(),
            "Prestataire",
            Geo::new(50.8467, 4.3525).unwrap(),
            vec![CodeCatalogue::parse("plomberie").unwrap()],
            instant(),
        )
        .unwrap()
    }

    fn actif() -> Provider {
        let mut p = prestataire();
        p.valider_kyc(PreuveKyc::demonstration(instant()));
        p.definir_disponibilite(true);
        p
    }

    #[test]
    fn happy_un_prestataire_en_service_est_sollicitable() {
        assert!(actif().peut_etre_sollicite());
    }

    #[test]
    fn happy_la_pause_et_la_reprise_sont_symetriques() {
        let mut p = actif();
        p.definir_disponibilite(false);
        assert!(!p.peut_etre_sollicite());
        p.definir_disponibilite(true);
        assert!(p.peut_etre_sollicite());
    }

    #[test]
    fn happy_le_rayon_par_defaut_ne_limite_rien() {
        // Par défaut le prestataire ne s'impose aucune limite propre : c'est le
        // rayon du tour qui décide seul. Un défaut plus serré retirerait du
        // service des prestataires qui n'ont rien demandé.
        assert_eq!(
            prestataire().rayon_intervention_metres,
            RAYON_INTERVENTION_MAX
        );
        assert!(actif().se_deplace_jusqu_a(RAYON_INTERVENTION_MAX));
    }

    #[test]
    fn happy_le_rayon_se_regle_entre_les_bornes() {
        let mut p = actif();
        for metres in [RAYON_INTERVENTION_MIN, 5_000.0, RAYON_INTERVENTION_MAX] {
            p.definir_rayon_intervention(metres).unwrap();
            assert_eq!(p.rayon_intervention_metres, metres);
        }
    }

    #[test]
    fn negative_un_rayon_hors_bornes_est_refuse() {
        let mut p = actif();
        for metres in [
            0.0,
            -1.0,
            RAYON_INTERVENTION_MIN - 1.0,
            RAYON_INTERVENTION_MAX + 1.0,
        ] {
            assert_eq!(
                p.definir_rayon_intervention(metres).unwrap_err().code(),
                "SERVICE_RADIUS_OUT_OF_RANGE",
                "rayon {metres}"
            );
        }
    }

    #[test]
    fn negative_un_rayon_non_fini_est_refuse() {
        // `f64::NAN` passe silencieusement toutes les comparaisons d'ordre :
        // sans contrôle explicite, il s'écrirait en base et rendrait chaque
        // comparaison de distance fausse.
        let mut p = actif();
        for metres in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(p.definir_rayon_intervention(metres).is_err(), "{metres}");
        }
        assert_eq!(p.rayon_intervention_metres, RAYON_INTERVENTION_DEFAUT);
    }

    #[test]
    fn edge_le_bord_du_rayon_est_inclus() {
        // Quelqu'un qui annonce dix kilomètres accepte une Demande à dix
        // kilomètres, comme le rayon du tour est inclusif à son bord.
        let mut p = actif();
        p.definir_rayon_intervention(10_000.0).unwrap();
        assert!(p.se_deplace_jusqu_a(10_000.0));
        assert!(!p.se_deplace_jusqu_a(10_000.1));
    }

    #[test]
    fn security_un_prestataire_neuf_n_est_pas_en_service() {
        // L'inscrire en service lui promettrait des Demandes qu'il ne recevra
        // pas, puisqu'il attend son contrôle.
        let p = prestataire();
        assert!(!p.disponible);
        assert!(!p.peut_etre_sollicite());
    }

    #[test]
    fn security_un_suspendu_qui_se_declare_en_service_reste_ecarte() {
        // Une pause n'est pas une radiation, et l'inverse n'est pas vrai non
        // plus : se remettre en service ne lève pas une suspension.
        let mut p = actif();
        p.suspendre();
        p.definir_disponibilite(true);
        assert!(!p.peut_etre_sollicite());
    }

    #[test]
    fn security_regler_son_rayon_ne_change_ni_statut_ni_disponibilite() {
        let mut p = actif();
        p.definir_rayon_intervention(3_000.0).unwrap();
        assert_eq!(p.statut, StatutProvider::Actif);
        assert!(p.disponible);
    }
}
