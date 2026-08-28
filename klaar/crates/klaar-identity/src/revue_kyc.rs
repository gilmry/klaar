//! Revue du contrôle d'entreprise par l'exploitation (FR-038, Story 8.1).
//!
//! **Valider et refuser ne se ressemblent pas.** Valider laisse entrer une
//! entreprise qui a fourni des pièces ; refuser lui ferme la porte sur la foi
//! d'un jugement. Le premier geste engage un seul examinateur, le second en
//! demande deux (FR-038 `@edge`) — c'est l'asymétrie normale entre ouvrir et
//! fermer.
//!
//! **Un refus sans motif n'existe pas.** Une entreprise refusée doit pouvoir
//! savoir ce qu'on lui reproche, sans quoi elle ne peut ni corriger ni
//! contester. Vingt caractères au moins : « non » n'est pas un motif.

use chrono::{DateTime, Utc};
use std::fmt;
use uuid::Uuid;

use crate::provider::StatutProvider;

/// Motif minimal exigé pour un refus (FR-038 `@negative`).
pub const MOTIF_MIN_CARACTERES: usize = 20;

/// Motif maximal.
pub const MOTIF_MAX_CARACTERES: usize = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionKyc {
    Valider,
    Refuser,
}

impl DecisionKyc {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Valider => "APPROVE",
            Self::Refuser => "REJECT",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "APPROVE" => Some(Self::Valider),
            "REJECT" => Some(Self::Refuser),
            _ => None,
        }
    }

    /// Vrai si la décision demande une seconde paire d'yeux (FR-038 `@edge`).
    ///
    /// **Seul le refus.** Exiger deux examinateurs pour valider doublerait le
    /// délai d'entrée de chaque entreprise honnête pour se prémunir d'un risque
    /// — laisser entrer quelqu'un — que la suspension corrige, alors qu'un
    /// refus injuste ne se corrige pas : l'entreprise est déjà partie.
    pub fn exige_quatre_yeux(&self) -> bool {
        matches!(self, Self::Refuser)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevueError {
    /// Motif absent ou trop court sur un refus.
    MotifRequis,
    MotifTropLong {
        longueur: usize,
    },
    /// Un motif a été fourni sur une validation : l'ignorer laisserait croire
    /// qu'il a été consigné.
    MotifHorsPropos,
    /// L'entreprise n'est plus en attente : déjà traitée, ou retirée.
    PlusEnAttente {
        statut: StatutProvider,
    },
    /// Confirmer son propre refus n'est pas une seconde paire d'yeux.
    MemeExaminateur,
}

impl RevueError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::MotifRequis => "MOTIVE_REQUIRED",
            Self::MotifTropLong { .. } => "MOTIVE_TOO_LONG",
            Self::MotifHorsPropos => "MOTIVE_NOT_APPLICABLE",
            // FR-038 `@edge` nomme ce cas `PROVIDER_CANCELLED` quand
            // l'entreprise s'est retirée ; les autres états déjà traités
            // partagent le même code, parce que la conclusion est la même :
            // il n'y a plus rien à décider.
            Self::PlusEnAttente { statut } => match statut {
                StatutProvider::Retire => "PROVIDER_CANCELLED",
                _ => "REVIEW_ALREADY_DONE",
            },
            Self::MemeExaminateur => "FOUR_EYES_REQUIRED",
        }
    }
}

impl fmt::Display for RevueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MotifRequis => write!(
                f,
                "dites ce qui est reproché, en {MOTIF_MIN_CARACTERES} caractères au moins"
            ),
            Self::MotifTropLong { longueur } => {
                write!(
                    f,
                    "motif de {longueur} caractères, maximum {MOTIF_MAX_CARACTERES}"
                )
            }
            Self::MotifHorsPropos => write!(f, "une validation ne porte pas de motif"),
            Self::PlusEnAttente { statut } => write!(
                f,
                "cette entreprise n'est plus en attente de contrôle ({})",
                statut.as_str()
            ),
            Self::MemeExaminateur => {
                write!(f, "un refus se confirme par un autre compte que le sien")
            }
        }
    }
}

impl std::error::Error for RevueError {}

