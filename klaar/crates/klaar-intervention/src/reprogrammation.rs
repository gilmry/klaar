//! Reprogrammation d'une intervention annulée (FR-023, Story 4.8).
//!
//! **Ce que reprogrammer veut dire ici.** Un prestataire se désiste, mais les
//! deux parties veulent quand même travailler ensemble : plutôt que de refaire
//! une Demande, de rediffuser et de renégocier, on reprend le devis convenu et
//! on repart. C'est le seul cas où cela a du sens — quand l'accord tenait et
//! que seul le moment n'allait pas.
//!
//! **Les deux doivent être d'accord, et c'est le cœur de la story.** Le
//! demandeur propose, le prestataire accepte. Sans le second accord, reprogrammer
//! reviendrait à réattribuer d'office une intervention à quelqu'un qui vient de
//! dire qu'il ne pouvait pas venir.
//!
//! **Sept jours, et c'est fini.** Au-delà, le devis ne vaut plus rien — un prix
//! donné pour une fuite d'il y a une semaine ne dit plus ce que coûte celle
//! d'aujourd'hui — et le demandeur a de toute façon dû trouver quelqu'un.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use crate::annulation::AuteurAnnulation;

/// Fenêtre de reprogrammation après l'annulation, en jours (FR-023 `@edge`).
pub const FENETRE_REPROGRAMMATION_JOURS: i64 = 7;

/// Où en est la proposition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatutReprogrammation {
    /// Proposée par le demandeur, en attente du prestataire.
    Proposee,
    /// Le prestataire a accepté : une nouvelle intervention est née.
    Acceptee,
    /// Le prestataire a refusé (FR-023 `@negative`).
    Refusee,
}

impl StatutReprogrammation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Proposee => "PROPOSED",
            Self::Acceptee => "ACCEPTED",
            Self::Refusee => "DECLINED",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "PROPOSED" => Some(Self::Proposee),
            "ACCEPTED" => Some(Self::Acceptee),
            "DECLINED" => Some(Self::Refusee),
            _ => None,
        }
    }

    pub fn est_close(&self) -> bool {
        match self {
            Self::Proposee => false,
            Self::Acceptee | Self::Refusee => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReprogrammationError {
    /// L'intervention n'a pas été annulée : il n'y a rien à reprendre.
    PasAnnulee,
    /// La fenêtre de sept jours est fermée (FR-023 `@edge`).
    FenetreFermee,
    /// Le prestataire a déjà refusé (FR-023 `@negative`).
    DejaRefusee,
    /// Aucun devis accepté : il n'y a pas d'accord à reprendre.
    SansAccord,
    /// L'annulation vient du demandeur : reprogrammer n'a alors pas de sens.
    AnnuleeParLeDemandeur,
}

impl ReprogrammationError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::PasAnnulee => "MISSION_NOT_CANCELLED",
            Self::FenetreFermee => "RESCHEDULE_EXPIRED",
            Self::DejaRefusee => "PROVIDER_DECLINED",
            Self::SansAccord => "QUOTE_NOT_ACCEPTED",
            Self::AnnuleeParLeDemandeur => "CANCELLED_BY_USER",
        }
    }
}

impl fmt::Display for ReprogrammationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PasAnnulee => write!(f, "cette intervention n'a pas été annulée"),
            Self::FenetreFermee => write!(
                f,
                "une reprogrammation se demande dans les {FENETRE_REPROGRAMMATION_JOURS} jours"
            ),
            Self::DejaRefusee => write!(f, "le prestataire a décliné la reprogrammation"),
            Self::SansAccord => write!(f, "aucun devis accepté à reprendre"),
            Self::AnnuleeParLeDemandeur => write!(
                f,
                "vous avez annulé cette intervention : refaites une demande"
            ),
        }
    }
}

impl std::error::Error for ReprogrammationError {}

/// Une proposition de reprogrammation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reprogrammation {
    pub id: Uuid,
    /// L'intervention annulée qu'on veut reprendre.
    pub mission_id: Uuid,
    /// Le devis dont le prix est repris. Conservé : c'est ce qui distingue une
    /// reprogrammation d'une nouvelle Demande.
    pub devis_id: Uuid,
    pub statut: StatutReprogrammation,
    pub proposee_le: DateTime<Utc>,
}

impl Reprogrammation {
    /// Propose de reprendre une intervention annulée.
    ///
    /// `annulee_le` et `auteur` viennent de l'annulation : c'est d'elle que
    /// court la fenêtre, et c'est elle qui dit si reprogrammer a du sens.
    pub fn proposer(
        mission_id: Uuid,
        devis_id: Uuid,
        auteur: AuteurAnnulation,
        annulee_le: DateTime<Utc>,
        maintenant: DateTime<Utc>,
    ) -> Result<Self, ReprogrammationError> {
        // **Seule une annulation du prestataire se reprogramme.** Un demandeur
        // qui a renoncé et qui change d'avis fait une nouvelle Demande : elle
        // rediffusera, et il trouvera peut-être mieux. Lui offrir de reprendre
        // l'ancien devis le priverait de ce tour.
        if auteur == AuteurAnnulation::Demandeur {
            return Err(ReprogrammationError::AnnuleeParLeDemandeur);
        }
        if maintenant >= echeance_reprogrammation(annulee_le) {
            return Err(ReprogrammationError::FenetreFermee);
        }

        Ok(Self {
            id: Uuid::new_v4(),
            mission_id,
            devis_id,
            statut: StatutReprogrammation::Proposee,
            proposee_le: maintenant,
        })
    }

