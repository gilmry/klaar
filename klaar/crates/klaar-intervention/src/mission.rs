//! Agrégat `Mission` (FR-013, Story 3.4).
//!
//! **Ce que cette story livre, et ce qu'elle ne livre pas.** Une Mission naît
//! au moment où un prestataire accepte une Demande, et c'est tout ce qu'elle
//! sait faire ici. La machine à états — en route, sur place, terminée, validée,
//! annulée, replanifiée — appartient à FR-018 et suivants, et rien ne serait
//! gagné à en écrire les transitions avant les stories qui les définissent.
//!
//! Un seul statut, donc, et il est nommé : `ASSIGNED`. Un `enum` à une variante
//! se lit mal, mais il dit la vérité sur ce qui existe, là où un `bool
//! terminee` mentirait déjà sur la suite.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatutMission {
    /// Attribuée à un prestataire, pas encore commencée.
    Attribuee,
}

impl StatutMission {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Attribuee => "ASSIGNED",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "ASSIGNED" => Some(Self::Attribuee),
            _ => None,
        }
    }

    /// Vrai si la Mission occupe encore le prestataire.
    ///
    /// C'est cette notion, et non le statut lui-même, que la règle « une
    /// Mission à la fois » (FR-013 `@edge`) interroge. Elle est ici pour que
    /// l'ajout d'un statut terminal par FR-018 passe par cette fonction plutôt
    /// que par une liste recopiée ailleurs.
    pub fn occupe_le_prestataire(&self) -> bool {
        match self {
            Self::Attribuee => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mission {
    pub id: Uuid,
    /// La Demande dont elle est née. Une Demande donne au plus une Mission :
    /// c'est ce que l'acceptation atomique garantit.
    pub demande_id: Uuid,
    pub provider_id: Uuid,
    pub statut: StatutMission,
    pub cree_le: DateTime<Utc>,
}

impl Mission {
    /// Crée la Mission née d'une acceptation.
    ///
    /// Aucun paramètre ne permet de la créer dans un autre état : une Mission
    /// naît attribuée, comme une Demande naît diffusée.
    pub fn attribuer(demande_id: Uuid, provider_id: Uuid, maintenant: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::new_v4(),
            demande_id,
            provider_id,
            statut: StatutMission::Attribuee,
            cree_le: maintenant,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn instant() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    #[test]
    fn happy_une_mission_nait_attribuee() {
        let m = Mission::attribuer(Uuid::new_v4(), Uuid::new_v4(), instant());
        assert_eq!(m.statut.as_str(), "ASSIGNED");
        assert_eq!(m.cree_le, instant());
    }

    #[test]
    fn happy_le_statut_fait_l_aller_retour() {
        // Un seul statut existe aujourd'hui : la boucle viendra avec FR-018.
        let statut = StatutMission::Attribuee;
        assert_eq!(StatutMission::parse(statut.as_str()), Some(statut));
    }

    #[test]
    fn negative_un_statut_inconnu_ne_se_relit_pas() {
        // Attrape la relecture d'une ligne écrite par une version ultérieure :
        // mieux vaut une erreur que de traiter « terminée » comme « attribuée ».
        for inconnu in ["assigned", "EN_ROUTE", "DONE", ""] {
            assert_eq!(StatutMission::parse(inconnu), None, "statut {inconnu}");
        }
    }

    #[test]
    fn edge_une_mission_attribuee_occupe_son_prestataire() {
        // C'est cette notion qu'interroge la règle « une Mission à la fois ».
        assert!(StatutMission::Attribuee.occupe_le_prestataire());
    }

    #[test]
    fn security_aucun_chemin_ne_cree_une_mission_deja_terminee() {
        // Une Mission naît attribuée. Ce test attrape l'ajout d'un paramètre
        // `statut` à `attribuer`, qui laisserait fabriquer une intervention
        // réputée faite sans que personne ne se soit déplacé.
        let m = Mission::attribuer(Uuid::new_v4(), Uuid::new_v4(), instant());
        assert_eq!(m.statut, StatutMission::Attribuee);
    }

    #[test]
    fn security_deux_missions_ne_partagent_pas_d_identifiant() {
        let demande = Uuid::new_v4();
        let a = Mission::attribuer(demande, Uuid::new_v4(), instant());
        let b = Mission::attribuer(demande, Uuid::new_v4(), instant());
        assert_ne!(a.id, b.id);
    }
}