/// Une revue, telle qu'elle sera consignée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevueKyc {
    pub id: Uuid,
    pub provider_id: Uuid,
    pub decision: DecisionKyc,
    /// Renseigné si et seulement si c'est un refus.
    pub motif: Option<String>,
    pub premier_ops: Uuid,
    pub propose_le: DateTime<Utc>,
    pub second_ops: Option<Uuid>,
    pub confirme_le: Option<DateTime<Utc>>,
}

impl RevueKyc {
    /// Ouvre une revue, ou dit pourquoi elle est refusée.
    ///
    /// Une validation naît **déjà confirmée** : elle n'attend personne. Un refus
    /// naît en attente de sa seconde paire d'yeux.
    pub fn proposer(
        provider_id: Uuid,
        statut_actuel: StatutProvider,
        decision: DecisionKyc,
        motif: Option<&str>,
        ops_id: Uuid,
        maintenant: DateTime<Utc>,
    ) -> Result<Self, RevueError> {
        if statut_actuel != StatutProvider::EnAttenteKyc {
            return Err(RevueError::PlusEnAttente {
                statut: statut_actuel,
            });
        }

        let motif = match (decision, motif.map(str::trim).filter(|m| !m.is_empty())) {
            (DecisionKyc::Valider, None) => None,
            // **Refusé, pas ignoré.** Un motif ignoré sur une validation
            // laisserait son auteur croire qu'il a été consigné, et l'entreprise
            // n'en verrait jamais la trace.
            (DecisionKyc::Valider, Some(_)) => return Err(RevueError::MotifHorsPropos),
            (DecisionKyc::Refuser, None) => return Err(RevueError::MotifRequis),
            (DecisionKyc::Refuser, Some(m)) => {
                let longueur = m.chars().count();
                if longueur < MOTIF_MIN_CARACTERES {
                    return Err(RevueError::MotifRequis);
                }
                if longueur > MOTIF_MAX_CARACTERES {
                    return Err(RevueError::MotifTropLong { longueur });
                }
                Some(m.to_string())
            }
        };

        let confirme_seul = !decision.exige_quatre_yeux();
        Ok(Self {
            id: Uuid::new_v4(),
            provider_id,
            decision,
            motif,
            premier_ops: ops_id,
            propose_le: maintenant,
            second_ops: confirme_seul.then_some(ops_id),
            confirme_le: confirme_seul.then_some(maintenant),
        })
    }

    /// Confirme un refus par une seconde paire d'yeux.
    pub fn confirmer(&mut self, ops_id: Uuid, maintenant: DateTime<Utc>) -> Result<(), RevueError> {
        if self.premier_ops == ops_id {
            return Err(RevueError::MemeExaminateur);
        }
        self.second_ops = Some(ops_id);
        self.confirme_le = Some(maintenant);
        Ok(())
    }

    /// Vrai si la revue a produit son effet.
    pub fn est_close(&self) -> bool {
        self.confirme_le.is_some()
    }

