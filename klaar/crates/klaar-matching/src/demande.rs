//! Agrégat `Demande` (FR-011 à FR-015, Story 3.1).

use chrono::{DateTime, Duration, Utc};
use klaar_catalog::CodeCatalogue;
use klaar_shared_kernel::Geo;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use crate::perimetre::dans_le_perimetre;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatutDemande {
    /// Diffusée aux prestataires, en attente d'acceptation.
    Diffusion,
    /// Aucun prestataire n'a répondu dans le délai (FR-015).
    SansReponse,
    /// Annulée par le demandeur avant acceptation (FR-014).
    Annulee,
}

impl StatutDemande {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Diffusion => "BROADCASTING",
            Self::SansReponse => "NO_MATCH",
            Self::Annulee => "CANCELLED",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "BROADCASTING" => Some(Self::Diffusion),
            "NO_MATCH" => Some(Self::SansReponse),
            "CANCELLED" => Some(Self::Annulee),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DemandeError {
    DescriptionVide,
    DescriptionTropLongue { longueur: usize },
    UrgenceInvalide,
    HorsPerimetre,
}

impl DemandeError {
    /// Codes de FR-011 `@negative`, repris tels quels.
    pub fn code(&self) -> &'static str {
        match self {
            Self::DescriptionVide => "DESCRIPTION_EMPTY",
            Self::DescriptionTropLongue { .. } => "DESCRIPTION_TOO_LONG",
            Self::UrgenceInvalide => "URGENCY_INVALID",
            Self::HorsPerimetre => "GEO_OUTSIDE_RBC",
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
            cree_le: maintenant,
        })
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
    fn security_deux_demandes_ne_partagent_pas_d_identifiant() {
        let a = demande("Fuite", bruxelles()).unwrap();
        let b = demande("Fuite", bruxelles()).unwrap();
        assert_ne!(a.id, b.id);
    }
}
