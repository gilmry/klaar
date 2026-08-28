//! Agrégat `Demande` (FR-011 à FR-015, Story 3.1).

use chrono::{DateTime, Duration, Utc};
use klaar_catalog::CodeCatalogue;
use klaar_shared_kernel::Geo;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use klaar_shared_kernel::dans_le_perimetre;

/// Longueur maximale de la description (FR-011 `@negative`).
///
/// Deux mille caractères suffisent à décrire une fuite ; au-delà, ce n'est plus
/// une description mais un fichier collé, que le prestataire ne lira pas et qui
/// grossit chaque notification envoyée.
pub const DESCRIPTION_MAX: usize = 2_000;

/// Fenêtre pendant laquelle une Demande identique est tenue pour un doublon
/// (FR-011 `@edge`).
///
/// Cinq minutes : le temps d'un double clic, d'un rechargement de page ou d'une
/// requête rejouée par la file hors-ligne. Au-delà, redemander la même chose au
/// même endroit est une intention, pas un accident.
pub const FENETRE_DOUBLON_MINUTES: i64 = 5;

/// Durée d'un tour de diffusion (FR-015 `@happy`).
///
/// **Le PRD se contredit sur ce délai.** FR-013 `@edge` refuse une acceptation
/// « après 5 min », FR-015 `@happy` annonce `NO_MATCH` « après 30 s ». Trente
/// secondes l'emportent, et sans arbitrage véritable : une règle à trente
/// secondes rejette aussi ce que la règle à cinq minutes rejetait, donc elle
/// satisfait les deux scénarios. L'inverse est faux — attendre cinq minutes
/// priverait le demandeur de la réponse que FR-015 lui promet en trente
/// secondes, alors qu'il est devant une fuite.
///
/// Le délai court depuis le **début du tour** et non depuis la création : un
/// élargissement rouvre une fenêtre entière, sans quoi la deuxième chance
/// serait déjà écoulée au moment où on l'offre.
pub const DUREE_DIFFUSION_SECONDES: i64 = 30;

/// Rayons successifs, en mètres (FR-012, FR-015).
///
/// Cinq kilomètres au premier tour, puis dix, quinze, vingt. Vingt kilomètres
/// depuis n'importe quel point de la Région couvrent la Région entière : c'est
/// ce qui borne la liste, et non un chiffre rond. Un cinquième tour
/// n'atteindrait personne de plus.
pub const RAYONS_METRES: [f64; 4] = [5_000.0, 10_000.0, 15_000.0, 20_000.0];

/// Élargissements autorisés (FR-015 `@security`).
///
/// Trois, c'est-à-dire la longueur de `RAYONS_METRES` moins le tour initial.
/// La constante est dérivée plutôt que recopiée : les deux ne peuvent pas
/// diverger.
pub const ELARGISSEMENTS_MAX: u8 = (RAYONS_METRES.len() - 1) as u8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Urgence {
    /// Peut attendre : un robinet qui goutte.
    Basse,
    Normale,
    /// Bloque l'usage du logement ou du véhicule.
    Haute,
}

impl Urgence {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Basse => "LOW",
            Self::Normale => "NORMAL",
            Self::Haute => "HIGH",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "LOW" => Some(Self::Basse),
            "NORMAL" => Some(Self::Normale),
            "HIGH" => Some(Self::Haute),
            _ => None,
        }
    }
}

/// Pourquoi une Demande a été retirée (FR-014 `@security`).
///
/// **Un vocabulaire fermé, et pas un texte libre.** FR-014 veut le motif « pour
/// analytics ». Un champ libre inviterait à écrire « le plombier d'hier était
/// désagréable, j'habite au 12 rue X » : une donnée personnelle non sollicitée,
/// dans un champ dont la finalité annoncée est statistique. Cinq codes servent
/// la même analyse et ne peuvent rien laisser fuir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotifAnnulation {
    /// Le problème s'est réglé tout seul.
    ResoluSeul,
    /// Trop long à venir.
    TropLong,
    /// Quelqu'un a été trouvé ailleurs.
    TrouveAilleurs,
    /// Demande soumise par erreur.
    Erreur,
    Autre,
}

