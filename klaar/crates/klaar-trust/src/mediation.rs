//! Médiation d'un litige par l'exploitation (FR-036, Story 7.4).
//!
//! **Trancher est un geste humain ; ce module en pose les bornes.** Il ne
//! décide de rien : il dit ce qu'une décision peut valoir, ce qu'elle entraîne
//! sur l'argent en jeu, et ce qui ne peut pas être décidé deux fois.
//!
//! **Une décision est définitive.** Rouvrir un litige tranché permettrait de
//! revenir sur un remboursement déjà versé, et rendrait la première décision
//! sans valeur pour celui qu'elle a débouté. Le recours après décision n'est
//! pas dans le produit : il est chez le juge, et c'est une limite écrite plutôt
//! que découverte.

use chrono::{DateTime, Duration, Utc};
use std::fmt;

use crate::litige::StatutLitige;

/// Ancienneté au-delà de laquelle un litige non tranché remonte (FR-036
/// `@edge`).
///
/// Trente jours. Ce n'est pas un délai de traitement visé — un litige de trente
/// jours est déjà un échec — mais le seuil à partir duquel personne ne peut
/// plus dire qu'il n'était pas au courant.
pub const ESCALADE_JOURS: i64 = 30;

/// Délai laissé à une partie pour répondre à une demande d'information
/// (FR-036 `@negative`).
///
/// Sept jours. Au-delà, l'exploitation tranche sur les pièces disponibles :
/// attendre indéfiniment la réponse d'une partie donnerait à celle qui se tait
/// un droit de veto sur la décision.
pub const RELANCE_JOURS: i64 = 7;

/// Ce que l'exploitation peut décider.
///
/// **Quatre issues, et un remboursement partiel qui porte son taux.** Sans le
/// taux dans la décision, « partiel » ne voudrait rien dire et le montant
/// rendu dépendrait de qui exécute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Le demandeur est remboursé intégralement.
    PourLeDemandeur,
    /// Le prestataire est payé intégralement.
    PourLePrestataire,
    /// Remboursement partiel, en points de base du total TTC.
    Partiel { part_bp: u16 },
    /// Personne n'a tort : geste commercial, accord amiable.
    SansFaute,
}

/// Part maximale d'un remboursement partiel, en points de base.
///
/// **Strictement sous cent pour cent.** Un « partiel » à 100 % est un
/// remboursement total ; le nommer autrement rendrait les comptages de litiges
/// perdus faux, puisqu'ils se fondent sur le statut.
pub const PART_PARTIELLE_MAX_BP: u16 = 9_900;

/// Part minimale, en points de base. Un partiel à zéro est une décision pour le
/// prestataire, avec un autre nom.
pub const PART_PARTIELLE_MIN_BP: u16 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediationError {
    /// Le litige n'est plus ouvert : il a déjà été tranché.
    DejaTranche,
    /// La part d'un remboursement partiel est hors bornes.
    PartHorsBornes,
    /// L'exploitation ne peut pas trancher un litige qu'elle a ouvert.
    ///
    /// N'arrive pas aujourd'hui — un litige s'ouvre côté demandeur ou
    /// prestataire — mais l'énumérer force à y répondre le jour où
    /// l'exploitation pourra en ouvrir un.
    JugeEtPartie,
}

impl MediationError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::DejaTranche => "DISPUTE_ALREADY_RESOLVED",
            Self::PartHorsBornes => "REFUND_SHARE_OUT_OF_RANGE",
            Self::JugeEtPartie => "MEDIATOR_IS_PARTY",
        }
    }
}

impl fmt::Display for MediationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DejaTranche => write!(f, "ce litige a déjà été tranché"),
            Self::PartHorsBornes => write!(
                f,
                "un remboursement partiel se situe entre {} et {} pour cent",
                PART_PARTIELLE_MIN_BP / 100,
                PART_PARTIELLE_MAX_BP / 100
            ),
            Self::JugeEtPartie => write!(f, "on ne tranche pas un litige dont on est partie"),
        }
    }
}

impl std::error::Error for MediationError {}

