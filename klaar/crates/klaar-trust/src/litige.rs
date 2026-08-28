//! Litige sur une intervention (FR-034, Story 7.2) et sanctions (FR-035).
//!
//! **Un litige est l'issue de ce que l'annulation refuse.** Une intervention
//! faite ne s'annule pas — elle a eu lieu — mais elle peut être contestée. Sans
//! ce recours, le seul geste possible après un travail mal fait serait une
//! mauvaise note, ce qui ne rend l'argent à personne.
//!
//! **Les deux parties peuvent ouvrir.** Le demandeur pour la qualité ou le
//! travail non fait ; le prestataire quand personne ne lui a ouvert. Ne donner
//! le recours qu'à l'un des deux ferait de l'autre un justiciable permanent.
//!
//! **Ce qui n'est pas ici.** La résolution appartient à l'exploitation
//! (FR-036) : ce module ouvre, borne et compte. Trancher demande un humain, et
//! une console pour lui — c'est l'Epic 8.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Fenêtre d'ouverture après la fin de l'intervention, en jours (FR-034).
///
/// Quatorze, comme la notation : au-delà, les preuves se sont effacées et les
/// souvenirs divergent, et rouvrir une affaire close depuis un mois ne sert ni
/// l'un ni l'autre.
pub const FENETRE_LITIGE_JOURS: i64 = 14;

/// Description minimale exigée à l'ouverture (FR-034 `@negative`).
pub const DESCRIPTION_MIN_CARACTERES: usize = 20;

/// Description maximale.
pub const DESCRIPTION_MAX_CARACTERES: usize = 2_000;

/// Litiges tranchés en faveur du demandeur avant suspension (FR-035).
pub const LITIGES_AVANT_SUSPENSION: i64 = 3;

/// Fenêtre glissante des litiges perdus par un prestataire, en jours.
pub const FENETRE_LITIGES_JOURS: i64 = 30;

/// Litiges ouverts par un demandeur avant examen pour fraude (FR-034 `@edge`).
pub const LITIGES_AVANT_EXAMEN: i64 = 2;

/// Fenêtre glissante des litiges d'un demandeur, en jours.
pub const FENETRE_DEMANDEUR_JOURS: i64 = 7;

/// Qui ouvre le litige.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PartieLitige {
    Demandeur,
    Prestataire,
}

impl PartieLitige {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Demandeur => "USER",
            Self::Prestataire => "PROVIDER",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "USER" => Some(Self::Demandeur),
            "PROVIDER" => Some(Self::Prestataire),
            _ => None,
        }
    }
}

/// Pourquoi le litige est ouvert.
///
/// **Vocabulaire fermé, et asymétrique.** Les griefs des deux parties ne sont
/// pas les mêmes : le demandeur conteste un travail, le prestataire constate
/// une porte close. Un vocabulaire commun aurait obligé chacun à choisir dans
/// une liste dont la moitié ne le concerne pas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotifLitige {
    /// Travail mal fait ou incomplet (demandeur).
    Qualite,
    /// Rien n'a été fait (demandeur).
    NonFait,
    /// Facturé au-delà de ce qui avait été convenu (demandeur).
    MontantConteste,
    /// Personne n'a ouvert (prestataire).
    AbsenceDemandeur,
    /// Le lieu ne permettait pas d'intervenir (prestataire).
    ConditionsImpossibles,
    Autre,
}

impl MotifLitige {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Qualite => "QUALITY",
            Self::NonFait => "NOT_DONE",
            Self::MontantConteste => "AMOUNT_DISPUTED",
            Self::AbsenceDemandeur => "USER_NO_SHOW",
            Self::ConditionsImpossibles => "IMPOSSIBLE_CONDITIONS",
            Self::Autre => "OTHER",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "QUALITY" => Some(Self::Qualite),
            "NOT_DONE" => Some(Self::NonFait),
            "AMOUNT_DISPUTED" => Some(Self::MontantConteste),
            "USER_NO_SHOW" => Some(Self::AbsenceDemandeur),
            "IMPOSSIBLE_CONDITIONS" => Some(Self::ConditionsImpossibles),
            "OTHER" => Some(Self::Autre),
            _ => None,
        }
    }

    /// Vrai si ce motif peut être invoqué par cette partie.
    ///
    /// **Le contrôle existe pour que les statistiques veuillent dire quelque
    /// chose.** Un prestataire qui ouvrirait un litige « qualité » contre
    /// lui-même rendrait tout comptage par motif ininterprétable.
    pub fn ouvert_a(&self, partie: PartieLitige) -> bool {
        match self {
            Self::Qualite | Self::NonFait | Self::MontantConteste => {
                partie == PartieLitige::Demandeur
            }
            Self::AbsenceDemandeur | Self::ConditionsImpossibles => {
                partie == PartieLitige::Prestataire
            }
            Self::Autre => true,
        }
    }
}