impl MotifAnnulation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ResoluSeul => "RESOLVED_ITSELF",
            Self::TropLong => "TOO_SLOW",
            Self::TrouveAilleurs => "FOUND_ELSEWHERE",
            Self::Erreur => "MISTAKE",
            Self::Autre => "OTHER",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "RESOLVED_ITSELF" => Some(Self::ResoluSeul),
            "TOO_SLOW" => Some(Self::TropLong),
            "FOUND_ELSEWHERE" => Some(Self::TrouveAilleurs),
            "MISTAKE" => Some(Self::Erreur),
            "OTHER" => Some(Self::Autre),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatutDemande {
    /// Diffusée aux prestataires, en attente d'acceptation.
    Diffusion,
    /// Un prestataire l'a acceptée ; une Mission existe (FR-013).
    Attribuee,
    /// Aucun prestataire n'a répondu dans le délai (FR-015).
    SansReponse,
    /// Annulée par le demandeur avant acceptation (FR-014).
    Annulee,
}

impl StatutDemande {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Diffusion => "BROADCASTING",
            Self::Attribuee => "MATCHED",
            Self::SansReponse => "NO_MATCH",
            Self::Annulee => "CANCELLED",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "BROADCASTING" => Some(Self::Diffusion),
            "MATCHED" => Some(Self::Attribuee),
            "NO_MATCH" => Some(Self::SansReponse),
            "CANCELLED" => Some(Self::Annulee),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DemandeError {
    DescriptionVide,
    DescriptionTropLongue {
        longueur: usize,
    },
    UrgenceInvalide,
    HorsPerimetre,
    /// Élargissement demandé sur une Demande qui n'attend pas (FR-015).
    PasSansReponse,
    /// Quatrième élargissement (FR-015 `@security`).
    ElargissementsEpuises,
}

impl DemandeError {
    /// Codes de FR-011 `@negative`, repris tels quels.
    pub fn code(&self) -> &'static str {
        match self {
            Self::DescriptionVide => "DESCRIPTION_EMPTY",
            Self::DescriptionTropLongue { .. } => "DESCRIPTION_TOO_LONG",
            Self::UrgenceInvalide => "URGENCY_INVALID",
            Self::HorsPerimetre => "GEO_OUTSIDE_RBC",
            Self::PasSansReponse => "REQUEST_NOT_EXPIRED",
            Self::ElargissementsEpuises => "MAX_RADIUS_REACHED",
        }
    }
}

impl fmt::Display for DemandeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DescriptionVide => write!(f, "description vide"),
            Self::DescriptionTropLongue { longueur } => {
                write!(
                    f,
                    "description de {longueur} caractères, maximum {DESCRIPTION_MAX}"
                )
            }
            Self::UrgenceInvalide => write!(f, "urgence inconnue"),
            Self::HorsPerimetre => write!(f, "position hors de la Région de Bruxelles-Capitale"),
            Self::PasSansReponse => write!(f, "la Demande n'est pas en attente d'élargissement"),
            Self::ElargissementsEpuises => write!(
                f,
                "rayon maximal atteint après {ELARGISSEMENTS_MAX} élargissements"
            ),
        }
    }
}

impl std::error::Error for DemandeError {}

#[derive(Debug, Clone, PartialEq)]
pub struct Demande {
    pub id: Uuid,
    pub demandeur_id: Uuid,
    pub secteur: CodeCatalogue,
    pub description: String,
    pub position: Geo,
    pub urgence: Urgence,
    pub statut: StatutDemande,
    /// Rayon du tour en cours, en mètres. Change à chaque élargissement.
    pub rayon_metres: f64,
    /// Élargissements déjà consommés (FR-015 `@security`).
    pub elargissements: u8,
    /// Début du tour de diffusion en cours.
    ///
    /// Distinct de `cree_le` : un élargissement rouvre une fenêtre entière, et
    /// la faire courir depuis la création la rendrait déjà écoulée.
    pub diffuse_depuis: DateTime<Utc>,
    /// Renseigné à l'annulation, et seulement si le demandeur en a donné un :
    /// c'est une information qu'il offre, pas une qu'on lui réclame.
    pub motif_annulation: Option<MotifAnnulation>,
    pub cree_le: DateTime<Utc>,
}