/// Ce qu'une décision produit : un statut, et un montant à rendre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Issue {
    pub statut: StatutLitige,
    /// Ce qui revient au demandeur, en centimes.
    pub remboursement_cents: i64,
    /// Ce qui reste au prestataire, en centimes.
    pub reversement_cents: i64,
}

/// Applique une décision à un litige ouvert.
///
/// `total_ttc_cents` est le montant du devis convenu : c'est lui que la décision
/// répartit, et non ce qui a déjà été versé. Un litige tranché après libération
/// donne lieu à un mouvement en sens inverse, ce que ce module ne fait pas —
/// voir la limite écrite dans l'en-tête du cas d'usage.
pub fn trancher(
    statut_actuel: StatutLitige,
    decision: Decision,
    total_ttc_cents: i64,
) -> Result<Issue, MediationError> {
    if statut_actuel != StatutLitige::Ouvert {
        return Err(MediationError::DejaTranche);
    }

    let (statut, remboursement_cents) = match decision {
        Decision::PourLeDemandeur => (StatutLitige::TrancheDemandeur, total_ttc_cents),
        Decision::PourLePrestataire => (StatutLitige::TranchePrestataire, 0),
        Decision::SansFaute => (StatutLitige::Classe, 0),
        Decision::Partiel { part_bp } => {
            if !(PART_PARTIELLE_MIN_BP..=PART_PARTIELLE_MAX_BP).contains(&part_bp) {
                return Err(MediationError::PartHorsBornes);
            }
            // **L'arrondi va au demandeur.** Un centime doit tomber quelque
            // part ; le donner à celui qui conteste plutôt qu'à celui qui est
            // contesté est le choix le moins arbitraire, et il est écrit ici
            // plutôt que laissé au hasard d'une division entière.
            let part = (total_ttc_cents * i64::from(part_bp)).div_euclid(10_000);
            let reste = (total_ttc_cents * i64::from(part_bp)).rem_euclid(10_000);
            let rembourse = if reste > 0 { part + 1 } else { part };
            // Un remboursement partiel tranche **en faveur du demandeur** :
            // c'est lui qui récupère de l'argent, et c'est ce statut que les
            // comptages de FR-035 doivent voir.
            (StatutLitige::TrancheDemandeur, rembourse)
        }
    };

    Ok(Issue {
        statut,
        remboursement_cents,
        // L'invariant comptable : rien ne se crée, rien ne disparaît.
        reversement_cents: total_ttc_cents - remboursement_cents,
    })
}

/// Vrai si un litige ouvert depuis trop longtemps doit remonter (FR-036
/// `@edge`).
pub fn doit_escalader(ouvert_le: DateTime<Utc>, maintenant: DateTime<Utc>) -> bool {
    maintenant >= ouvert_le + Duration::days(ESCALADE_JOURS)
}