/// Où en est le litige.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatutLitige {
    /// Ouvert, en attente d'examen.
    Ouvert,
    /// Tranché en faveur du demandeur.
    TrancheDemandeur,
    /// Tranché en faveur du prestataire.
    TranchePrestataire,
    /// Clos sans que personne n'ait tort : geste commercial, accord amiable.
    Classe,
}

impl StatutLitige {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ouvert => "OPENED",
            Self::TrancheDemandeur => "RESOLVED_USER_FAVOR",
            Self::TranchePrestataire => "RESOLVED_PROVIDER_FAVOR",
            Self::Classe => "CLOSED_NO_FAULT",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "OPENED" => Some(Self::Ouvert),
            "RESOLVED_USER_FAVOR" => Some(Self::TrancheDemandeur),
            "RESOLVED_PROVIDER_FAVOR" => Some(Self::TranchePrestataire),
            "CLOSED_NO_FAULT" => Some(Self::Classe),
            _ => None,
        }
    }

    pub fn est_clos(&self) -> bool {
        match self {
            Self::Ouvert => false,
            Self::TrancheDemandeur | Self::TranchePrestataire | Self::Classe => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LitigeError {
    /// Description absente ou trop courte (FR-034 `@negative`).
    DescriptionInsuffisante,
    DescriptionTropLongue,
    /// Motif que cette partie ne peut pas invoquer.
    MotifHorsPropos,
    /// La fenêtre de quatorze jours est fermée (FR-034 `@negative`).
    FenetreFermee,
}

impl LitigeError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::DescriptionInsuffisante => "MOTIVE_REQUIRED",
            Self::DescriptionTropLongue => "DESCRIPTION_TOO_LONG",
            Self::MotifHorsPropos => "MOTIVE_NOT_APPLICABLE",
            Self::FenetreFermee => "DISPUTE_WINDOW_CLOSED",
        }
    }
}

impl fmt::Display for LitigeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DescriptionInsuffisante => write!(
                f,
                "dites ce qui s'est passé, en {DESCRIPTION_MIN_CARACTERES} caractères au moins"
            ),
            Self::DescriptionTropLongue => write!(
                f,
                "description au-delà de {DESCRIPTION_MAX_CARACTERES} caractères"
            ),
            Self::MotifHorsPropos => write!(f, "ce motif ne s'applique pas à votre situation"),
            Self::FenetreFermee => write!(
                f,
                "un litige s'ouvre dans les {FENETRE_LITIGE_JOURS} jours qui suivent l'intervention"
            ),
        }
    }
}

impl std::error::Error for LitigeError {}

/// Un litige, tel qu'il sera consigné.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Litige {
    pub id: Uuid,
    pub mission_id: Uuid,
    /// Le compte qui ouvre.
    pub auteur_id: Uuid,
    pub partie: PartieLitige,
    pub motif: MotifLitige,
    pub description: String,
    pub statut: StatutLitige,
    pub ouvert_le: DateTime<Utc>,
}

impl Litige {
    /// Ouvre un litige, ou dit pourquoi il est refusé.
    ///
    /// `close_depuis` est l'instant où l'intervention s'est terminée : c'est de
    /// là que court la fenêtre, et non de la validation qui peut arriver trois
    /// jours plus tard.
    pub fn ouvrir(
        mission_id: Uuid,
        auteur_id: Uuid,
        partie: PartieLitige,
        motif: MotifLitige,
        description: &str,
        close_depuis: DateTime<Utc>,
        maintenant: DateTime<Utc>,
    ) -> Result<Self, LitigeError> {
        if maintenant >= close_depuis + Duration::days(FENETRE_LITIGE_JOURS) {
            return Err(LitigeError::FenetreFermee);
        }
        if !motif.ouvert_a(partie) {
            return Err(LitigeError::MotifHorsPropos);
        }

        // La description est exigée, et pas seulement non vide : « pas content »
        // ne permet à personne de trancher, et FR-034 `@negative` refuse une
        // ouverture « sans motif ni preuve ».
        let description = description.trim();
        let longueur = description.chars().count();
        if longueur < DESCRIPTION_MIN_CARACTERES {
            return Err(LitigeError::DescriptionInsuffisante);
        }
        if longueur > DESCRIPTION_MAX_CARACTERES {
            return Err(LitigeError::DescriptionTropLongue);
        }

        Ok(Self {
            id: Uuid::new_v4(),
            mission_id,
            auteur_id,
            partie,
            motif,
            description: description.to_string(),
            statut: StatutLitige::Ouvert,
            ouvert_le: maintenant,
        })
    }
}

