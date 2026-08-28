//! Annulation d'une Mission en cours et ses conséquences (FR-022, Story 4.7).
//!
//! **Ce qui est livré, et ce qui ne l'est pas.** FR-022 fait du remboursement un
//! mouvement Stripe. Le compte n'est pas ouvert : ce module calcule ce qui est
//! dû à qui, et l'enregistre. Le mouvement d'argent rejoindra l'Epic 5, et il
//! lira ces lignes plutôt que de recalculer.
//!
//! **Le forfait de déplacement n'est pas une pénalité.** Quand le prestataire
//! est déjà sur place, il a engagé un trajet et du temps ; les trente euros que
//! FR-022 prévoit couvrent cela, et ils lui reviennent. La pénalité, elle,
//! frappe le prestataire qui se désiste, et elle ne se compte pas en argent
//! mais en compteur — trois désistements en trente jours suspendent.
//!
//! **Une Mission faite ne s'annule pas.** Elle se conteste, et le litige relève
//! de FR-034. Confondre les deux permettrait d'effacer une intervention réelle
//! d'un clic.

use chrono::{DateTime, Utc};
use klaar_shared_kernel::Money;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use crate::mission::StatutMission;

/// Forfait de déplacement dû au prestataire déjà sur place (FR-022 `@negative`).
pub const FORFAIT_DEPLACEMENT_CENTS: i64 = 3_000;

/// Désistements d'un prestataire avant suspension automatique (FR-022 `@edge`).
pub const DESISTEMENTS_AVANT_SUSPENSION: i64 = 3;

/// Fenêtre glissante des désistements du prestataire, en jours.
pub const FENETRE_DESISTEMENTS_JOURS: i64 = 30;

/// Durée de la suspension automatique, en jours (FR-022 `@edge`).
pub const SUSPENSION_JOURS: i64 = 7;

/// Annulations d'un demandeur avant signalement de fraude (FR-022 `@edge`).
pub const ANNULATIONS_AVANT_SIGNALEMENT: i64 = 5;

/// Fenêtre glissante des annulations du demandeur, en jours.
pub const FENETRE_ANNULATIONS_JOURS: i64 = 7;

/// Qui annule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuteurAnnulation {
    Demandeur,
    Prestataire,
}

impl AuteurAnnulation {
    /// Vocabulaire de FR-022, repris tel quel.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Demandeur => "CANCELLED_USER",
            Self::Prestataire => "CANCELLED_PROVIDER",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "CANCELLED_USER" => Some(Self::Demandeur),
            "CANCELLED_PROVIDER" => Some(Self::Prestataire),
            _ => None,
        }
    }
}

/// Pourquoi la Mission est annulée.
///
/// **Vocabulaire fermé**, comme partout ailleurs : un texte libre ne se
/// compterait pas, et FR-022 `@security` demande que le motif serve à des
/// statistiques.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotifAnnulationMission {
    /// Le problème s'est réglé, ou n'existe plus.
    PlusNecessaire,
    /// Le prestataire ne peut plus venir.
    Indisponible,
    /// Les deux parties ne s'entendent pas sur le travail à faire.
    Desaccord,
    /// Impossible d'accéder au lieu.
    AccesImpossible,
    Autre,
}

impl MotifAnnulationMission {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PlusNecessaire => "NO_LONGER_NEEDED",
            Self::Indisponible => "UNAVAILABLE",
            Self::Desaccord => "DISAGREEMENT",
            Self::AccesImpossible => "NO_ACCESS",
            Self::Autre => "OTHER",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "NO_LONGER_NEEDED" => Some(Self::PlusNecessaire),
            "UNAVAILABLE" => Some(Self::Indisponible),
            "DISAGREEMENT" => Some(Self::Desaccord),
            "NO_ACCESS" => Some(Self::AccesImpossible),
            "OTHER" => Some(Self::Autre),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnulationError {
    /// L'intervention est faite : elle se conteste, elle ne s'annule pas.
    MissionTerminee,
    /// Déjà annulée.
    DejaAnnulee,
}

impl AnnulationError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::MissionTerminee => "MISSION_COMPLETED",
            Self::DejaAnnulee => "MISSION_ALREADY_CANCELLED",
        }
    }
}

impl fmt::Display for AnnulationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissionTerminee => write!(
                f,
                "une intervention faite ne s'annule pas : elle se conteste"
            ),
            Self::DejaAnnulee => write!(f, "cette intervention est déjà annulée"),
        }
    }
}

impl std::error::Error for AnnulationError {}

/// Ce que l'annulation coûte, et à qui.
// `Eq` tient : tout est en centimes entiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsequenceAnnulation {
    /// Dû au prestataire pour son déplacement, quand il était déjà sur place.
    pub forfait_deplacement: Money,
    /// Rendu au demandeur.
    pub remboursement: Money,
    /// Vrai si ce désistement compte contre le prestataire.
    pub penalise_le_prestataire: bool,
}