/// Échéance de réponse d'une partie relancée (FR-036 `@negative`).
pub fn echeance_relance(relance_le: DateTime<Utc>) -> DateTime<Utc> {
    relance_le + Duration::days(RELANCE_JOURS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 1, 9, 0, 0).unwrap()
    }

    #[test]
    fn happy_une_decision_pour_le_demandeur_rend_tout() {
        let issue = trancher(StatutLitige::Ouvert, Decision::PourLeDemandeur, 21_780).unwrap();
        assert_eq!(issue.statut, StatutLitige::TrancheDemandeur);
        assert_eq!(issue.remboursement_cents, 21_780);
        assert_eq!(issue.reversement_cents, 0);
    }

    #[test]
    fn happy_une_decision_pour_le_prestataire_ne_rend_rien() {
        let issue = trancher(StatutLitige::Ouvert, Decision::PourLePrestataire, 21_780).unwrap();
        assert_eq!(issue.statut, StatutLitige::TranchePrestataire);
        assert_eq!(issue.remboursement_cents, 0);
        assert_eq!(issue.reversement_cents, 21_780);
    }

    #[test]
    fn happy_le_partiel_de_trente_pour_cent_du_scenario_du_prd() {
        // FR-036 `@happy` : « PARTIAL_REFUND 30 % ».
        let issue = trancher(
            StatutLitige::Ouvert,
            Decision::Partiel { part_bp: 3_000 },
            21_780,
        )
        .unwrap();
        assert_eq!(issue.remboursement_cents, 6_534);
        assert_eq!(issue.reversement_cents, 15_246);
        // Un partiel reste une décision en faveur du demandeur : c'est ce que
        // les comptages de sanctions (FR-035) doivent voir.
        assert_eq!(issue.statut, StatutLitige::TrancheDemandeur);
    }

    #[test]
    fn security_rien_ne_se_cree_ni_ne_disparait() {
        // L'invariant comptable, sur toute l'échelle des parts admissibles.
        for part_bp in (PART_PARTIELLE_MIN_BP..=PART_PARTIELLE_MAX_BP).step_by(37) {
            for total in [1, 99, 100, 12_345, 21_780, 1_210_000] {
                let issue =
                    trancher(StatutLitige::Ouvert, Decision::Partiel { part_bp }, total).unwrap();
                assert_eq!(
                    issue.remboursement_cents + issue.reversement_cents,
                    total,
                    "part {part_bp} sur {total}"
                );
                assert!(issue.remboursement_cents >= 0 && issue.reversement_cents >= 0);
            }
        }
    }

    #[test]
    fn edge_l_arrondi_va_au_demandeur() {
        // 33,33 % de 1 centime : la part exacte est 0,003333 centime. Elle est
        // arrondie **vers le haut**, en faveur de celui qui conteste. Un
        // arrondi vers le bas donnerait zéro et transformerait un partiel en
        // décision pour le prestataire.
        let issue = trancher(
            StatutLitige::Ouvert,
            Decision::Partiel { part_bp: 3_333 },
            1,
        )
        .unwrap();
        assert_eq!(issue.remboursement_cents, 1);
        assert_eq!(issue.reversement_cents, 0);
    }

    #[test]
    fn negative_un_litige_deja_tranche_ne_se_retranche_pas() {
        for statut in [
            StatutLitige::TrancheDemandeur,
            StatutLitige::TranchePrestataire,
            StatutLitige::Classe,
        ] {
            assert_eq!(
                trancher(statut, Decision::PourLeDemandeur, 100),
                Err(MediationError::DejaTranche)
            );
        }
    }

    #[test]
    fn negative_une_part_hors_bornes_est_refusee() {
        // 0 % est une décision pour le prestataire, 100 % une décision pour le
        // demandeur. Les laisser passer sous le nom « partiel » fausserait les
        // comptages qui se fondent sur le statut.
        for part_bp in [0, 99, 10_000, u16::MAX] {
            assert_eq!(
                trancher(StatutLitige::Ouvert, Decision::Partiel { part_bp }, 10_000),
                Err(MediationError::PartHorsBornes)
            );
        }
    }

    #[test]
    fn edge_l_escalade_se_declenche_a_trente_jours_pile() {
        let ouvert = t0();
        assert!(!doit_escalader(
            ouvert,
            ouvert + Duration::days(ESCALADE_JOURS) - Duration::seconds(1)
        ));
        assert!(doit_escalader(
            ouvert,
            ouvert + Duration::days(ESCALADE_JOURS)
        ));
    }

    #[test]
    fn edge_la_relance_laisse_sept_jours() {
        assert_eq!(echeance_relance(t0()), t0() + Duration::days(RELANCE_JOURS));
    }

    #[test]
    fn security_un_partiel_ne_rend_jamais_plus_que_le_total() {
        // Le garde-fou qui compte : une part mal lue ne doit pas produire un
        // remboursement supérieur à ce qui a été payé.
        let issue = trancher(
            StatutLitige::Ouvert,
            Decision::Partiel {
                part_bp: PART_PARTIELLE_MAX_BP,
            },
            21_780,
        )
        .unwrap();
        assert!(issue.remboursement_cents < 21_780);
        assert!(issue.reversement_cents > 0);
    }
}