/// Instant où la fenêtre de litige se ferme.
pub fn echeance_litige(close_depuis: DateTime<Utc>) -> DateTime<Utc> {
    close_depuis + Duration::days(FENETRE_LITIGE_JOURS)
}

/// Faut-il suspendre ce prestataire (FR-035 `@happy`) ?
///
/// Trois litiges tranchés en faveur du demandeur en trente jours. Le comptage
/// ne retient que ceux qu'il a **perdus** : un prestataire attaqué trois fois et
/// blanchi trois fois n'a rien fait de mal, et le suspendre reviendrait à
/// punir le fait d'avoir été accusé.
pub fn suspension_meritee(litiges_perdus: i64) -> bool {
    litiges_perdus >= LITIGES_AVANT_SUSPENSION
}

/// Faut-il examiner ce demandeur pour fraude (FR-034 `@edge`) ?
///
/// Deux litiges en sept jours. Ce n'est **pas** une sanction : c'est un signal
/// d'exploitation. Quelqu'un peut légitimement tomber deux fois sur un mauvais
/// prestataire, et le bloquer automatiquement serait le punir pour de la
/// malchance.
pub fn examen_merite(litiges_ouverts: i64) -> bool {
    litiges_ouverts >= LITIGES_AVANT_EXAMEN
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t0() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    const RECIT: &str = "Le joint fuit toujours et la trace d'eau s'est agrandie.";

    fn ouvrir(
        partie: PartieLitige,
        motif: MotifLitige,
        description: &str,
        quand: DateTime<Utc>,
    ) -> Result<Litige, LitigeError> {
        Litige::ouvrir(
            Uuid::new_v4(),
            Uuid::new_v4(),
            partie,
            motif,
            description,
            t0(),
            quand,
        )
    }

    // === @happy ===

    #[test]
    fn happy_le_demandeur_ouvre_pour_qualite() {
        let l = ouvrir(PartieLitige::Demandeur, MotifLitige::Qualite, RECIT, t0()).unwrap();
        assert_eq!(l.statut, StatutLitige::Ouvert);
        assert_eq!(l.motif.as_str(), "QUALITY");
        assert_eq!(l.partie.as_str(), "USER");
    }

    #[test]
    fn happy_le_prestataire_ouvre_pour_porte_close() {
        // FR-034 `@happy` : les deux parties peuvent ouvrir.
        let l = ouvrir(
            PartieLitige::Prestataire,
            MotifLitige::AbsenceDemandeur,
            "Personne n'a ouvert après vingt minutes d'attente.",
            t0(),
        )
        .unwrap();
        assert_eq!(l.motif.as_str(), "USER_NO_SHOW");
    }

    // === @negative ===

    #[test]
    fn negative_une_description_trop_courte_est_refusee() {
        // FR-034 `@negative` : « pas content » ne permet à personne de trancher.
        for maigre in ["", "   ", "pas content", "nul"] {
            assert_eq!(
                ouvrir(PartieLitige::Demandeur, MotifLitige::Qualite, maigre, t0()),
                Err(LitigeError::DescriptionInsuffisante),
                "{maigre}"
            );
        }
    }

    #[test]
    fn negative_la_fenetre_se_ferme_apres_quatorze_jours() {
        // FR-034 `@negative` : 410.
        let trop_tard = t0() + Duration::days(FENETRE_LITIGE_JOURS);
        assert_eq!(
            ouvrir(
                PartieLitige::Demandeur,
                MotifLitige::Qualite,
                RECIT,
                trop_tard
            ),
            Err(LitigeError::FenetreFermee)
        );
    }

    #[test]
    fn negative_un_motif_hors_propos_est_refuse() {
        // Un prestataire ne conteste pas sa propre qualité, et un demandeur ne
        // se reproche pas d'avoir été absent.
        assert_eq!(
            ouvrir(PartieLitige::Prestataire, MotifLitige::Qualite, RECIT, t0()),
            Err(LitigeError::MotifHorsPropos)
        );
        assert_eq!(
            ouvrir(
                PartieLitige::Demandeur,
                MotifLitige::AbsenceDemandeur,
                RECIT,
                t0()
            ),
            Err(LitigeError::MotifHorsPropos)
        );
    }

    // === @edge ===

    #[test]
    fn edge_le_dernier_jour_passe_encore() {
        let juste_avant = t0() + Duration::days(FENETRE_LITIGE_JOURS) - Duration::seconds(1);
        assert!(ouvrir(
            PartieLitige::Demandeur,
            MotifLitige::Qualite,
            RECIT,
            juste_avant
        )
        .is_ok());
    }

    #[test]
    fn edge_le_motif_autre_est_ouvert_aux_deux_parties() {
        for partie in [PartieLitige::Demandeur, PartieLitige::Prestataire] {
            assert!(ouvrir(partie, MotifLitige::Autre, RECIT, t0()).is_ok());
        }
    }

    #[test]
    fn edge_une_description_trop_longue_est_refusee() {
        let refus = ouvrir(
            PartieLitige::Demandeur,
            MotifLitige::Qualite,
            &"x".repeat(DESCRIPTION_MAX_CARACTERES + 1),
            t0(),
        );
        assert_eq!(refus, Err(LitigeError::DescriptionTropLongue));
    }

    #[test]
    fn edge_la_longueur_se_compte_apres_nettoyage() {
        // Vingt espaces autour de « nul » ne font pas une description.
        let refus = ouvrir(
            PartieLitige::Demandeur,
            MotifLitige::Qualite,
            "                    nul                    ",
            t0(),
        );
        assert_eq!(refus, Err(LitigeError::DescriptionInsuffisante));
    }

    // === @security ===

    #[test]
    fn security_un_litige_nait_toujours_ouvert() {
        // Rien dans la signature ne permet d'en fabriquer un déjà tranché, ce
        // qui court-circuiterait l'examen.
        let l = ouvrir(PartieLitige::Demandeur, MotifLitige::Qualite, RECIT, t0()).unwrap();
        assert_eq!(l.statut, StatutLitige::Ouvert);
        assert!(!l.statut.est_clos());
    }

    #[test]
    fn security_la_suspension_ne_compte_que_les_litiges_perdus() {
        // Un prestataire attaqué trois fois et blanchi trois fois n'a rien fait
        // de mal : le suspendre reviendrait à punir le fait d'avoir été accusé.
        assert!(!suspension_meritee(0));
        assert!(!suspension_meritee(LITIGES_AVANT_SUSPENSION - 1));
        assert!(suspension_meritee(LITIGES_AVANT_SUSPENSION));
    }

    #[test]
    fn security_l_examen_d_un_demandeur_n_est_pas_une_sanction() {
        // Quelqu'un peut légitimement tomber deux fois sur un mauvais
        // prestataire. Le seuil lève un signal, il ne bloque rien.
        assert!(!examen_merite(1));
        assert!(examen_merite(LITIGES_AVANT_EXAMEN));
    }

    #[test]
    fn security_le_vocabulaire_est_ferme_et_stable() {
        // Ces codes sortent du service et se retrouvent dans des exports
        // réglementaires.
        for motif in [
            MotifLitige::Qualite,
            MotifLitige::NonFait,
            MotifLitige::MontantConteste,
            MotifLitige::AbsenceDemandeur,
            MotifLitige::ConditionsImpossibles,
            MotifLitige::Autre,
        ] {
            assert_eq!(MotifLitige::parse(motif.as_str()), Some(motif));
        }
        for statut in [
            StatutLitige::Ouvert,
            StatutLitige::TrancheDemandeur,
            StatutLitige::TranchePrestataire,
            StatutLitige::Classe,
        ] {
            assert_eq!(StatutLitige::parse(statut.as_str()), Some(statut));
        }
        assert_eq!(MotifLitige::parse("il m'a mal parlé"), None);
        assert_eq!(StatutLitige::parse("PENDING"), None);
    }

    #[test]
    fn security_la_description_est_conservee_telle_quelle() {
        // Ni échappement ni réécriture : c'est le récit de quelqu'un, et un
        // examen doit lire ce qui a été écrit.
        let hostile = "Le <b>joint</b> fuit & la trace s'est agrandie de 3\" environ.";
        let l = ouvrir(PartieLitige::Demandeur, MotifLitige::Qualite, hostile, t0()).unwrap();
        assert_eq!(l.description, hostile);
    }
}