    /// Le prestataire accepte. Rend `false` si la proposition avait déjà bougé.
    pub fn accepter(&mut self) -> bool {
        if self.statut.est_close() {
            return false;
        }
        self.statut = StatutReprogrammation::Acceptee;
        true
    }

    /// Le prestataire décline.
    pub fn refuser(&mut self) -> bool {
        if self.statut.est_close() {
            return false;
        }
        self.statut = StatutReprogrammation::Refusee;
        true
    }
}

/// Instant où la fenêtre de reprogrammation se ferme.
pub fn echeance_reprogrammation(annulee_le: DateTime<Utc>) -> DateTime<Utc> {
    annulee_le + Duration::days(FENETRE_REPROGRAMMATION_JOURS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t0() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    fn proposer(
        auteur: AuteurAnnulation,
        quand: DateTime<Utc>,
    ) -> Result<Reprogrammation, ReprogrammationError> {
        Reprogrammation::proposer(Uuid::new_v4(), Uuid::new_v4(), auteur, t0(), quand)
    }

    // === @happy ===

    #[test]
    fn happy_une_annulation_du_prestataire_se_reprogramme() {
        let r = proposer(AuteurAnnulation::Prestataire, t0()).unwrap();
        assert_eq!(r.statut, StatutReprogrammation::Proposee);
        assert!(!r.statut.est_close());
    }

    #[test]
    fn happy_le_prestataire_accepte_et_la_proposition_se_ferme() {
        let mut r = proposer(AuteurAnnulation::Prestataire, t0()).unwrap();
        assert!(r.accepter());
        assert_eq!(r.statut, StatutReprogrammation::Acceptee);
        assert!(r.statut.est_close());
    }

    // === @negative ===

    #[test]
    fn negative_une_annulation_du_demandeur_ne_se_reprogramme_pas() {
        // Un demandeur qui a renoncé et qui change d'avis fait une nouvelle
        // Demande : elle rediffusera, et il trouvera peut-être mieux.
        assert_eq!(
            proposer(AuteurAnnulation::Demandeur, t0()),
            Err(ReprogrammationError::AnnuleeParLeDemandeur)
        );
    }

    #[test]
    fn negative_la_fenetre_se_ferme_apres_sept_jours() {
        // FR-023 `@edge` : 410. Un prix donné pour une fuite d'il y a une
        // semaine ne dit plus ce que coûte celle d'aujourd'hui.
        let trop_tard = t0() + Duration::days(FENETRE_REPROGRAMMATION_JOURS);
        assert_eq!(
            proposer(AuteurAnnulation::Prestataire, trop_tard),
            Err(ReprogrammationError::FenetreFermee)
        );
    }

    #[test]
    fn negative_une_proposition_close_ne_se_reprend_pas() {
        let mut r = proposer(AuteurAnnulation::Prestataire, t0()).unwrap();
        assert!(r.refuser());
        assert!(!r.accepter(), "un refus ne se retourne pas");
        assert!(!r.refuser());
        assert_eq!(r.statut, StatutReprogrammation::Refusee);
    }

    // === @edge ===

    #[test]
    fn edge_le_dernier_jour_passe_encore() {
        let juste_avant =
            t0() + Duration::days(FENETRE_REPROGRAMMATION_JOURS) - Duration::seconds(1);
        assert!(proposer(AuteurAnnulation::Prestataire, juste_avant).is_ok());
    }

    #[test]
    fn edge_l_echeance_tombe_a_sept_jours() {
        assert_eq!(
            (echeance_reprogrammation(t0()) - t0()).num_days(),
            FENETRE_REPROGRAMMATION_JOURS
        );
    }

    // === @security ===

    #[test]
    fn security_une_proposition_nait_toujours_en_attente() {
        // Rien ne permet d'en fabriquer une déjà acceptée, ce qui
        // réattribuerait d'office une intervention à quelqu'un qui vient de
        // dire qu'il ne pouvait pas venir.
        let r = proposer(AuteurAnnulation::Prestataire, t0()).unwrap();
        assert_eq!(r.statut, StatutReprogrammation::Proposee);
    }

    #[test]
    fn security_le_devis_repris_est_conserve_sur_la_proposition() {
        // C'est ce qui distingue une reprogrammation d'une nouvelle Demande :
        // le prix a déjà été convenu, et il ne se renégocie pas au passage.
        let devis = Uuid::new_v4();
        let r = Reprogrammation::proposer(
            Uuid::new_v4(),
            devis,
            AuteurAnnulation::Prestataire,
            t0(),
            t0(),
        )
        .unwrap();
        assert_eq!(r.devis_id, devis);
    }

    #[test]
    fn security_le_vocabulaire_est_ferme() {
        for statut in [
            StatutReprogrammation::Proposee,
            StatutReprogrammation::Acceptee,
            StatutReprogrammation::Refusee,
        ] {
            assert_eq!(StatutReprogrammation::parse(statut.as_str()), Some(statut));
        }
        assert_eq!(StatutReprogrammation::parse("PENDING"), None);
    }
}