impl Demande {
    /// Crée une Demande diffusable.
    ///
    /// L'existence du Secteur n'est **pas** vérifiée ici : le domaine ne connaît
    /// pas le catalogue, qui est un autre bounded context et vit en base. C'est
    /// le cas d'usage qui s'en charge, et qui rend `SECTOR_NOT_FOUND`.
    pub fn soumettre(
        demandeur_id: Uuid,
        secteur: CodeCatalogue,
        description: &str,
        position: Geo,
        urgence: Urgence,
        maintenant: DateTime<Utc>,
    ) -> Result<Self, DemandeError> {
        // `trim` pour le contrôle de vacuité, mais la description est conservée
        // telle quelle : les retours à la ligne d'un utilisateur qui structure
        // son texte font partie de ce qu'il a voulu dire.
        if description.trim().is_empty() {
            return Err(DemandeError::DescriptionVide);
        }
        let longueur = description.chars().count();
        if longueur > DESCRIPTION_MAX {
            return Err(DemandeError::DescriptionTropLongue { longueur });
        }
        if !dans_le_perimetre(position) {
            return Err(DemandeError::HorsPerimetre);
        }

        Ok(Self {
            id: Uuid::new_v4(),
            demandeur_id,
            secteur,
            description: description.to_string(),
            position,
            urgence,
            // Aucun paramètre ne permet de créer une Demande dans un autre
            // état : une Demande naît diffusée, c'est ce qui la définit.
            statut: StatutDemande::Diffusion,
            rayon_metres: RAYONS_METRES[0],
            elargissements: 0,
            diffuse_depuis: maintenant,
            motif_annulation: None,
            cree_le: maintenant,
        })
    }

    /// Vrai si le tour de diffusion en cours est écoulé (FR-015).
    ///
    /// Se calcule à la lecture, à partir de `diffuse_depuis`. Le balayage qui
    /// bascule le statut (`klaar-expirer`) passe périodiquement, donc une
    /// Demande peut être `BROADCASTING` et échue en même temps : c'est cette
    /// méthode qui tranche, et non le statut stocké.
    pub fn est_expiree(&self, maintenant: DateTime<Utc>) -> bool {
        maintenant - self.diffuse_depuis >= Duration::seconds(DUREE_DIFFUSION_SECONDES)
    }

    /// Vrai si un prestataire peut encore l'accepter.
    ///
    /// Deux conditions, et pas une : le statut **et** la fenêtre. Ne vérifier
    /// que le statut laisserait accepter une Demande que le balayage n'a pas
    /// encore éteinte.
    pub fn est_acceptable(&self, maintenant: DateTime<Utc>) -> bool {
        self.statut == StatutDemande::Diffusion && !self.est_expiree(maintenant)
    }

    /// Éteint une Demande dont le tour est écoulé (FR-015 `@happy`).
    ///
    /// Rend `false` si rien n'était à éteindre : le balayage repasse, et une
    /// Demande acceptée entre-temps ne doit pas être défaite par le tour
    /// précédent.
    pub fn expirer(&mut self, maintenant: DateTime<Utc>) -> bool {
        if self.statut != StatutDemande::Diffusion || !self.est_expiree(maintenant) {
            return false;
        }
        self.statut = StatutDemande::SansReponse;
        true
    }

    /// Relance la diffusion sur un rayon plus large (FR-015 `@happy`).
    ///
    /// Ne s'applique qu'à une Demande restée sans réponse : élargir une Demande
    /// encore diffusée couperait son tour en cours, et élargir une Demande
    /// attribuée ou annulée n'a pas de sens.
    ///
    /// Le compteur d'élargissements ne se remet jamais à zéro. C'est ce qui
    /// rend la limite effective : un compteur remis à neuf à chaque tour
    /// laisserait relancer indéfiniment.
    pub fn elargir(&mut self, maintenant: DateTime<Utc>) -> Result<(), DemandeError> {
        if self.statut != StatutDemande::SansReponse {
            return Err(DemandeError::PasSansReponse);
        }
        if self.elargissements >= ELARGISSEMENTS_MAX {
            return Err(DemandeError::ElargissementsEpuises);
        }
        self.elargissements += 1;
        self.rayon_metres = RAYONS_METRES[self.elargissements as usize];
        self.statut = StatutDemande::Diffusion;
        // Fenêtre entière et non reliquat : c'est une nouvelle chance, pas la
        // fin de la précédente.
        self.diffuse_depuis = maintenant;
        Ok(())
    }