/// L'annulation, telle qu'elle sera consignée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnulationMission {
    pub id: Uuid,
    pub mission_id: Uuid,
    pub auteur: AuteurAnnulation,
    /// Le statut d'où la Mission a été annulée. Conservé : c'est lui qui
    /// justifie le forfait, et le déduire après coup demanderait de relire tout
    /// l'historique.
    pub depuis: StatutMission,
    pub motif: Option<MotifAnnulationMission>,
    pub consequence: ConsequenceAnnulation,
    pub decidee_le: DateTime<Utc>,
}

impl AnnulationMission {
    /// Prononce l'annulation et calcule ses conséquences.
    ///
    /// `engage` est ce que le demandeur devait — le total TTC d'un devis
    /// accepté, ou zéro quand aucun accord n'a été conclu. Passé en paramètre
    /// plutôt que lu ici : ce module ne connaît pas les devis, et lui donner ce
    /// lien ferait dépendre l'intervention du paiement.
    pub fn prononcer(
        mission_id: Uuid,
        statut: StatutMission,
        auteur: AuteurAnnulation,
        motif: Option<MotifAnnulationMission>,
        engage: Money,
        maintenant: DateTime<Utc>,
    ) -> Result<Self, AnnulationError> {
        match statut {
            // Terminée et validée sont hors de portée : l'intervention a eu
            // lieu. FR-022 `@negative` rend 409 et renvoie vers le litige.
            StatutMission::Terminee | StatutMission::Validee => {
                return Err(AnnulationError::MissionTerminee)
            }
            StatutMission::Annulee => return Err(AnnulationError::DejaAnnulee),
            StatutMission::Acceptee | StatutMission::EnRoute | StatutMission::SurPlace => {}
        }

        // Le forfait n'est dû que si le prestataire était **sur place**. En
        // route, il a commencé à se déplacer mais rien ne dit qu'il était
        // arrivé ; le facturer reviendrait à faire payer un trajet qu'on ne
        // peut pas constater.
        let forfait = if statut == StatutMission::SurPlace {
            // Jamais plus que ce qui était engagé : sans cette borne, une
            // annulation sur une Mission sans devis accepté produirait un
            // remboursement négatif, c'est-à-dire une dette inventée.
            FORFAIT_DEPLACEMENT_CENTS.min(engage.cents())
        } else {
            0
        };

        Ok(Self {
            id: Uuid::new_v4(),
            mission_id,
            auteur,
            depuis: statut,
            motif,
            consequence: ConsequenceAnnulation {
                forfait_deplacement: Money::from_cents(forfait),
                remboursement: Money::from_cents(engage.cents() - forfait),
                // Le prestataire qui se désiste est pénalisé, quel que soit
                // l'état : c'est le fait de laisser quelqu'un sans dépanneur qui
                // compte, pas le moment où il le fait.
                penalise_le_prestataire: auteur == AuteurAnnulation::Prestataire,
            },
            decidee_le: maintenant,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t0() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    fn prononcer(
        statut: StatutMission,
        auteur: AuteurAnnulation,
        engage_cents: i64,
    ) -> Result<AnnulationMission, AnnulationError> {
        AnnulationMission::prononcer(
            Uuid::new_v4(),
            statut,
            auteur,
            Some(MotifAnnulationMission::PlusNecessaire),
            Money::from_cents(engage_cents),
            t0(),
        )
    }

    // === @happy ===

    #[test]
    fn happy_le_demandeur_annule_avant_le_depart_et_est_rembourse() {
        let a = prononcer(StatutMission::Acceptee, AuteurAnnulation::Demandeur, 21_780).unwrap();
        assert_eq!(a.consequence.forfait_deplacement.cents(), 0);
        assert_eq!(a.consequence.remboursement.cents(), 21_780);
        assert!(!a.consequence.penalise_le_prestataire);
        assert_eq!(a.auteur.as_str(), "CANCELLED_USER");
    }

    #[test]
    fn happy_le_prestataire_qui_se_desiste_est_penalise() {
        let a = prononcer(
            StatutMission::Acceptee,
            AuteurAnnulation::Prestataire,
            21_780,
        )
        .unwrap();
        assert!(a.consequence.penalise_le_prestataire);
        // Le demandeur récupère tout : ce n'est pas lui qui a renoncé.
        assert_eq!(a.consequence.remboursement.cents(), 21_780);
        assert_eq!(a.auteur.as_str(), "CANCELLED_PROVIDER");
    }

    // === @negative ===

    #[test]
    fn negative_une_intervention_faite_ne_s_annule_pas() {
        // FR-022 `@negative` : 409, et renvoi vers le litige.
        for statut in [StatutMission::Terminee, StatutMission::Validee] {
            assert_eq!(
                prononcer(statut, AuteurAnnulation::Demandeur, 21_780),
                Err(AnnulationError::MissionTerminee),
                "{}",
                statut.as_str()
            );
        }
    }

    #[test]
    fn negative_une_mission_deja_annulee_ne_se_reannule_pas() {
        assert_eq!(
            prononcer(StatutMission::Annulee, AuteurAnnulation::Demandeur, 0),
            Err(AnnulationError::DejaAnnulee)
        );
    }

    // === @edge ===

    #[test]
    fn edge_sur_place_le_forfait_de_deplacement_est_prelevé() {
        // FR-022 `@negative` : trente euros pour le déplacement, le reste rendu.
        let a = prononcer(StatutMission::SurPlace, AuteurAnnulation::Demandeur, 21_780).unwrap();
        assert_eq!(a.consequence.forfait_deplacement.cents(), 3_000);
        assert_eq!(a.consequence.remboursement.cents(), 18_780);
        assert_eq!(
            a.consequence.forfait_deplacement.cents() + a.consequence.remboursement.cents(),
            21_780
        );
    }

    #[test]
    fn edge_en_route_aucun_forfait_n_est_du() {
        // Il a commencé à se déplacer, mais rien ne dit qu'il était arrivé ; le
        // facturer ferait payer un trajet qu'on ne peut pas constater.
        let a = prononcer(StatutMission::EnRoute, AuteurAnnulation::Demandeur, 21_780).unwrap();
        assert_eq!(a.consequence.forfait_deplacement.cents(), 0);
        assert_eq!(a.consequence.remboursement.cents(), 21_780);
    }

    #[test]
    fn edge_sans_devis_accepte_il_n_y_a_rien_a_prelever() {
        // Sans accord de prix, l'annulation ne coûte rien à personne.
        let a = prononcer(StatutMission::SurPlace, AuteurAnnulation::Demandeur, 0).unwrap();
        assert_eq!(a.consequence.forfait_deplacement.cents(), 0);
        assert_eq!(a.consequence.remboursement.cents(), 0);
    }

    #[test]
    fn edge_un_engagement_inferieur_au_forfait_n_est_pas_depasse() {
        // Vingt euros engagés, trente de forfait : le prestataire touche vingt,
        // et le demandeur ne doit rien de plus. Sans cette borne, le
        // remboursement serait négatif, c'est-à-dire une dette inventée.
        let a = prononcer(StatutMission::SurPlace, AuteurAnnulation::Demandeur, 2_000).unwrap();
        assert_eq!(a.consequence.forfait_deplacement.cents(), 2_000);
        assert_eq!(a.consequence.remboursement.cents(), 0);
    }

    // === @security ===

    #[test]
    fn security_la_somme_des_parts_fait_toujours_l_engagement() {
        // L'invariant comptable : rien ne se crée, rien ne disparaît.
        for engage in [0, 1, 1_500, 3_000, 3_001, 21_780, 100_000] {
            for statut in [
                StatutMission::Acceptee,
                StatutMission::EnRoute,
                StatutMission::SurPlace,
            ] {
                let a = prononcer(statut, AuteurAnnulation::Demandeur, engage).unwrap();
                assert_eq!(
                    a.consequence.forfait_deplacement.cents() + a.consequence.remboursement.cents(),
                    engage,
                    "{} à {engage}",
                    statut.as_str()
                );
                assert!(a.consequence.remboursement.cents() >= 0);
            }
        }
    }

    #[test]
    fn security_l_annulation_porte_de_quoi_l_auditer() {
        // FR-022 `@security` : « journalisée avec motif, timestamps, pénalité ».
        let a = prononcer(
            StatutMission::SurPlace,
            AuteurAnnulation::Prestataire,
            21_780,
        )
        .unwrap();
        assert_eq!(a.motif, Some(MotifAnnulationMission::PlusNecessaire));
        assert_eq!(a.decidee_le, t0());
        assert_eq!(a.depuis, StatutMission::SurPlace);
        assert!(a.consequence.penalise_le_prestataire);
    }

    #[test]
    fn security_le_vocabulaire_est_ferme_des_deux_cotes() {
        for auteur in [AuteurAnnulation::Demandeur, AuteurAnnulation::Prestataire] {
            assert_eq!(AuteurAnnulation::parse(auteur.as_str()), Some(auteur));
        }
        for motif in [
            MotifAnnulationMission::PlusNecessaire,
            MotifAnnulationMission::Indisponible,
            MotifAnnulationMission::Desaccord,
            MotifAnnulationMission::AccesImpossible,
            MotifAnnulationMission::Autre,
        ] {
            assert_eq!(MotifAnnulationMission::parse(motif.as_str()), Some(motif));
        }
        assert_eq!(AuteurAnnulation::parse("CANCELLED"), None);
        assert_eq!(MotifAnnulationMission::parse("il m'a mal parlé"), None);
    }

    #[test]
    fn security_seul_le_prestataire_est_penalise_par_son_desistement() {
        // Un demandeur qui annule n'est pas pénalisé ici : sa fraude éventuelle
        // se mesure au compteur, pas à l'annulation isolée.
        for statut in [
            StatutMission::Acceptee,
            StatutMission::EnRoute,
            StatutMission::SurPlace,
        ] {
            assert!(
                !prononcer(statut, AuteurAnnulation::Demandeur, 0)
                    .unwrap()
                    .consequence
                    .penalise_le_prestataire
            );
            assert!(
                prononcer(statut, AuteurAnnulation::Prestataire, 0)
                    .unwrap()
                    .consequence
                    .penalise_le_prestataire
            );
        }
    }
}