    /// Le statut que prend l'entreprise une fois la revue close.
    ///
    /// `None` tant qu'elle ne l'est pas : une revue en attente ne change rien,
    /// et c'est ce qui distingue « proposé » de « décidé ».
    pub fn statut_resultant(&self) -> Option<StatutProvider> {
        if !self.est_close() {
            return None;
        }
        Some(match self.decision {
            DecisionKyc::Valider => StatutProvider::Actif,
            DecisionKyc::Refuser => StatutProvider::Refuse,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 1, 9, 0, 0).unwrap()
    }

    const MOTIF: &str = "Le numéro d'entreprise ne correspond à aucune inscription active.";

    #[test]
    fn happy_une_validation_prend_effet_seule() {
        let r = RevueKyc::proposer(
            Uuid::new_v4(),
            StatutProvider::EnAttenteKyc,
            DecisionKyc::Valider,
            None,
            Uuid::new_v4(),
            t0(),
        )
        .unwrap();
        assert!(r.est_close(), "valider n'attend personne");
        assert_eq!(r.statut_resultant(), Some(StatutProvider::Actif));
    }

    #[test]
    fn security_un_refus_n_a_aucun_effet_avant_confirmation() {
        let mut r = RevueKyc::proposer(
            Uuid::new_v4(),
            StatutProvider::EnAttenteKyc,
            DecisionKyc::Refuser,
            Some(MOTIF),
            Uuid::new_v4(),
            t0(),
        )
        .unwrap();
        // C'est le cœur des quatre yeux : proposé n'est pas décidé.
        assert!(!r.est_close());
        assert_eq!(r.statut_resultant(), None);

        r.confirmer(Uuid::new_v4(), t0()).unwrap();
        assert!(r.est_close());
        assert_eq!(r.statut_resultant(), Some(StatutProvider::Refuse));
    }

    #[test]
    fn security_on_ne_confirme_pas_son_propre_refus() {
        let moi = Uuid::new_v4();
        let mut r = RevueKyc::proposer(
            Uuid::new_v4(),
            StatutProvider::EnAttenteKyc,
            DecisionKyc::Refuser,
            Some(MOTIF),
            moi,
            t0(),
        )
        .unwrap();
        assert_eq!(r.confirmer(moi, t0()), Err(RevueError::MemeExaminateur));
        assert!(!r.est_close(), "le refus reste en attente");
    }

    #[test]
    fn negative_un_refus_sans_motif_est_impossible() {
        for motif in [None, Some(""), Some("   "), Some("non")] {
            assert_eq!(
                RevueKyc::proposer(
                    Uuid::new_v4(),
                    StatutProvider::EnAttenteKyc,
                    DecisionKyc::Refuser,
                    motif,
                    Uuid::new_v4(),
                    t0(),
                ),
                Err(RevueError::MotifRequis),
                "motif : {motif:?}"
            );
        }
    }

    #[test]
    fn negative_un_motif_sur_une_validation_est_refuse_et_non_ignore() {
        // L'ignorer laisserait son auteur croire qu'il a été consigné.
        assert_eq!(
            RevueKyc::proposer(
                Uuid::new_v4(),
                StatutProvider::EnAttenteKyc,
                DecisionKyc::Valider,
                Some(MOTIF),
                Uuid::new_v4(),
                t0(),
            ),
            Err(RevueError::MotifHorsPropos)
        );
    }

    #[test]
    fn edge_une_entreprise_retiree_ne_se_juge_plus() {
        // FR-038 `@edge` : « Provider annule pendant review ».
        let e = RevueKyc::proposer(
            Uuid::new_v4(),
            StatutProvider::Retire,
            DecisionKyc::Valider,
            None,
            Uuid::new_v4(),
            t0(),
        )
        .unwrap_err();
        assert_eq!(e.code(), "PROVIDER_CANCELLED");
    }

    #[test]
    fn edge_une_entreprise_deja_traitee_donne_un_autre_code() {
        // Le distinguer du retrait : « déjà traitée » et « s'est retirée » ne
        // demandent pas la même suite à celui qui lit l'écran.
        for statut in [
            StatutProvider::Actif,
            StatutProvider::Refuse,
            StatutProvider::Suspendu,
        ] {
            let e = RevueKyc::proposer(
                Uuid::new_v4(),
                statut,
                DecisionKyc::Valider,
                None,
                Uuid::new_v4(),
                t0(),
            )
            .unwrap_err();
            assert_eq!(e.code(), "REVIEW_ALREADY_DONE");
        }
    }

    #[test]
    fn negative_un_motif_trop_long_est_borne() {
        let long = "a".repeat(MOTIF_MAX_CARACTERES + 1);
        assert!(matches!(
            RevueKyc::proposer(
                Uuid::new_v4(),
                StatutProvider::EnAttenteKyc,
                DecisionKyc::Refuser,
                Some(&long),
                Uuid::new_v4(),
                t0(),
            ),
            Err(RevueError::MotifTropLong { .. })
        ));
    }

    #[test]
    fn edge_seul_le_refus_exige_quatre_yeux() {
        assert!(DecisionKyc::Refuser.exige_quatre_yeux());
        assert!(!DecisionKyc::Valider.exige_quatre_yeux());
    }

    #[test]
    fn edge_le_vocabulaire_fait_l_aller_retour() {
        for d in [DecisionKyc::Valider, DecisionKyc::Refuser] {
            assert_eq!(DecisionKyc::parse(d.as_str()), Some(d));
        }
        assert_eq!(DecisionKyc::parse("PEUT_ETRE"), None);
    }
}