    /// Annule la Demande à la demande de son auteur (FR-014, FR-015).
    ///
    /// Rend `false` si elle était déjà attribuée : à ce stade, c'est la Mission
    /// qu'il faut annuler (FR-023), et non la Demande. Annuler celle-ci
    /// laisserait un prestataire en route sans que rien ne le dise.
    ///
    /// Le motif est facultatif : c'est une information que le demandeur offre,
    /// pas une qu'on lui réclame pour lui rendre un droit.
    pub fn annuler(&mut self, motif: Option<MotifAnnulation>) -> bool {
        if self.statut == StatutDemande::Attribuee {
            return false;
        }
        self.statut = StatutDemande::Annulee;
        self.motif_annulation = motif;
        true
    }

    /// Vrai si `autre` est un doublon de celle-ci au sens de FR-011 `@edge`.
    ///
    /// Même demandeur, même secteur, position proche et moins de cinq minutes
    /// d'écart. La description n'entre pas dans la comparaison : quelqu'un qui
    /// reformule sa demande deux minutes plus tard décrit le même problème.
    pub fn est_doublon_de(
        &self,
        demandeur_id: Uuid,
        secteur: &CodeCatalogue,
        position: Geo,
        maintenant: DateTime<Utc>,
    ) -> bool {
        self.statut == StatutDemande::Diffusion
            && self.demandeur_id == demandeur_id
            && &self.secteur == secteur
            && maintenant - self.cree_le < Duration::minutes(FENETRE_DOUBLON_MINUTES)
            && position_proche(self.position, position)
    }
}

/// Tolérance de position pour la détection de doublon, en degrés.
///
/// Environ cent mètres sous nos latitudes. La position d'un téléphone varie de
/// quelques dizaines de mètres d'une mesure à l'autre sans que personne n'ait
/// bougé : exiger l'égalité stricte ne détecterait jamais aucun doublon.
const TOLERANCE_DOUBLON_DEGRES: f64 = 0.001;

