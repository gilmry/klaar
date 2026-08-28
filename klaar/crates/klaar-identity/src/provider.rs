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
}

impl StatutProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::EnAttenteKyc => "PENDING_KYC",
            Self::Actif => "ACTIVE",
            Self::Suspendu => "SUSPENDED",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "PENDING_KYC" => Some(Self::EnAttenteKyc),
            "ACTIVE" => Some(Self::Actif),
            "SUSPENDED" => Some(Self::Suspendu),
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
    /// Aucun contrôle. Réservé aux jeux de démonstration, et conservé en base
    /// pour qu'un prestataire non contrôlé reste identifiable après coup.
    Demonstration,
}

impl OrigineKyc {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bce => "BCE",
            Self::Demonstration => "DEMONSTRATION",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "BCE" => Some(Self::Bce),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    RaisonSocialeVide,
    RaisonSocialeTropLongue { longueur: usize },
    AucuneCompetence,
}

impl ProviderError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::RaisonSocialeVide => "COMPANY_NAME_EMPTY",
            Self::RaisonSocialeTropLongue { .. } => "COMPANY_NAME_TOO_LONG",
            Self::AucuneCompetence => "SKILLS_REQUIRED",
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
        }
    }
}

impl std::error::Error for ProviderError {}

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

    /// Vrai si le prestataire peut recevoir des Demandes.
    pub fn peut_etre_sollicite(&self) -> bool {
        self.statut == StatutProvider::Actif
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
        assert!(p.peut_etre_sollicite());
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