fn position_proche(a: Geo, b: Geo) -> bool {
    (a.lat() - b.lat()).abs() < TOLERANCE_DOUBLON_DEGRES
        && (a.lon() - b.lon()).abs() < TOLERANCE_DOUBLON_DEGRES
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn instant() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    fn secteur() -> CodeCatalogue {
        CodeCatalogue::parse("plomberie").unwrap()
    }

    fn bruxelles() -> Geo {
        Geo::new(50.8467, 4.3525).unwrap()
    }

    fn demande(description: &str, position: Geo) -> Result<Demande, DemandeError> {
        Demande::soumettre(
            Uuid::new_v4(),
            secteur(),
            description,
            position,
            Urgence::Haute,
            instant(),
        )
    }

    #[test]
    fn happy_une_demande_valide_nait_en_diffusion() {
        let d = demande("Fuite sous l'évier", bruxelles()).unwrap();
        assert_eq!(d.statut.as_str(), "BROADCASTING");
        assert_eq!(d.urgence.as_str(), "HIGH");
        assert_eq!(d.cree_le, instant());
    }

    #[test]
    fn happy_les_trois_urgences_font_l_aller_retour() {
        for urgence in [Urgence::Basse, Urgence::Normale, Urgence::Haute] {
            assert_eq!(Urgence::parse(urgence.as_str()), Some(urgence));
        }
    }

    #[test]
    fn negative_refuse_une_description_vide() {
        assert_eq!(
            demande("", bruxelles()).unwrap_err().code(),
            "DESCRIPTION_EMPTY"
        );
        assert_eq!(
            demande("   \n\t ", bruxelles()).unwrap_err().code(),
            "DESCRIPTION_EMPTY"
        );
    }

    #[test]
    fn negative_refuse_une_description_trop_longue() {
        let e = demande(&"a".repeat(DESCRIPTION_MAX + 1), bruxelles()).unwrap_err();
        assert_eq!(e.code(), "DESCRIPTION_TOO_LONG");
        assert!(demande(&"a".repeat(DESCRIPTION_MAX), bruxelles()).is_ok());
    }

    #[test]
    fn negative_refuse_une_position_hors_region() {
        let anvers = Geo::new(51.2194, 4.4025).unwrap();
        assert_eq!(
            demande("Fuite", anvers).unwrap_err().code(),
            "GEO_OUTSIDE_RBC"
        );
    }

    #[test]
    fn negative_une_urgence_inconnue_ne_se_relit_pas() {
        assert_eq!(Urgence::parse("URGENT"), None);
        assert_eq!(Urgence::parse("high"), None);
        assert_eq!(Urgence::parse(""), None);
    }

    #[test]
    fn edge_la_longueur_se_compte_en_caracteres_et_non_en_octets() {
        // Deux mille caractères accentués font plus de deux mille octets :
        // compter les octets refuserait une description parfaitement valable.
        let accents = "é".repeat(DESCRIPTION_MAX);
        assert!(accents.len() > DESCRIPTION_MAX);
        assert!(demande(&accents, bruxelles()).is_ok());
    }

    #[test]
    fn edge_la_description_est_conservee_telle_quelle() {
        // Les retours à la ligne d'un utilisateur qui structure son texte font
        // partie de ce qu'il a voulu dire.
        let texte = "Fuite sous l'évier.\n\nDepuis hier soir.\n- goutte à goutte\n- flaque";
        assert_eq!(demande(texte, bruxelles()).unwrap().description, texte);
    }

    #[test]
    fn edge_un_doublon_est_reconnu_dans_les_cinq_minutes() {
        let premiere = demande("Fuite", bruxelles()).unwrap();
        assert!(premiere.est_doublon_de(
            premiere.demandeur_id,
            &secteur(),
            bruxelles(),
            instant() + Duration::minutes(2)
        ));
    }

    #[test]
    fn edge_au_dela_de_cinq_minutes_ce_n_est_plus_un_doublon() {
        // Redemander la même chose au même endroit une heure plus tard est une
        // intention, pas un accident.
        let premiere = demande("Fuite", bruxelles()).unwrap();
        assert!(!premiere.est_doublon_de(
            premiere.demandeur_id,
            &secteur(),
            bruxelles(),
            instant() + Duration::minutes(FENETRE_DOUBLON_MINUTES)
        ));
    }

    #[test]
    fn edge_une_position_a_quelques_metres_reste_un_doublon() {
        // La position d'un téléphone varie de quelques dizaines de mètres d'une
        // mesure à l'autre sans que personne n'ait bougé : exiger l'égalité
        // stricte ne détecterait jamais aucun doublon.
        let premiere = demande("Fuite", bruxelles()).unwrap();
        let a_cote = Geo::new(50.8467 + 0.0005, 4.3525 - 0.0005).unwrap();
        assert!(premiere.est_doublon_de(
            premiere.demandeur_id,
            &secteur(),
            a_cote,
            instant() + Duration::minutes(1)
        ));
    }

    #[test]
    fn edge_la_description_n_entre_pas_dans_la_comparaison_de_doublon() {
        // Quelqu'un qui reformule sa demande deux minutes plus tard décrit le
        // même problème, pas un nouveau.
        let premiere = demande("Fuite", bruxelles()).unwrap();
        assert!(premiere.est_doublon_de(
            premiere.demandeur_id,
            &secteur(),
            bruxelles(),
            instant() + Duration::minutes(2)
        ));
    }

    #[test]
    fn security_la_demande_d_un_autre_n_est_jamais_un_doublon() {
        // Deux voisins qui appellent un plombier à la même minute ont chacun
        // leur fuite. Confondre leurs Demandes en priverait un.
        let premiere = demande("Fuite", bruxelles()).unwrap();
        assert!(!premiere.est_doublon_de(
            Uuid::new_v4(),
            &secteur(),
            bruxelles(),
            instant() + Duration::minutes(1)
        ));
    }

    #[test]
    fn security_un_autre_secteur_n_est_jamais_un_doublon() {
        // Une fuite et une porte claquée le même soir sont deux problèmes.
        let premiere = demande("Fuite", bruxelles()).unwrap();
        assert!(!premiere.est_doublon_de(
            premiere.demandeur_id,
            &CodeCatalogue::parse("serrurerie").unwrap(),
            bruxelles(),
            instant() + Duration::minutes(1)
        ));
    }

    #[test]
    fn security_une_demande_annulee_ne_bloque_pas_la_suivante() {
        // Sinon, annuler puis resoumettre serait impossible pendant cinq
        // minutes, et l'utilisateur ne comprendrait pas pourquoi.
        let mut premiere = demande("Fuite", bruxelles()).unwrap();
        premiere.statut = StatutDemande::Annulee;
        assert!(!premiere.est_doublon_de(
            premiere.demandeur_id,
            &secteur(),
            bruxelles(),
            instant() + Duration::minutes(1)
        ));
    }

    #[test]
    fn security_aucun_chemin_ne_cree_une_demande_deja_acceptee() {
        // Une Demande naît diffusée, c'est ce qui la définit. Ce test attrape
        // l'ajout d'un paramètre `statut` à `soumettre`.
        for urgence in [Urgence::Basse, Urgence::Normale, Urgence::Haute] {
            let d = Demande::soumettre(
                Uuid::new_v4(),
                secteur(),
                "Fuite",
                bruxelles(),
                urgence,
                instant(),
            )
            .unwrap();
            assert_eq!(d.statut, StatutDemande::Diffusion);
        }
    }

    #[test]
    fn happy_une_demande_fraiche_est_acceptable() {
        let d = demande("Fuite", bruxelles()).unwrap();
        assert!(d.est_acceptable(instant()));
        assert!(d.est_acceptable(instant() + Duration::seconds(29)));
        assert!(!d.est_expiree(instant() + Duration::seconds(29)));
    }

    #[test]
    fn happy_une_demande_nait_au_premier_rayon_sans_elargissement() {
        let d = demande("Fuite", bruxelles()).unwrap();
        assert_eq!(d.rayon_metres, RAYONS_METRES[0]);
        assert_eq!(d.elargissements, 0);
        assert_eq!(d.diffuse_depuis, d.cree_le);
    }

    #[test]
    fn happy_un_elargissement_relance_la_diffusion_sur_un_rayon_plus_large() {
        let mut d = demande("Fuite", bruxelles()).unwrap();
        d.expirer(instant() + Duration::seconds(30));
        let plus_tard = instant() + Duration::seconds(40);
        d.elargir(plus_tard).unwrap();
        assert_eq!(d.statut, StatutDemande::Diffusion);
        assert_eq!(d.rayon_metres, RAYONS_METRES[1]);
        assert_eq!(d.elargissements, 1);
        // Fenêtre entière et non reliquat : c'est une nouvelle chance.
        assert_eq!(d.diffuse_depuis, plus_tard);
        assert!(d.est_acceptable(plus_tard));
    }

    #[test]
    fn happy_les_trois_elargissements_parcourent_toute_l_echelle() {
        let mut d = demande("Fuite", bruxelles()).unwrap();
        let mut t = instant();
        for (tour, attendu) in RAYONS_METRES.iter().enumerate().skip(1) {
            t += Duration::seconds(30);
            assert!(d.expirer(t), "tour {tour}");
            d.elargir(t).unwrap();
            assert_eq!(d.rayon_metres, *attendu, "tour {tour}");
        }
        // Vingt kilomètres depuis n'importe quel point couvrent la Région.
        assert_eq!(d.rayon_metres, *RAYONS_METRES.last().unwrap());
    }

    #[test]
    fn negative_un_quatrieme_elargissement_est_refuse() {
        let mut d = demande("Fuite", bruxelles()).unwrap();
        let mut t = instant();
        for _ in 0..ELARGISSEMENTS_MAX {
            t += Duration::seconds(30);
            d.expirer(t);
            d.elargir(t).unwrap();
        }
        t += Duration::seconds(30);
        d.expirer(t);
        assert_eq!(d.elargir(t).unwrap_err().code(), "MAX_RADIUS_REACHED");
    }

    #[test]
    fn negative_on_n_elargit_pas_une_demande_encore_diffusee() {
        // Cela couperait le tour en cours, alors qu'un prestataire est peut-être
        // en train de répondre.
        let mut d = demande("Fuite", bruxelles()).unwrap();
        assert_eq!(
            d.elargir(instant()).unwrap_err().code(),
            "REQUEST_NOT_EXPIRED"
        );
    }

    #[test]
    fn negative_on_n_elargit_ni_une_demande_attribuee_ni_une_annulee() {
        for statut in [StatutDemande::Attribuee, StatutDemande::Annulee] {
            let mut d = demande("Fuite", bruxelles()).unwrap();
            d.statut = statut;
            assert_eq!(
                d.elargir(instant()).unwrap_err().code(),
                "REQUEST_NOT_EXPIRED",
                "statut {statut:?}"
            );
        }
    }

    #[test]
    fn edge_un_balayage_n_eteint_pas_une_demande_encore_dans_sa_fenetre() {
        let mut d = demande("Fuite", bruxelles()).unwrap();
        assert!(!d.expirer(instant() + Duration::seconds(29)));
        assert_eq!(d.statut, StatutDemande::Diffusion);
    }

    #[test]
    fn edge_un_balayage_repasse_sans_defaire_une_demande_attribuee() {
        // Le balayage passe périodiquement : une Demande acceptée entre deux
        // passages ne doit pas être éteinte par le tour précédent.
        let mut d = demande("Fuite", bruxelles()).unwrap();
        d.statut = StatutDemande::Attribuee;
        assert!(!d.expirer(instant() + Duration::hours(1)));
        assert_eq!(d.statut, StatutDemande::Attribuee);
    }

    #[test]
    fn security_le_compteur_d_elargissements_ne_se_remet_jamais_a_zero() {
        // Un compteur remis à neuf à chaque tour laisserait relancer
        // indéfiniment, et la limite de FR-015 ne vaudrait rien.
        let mut d = demande("Fuite", bruxelles()).unwrap();
        let mut t = instant();
        for attendu in 1..=ELARGISSEMENTS_MAX {
            t += Duration::seconds(30);
            d.expirer(t);
            d.elargir(t).unwrap();
            assert_eq!(d.elargissements, attendu);
        }
    }

    #[test]
    fn happy_les_cinq_motifs_font_l_aller_retour() {
        for motif in [
            MotifAnnulation::ResoluSeul,
            MotifAnnulation::TropLong,
            MotifAnnulation::TrouveAilleurs,
            MotifAnnulation::Erreur,
            MotifAnnulation::Autre,
        ] {
            assert_eq!(MotifAnnulation::parse(motif.as_str()), Some(motif));
        }
    }

    #[test]
    fn negative_un_motif_inconnu_ne_se_relit_pas() {
        // Le vocabulaire est fermé : un champ libre inviterait à écrire une
        // donnée personnelle dans un champ dont la finalité est statistique.
        for inconnu in ["le plombier etait desagreable", "resolved_itself", ""] {
            assert_eq!(MotifAnnulation::parse(inconnu), None, "motif {inconnu}");
        }
    }

    #[test]
    fn happy_le_motif_est_facultatif() {
        // C'est une information que le demandeur offre, pas une qu'on lui
        // réclame pour lui rendre un droit.
        let mut d = demande("Fuite", bruxelles()).unwrap();
        assert!(d.annuler(None));
        assert_eq!(d.motif_annulation, None);
    }

    #[test]
    fn happy_le_motif_donne_est_conserve() {
        let mut d = demande("Fuite", bruxelles()).unwrap();
        assert!(d.annuler(Some(MotifAnnulation::TrouveAilleurs)));
        assert_eq!(d.motif_annulation, Some(MotifAnnulation::TrouveAilleurs));
    }

    #[test]
    fn security_une_annulation_refusee_n_enregistre_aucun_motif() {
        // Sinon une Demande attribuée porterait le motif d'une annulation qui
        // n'a pas eu lieu, et l'analyse compterait des annulations imaginaires.
        let mut d = demande("Fuite", bruxelles()).unwrap();
        d.statut = StatutDemande::Attribuee;
        assert!(!d.annuler(Some(MotifAnnulation::TropLong)));
        assert_eq!(d.motif_annulation, None);
    }

    #[test]
    fn security_une_demande_attribuee_ne_s_annule_pas() {
        // À ce stade, c'est la Mission qu'il faut annuler (FR-023) : annuler la
        // Demande laisserait un prestataire en route sans que rien ne le dise.
        let mut d = demande("Fuite", bruxelles()).unwrap();
        d.statut = StatutDemande::Attribuee;
        assert!(!d.annuler(Some(MotifAnnulation::Erreur)));
        assert_eq!(d.statut, StatutDemande::Attribuee);
    }

    #[test]
    fn happy_une_demande_diffusee_ou_sans_reponse_s_annule() {
        for statut in [StatutDemande::Diffusion, StatutDemande::SansReponse] {
            let mut d = demande("Fuite", bruxelles()).unwrap();
            d.statut = statut;
            assert!(d.annuler(None), "statut {statut:?}");
            assert_eq!(d.statut, StatutDemande::Annulee);
        }
    }

    #[test]
    fn happy_les_quatre_statuts_font_l_aller_retour() {
        for statut in [
            StatutDemande::Diffusion,
            StatutDemande::Attribuee,
            StatutDemande::SansReponse,
            StatutDemande::Annulee,
        ] {
            assert_eq!(StatutDemande::parse(statut.as_str()), Some(statut));
        }
    }

    #[test]
    fn negative_une_demande_de_plus_de_trente_secondes_n_est_plus_acceptable() {
        // Passé le tour, le demandeur s'est vu proposer d'élargir ou d'annuler :
        // envoyer quelqu'un sur la foi du tour précédent le prendrait de court.
        let d = demande("Fuite", bruxelles()).unwrap();
        let apres = instant() + Duration::seconds(DUREE_DIFFUSION_SECONDES);
        assert!(d.est_expiree(apres));
        assert!(!d.est_acceptable(apres));
    }

    #[test]
    fn negative_une_demande_deja_attribuee_n_est_plus_acceptable() {
        let mut d = demande("Fuite", bruxelles()).unwrap();
        d.statut = StatutDemande::Attribuee;
        assert!(!d.est_acceptable(instant()));
    }

    #[test]
    fn edge_le_statut_seul_ne_suffit_pas_a_dire_qu_une_demande_est_vivante() {
        // Le balayage passe périodiquement : entre deux passages, une Demande
        // échue est encore `BROADCASTING` en base. Ne vérifier que le statut la
        // laisserait accepter.
        let d = demande("Fuite", bruxelles()).unwrap();
        assert_eq!(d.statut, StatutDemande::Diffusion);
        assert!(!d.est_acceptable(instant() + Duration::hours(24)));
    }

    #[test]
    fn security_une_demande_attribuee_ne_bloque_pas_la_suivante() {
        // Même raison qu'une annulée : sinon, le demandeur dont l'intervention
        // vient d'être attribuée ne pourrait rien redemander pendant cinq
        // minutes, sans comprendre pourquoi.
        let mut premiere = demande("Fuite", bruxelles()).unwrap();
        premiere.statut = StatutDemande::Attribuee;
        assert!(!premiere.est_doublon_de(
            premiere.demandeur_id,
            &secteur(),
            bruxelles(),
            instant() + Duration::minutes(1)
        ));
    }

    #[test]
    fn security_deux_demandes_ne_partagent_pas_d_identifiant() {
        let a = demande("Fuite", bruxelles()).unwrap();
        let b = demande("Fuite", bruxelles()).unwrap();
        assert_ne!(a.id, b.id);
    }
}
