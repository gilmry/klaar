//! Événements de Mission diffusés en temps réel (Story 4.9).
//!
//! **Un événement dit qu'il s'est passé quelque chose, pas ce que c'est.** Il
//! porte l'identifiant de la Mission, un genre et un instant — rien d'autre. La
//! description, l'adresse, le nom de l'entreprise et le montant d'un devis
//! restent derrière la session : le destinataire relit ce qu'il a le droit de
//! voir par les routes qui vérifient déjà ses droits.
//!
//! Ce choix n'est pas seulement prudent, il est nécessaire : la charge d'un
//! `NOTIFY` PostgreSQL traverse la base, se retrouve dans ses journaux, et
//! serait diffusée à tous les exemplaires du service. Y mettre une adresse
//! reviendrait à la publier sur un canal que personne n'audite.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Nom du canal `LISTEN`/`NOTIFY`.
///
/// Préfixé, parce qu'une base partagée avec un autre service ferait se
/// mélanger deux flux dont ni l'un ni l'autre ne saurait quoi faire de
/// l'intrus.
pub const CANAL: &str = "klaar_mission";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GenreEvenement {
    /// La Mission a changé d'état (FR-018).
    StatutMission,
    /// Un devis a été émis (FR-016).
    DevisEmis,
    /// Un devis a expiré sans réponse.
    DevisExpire,
    /// Le demandeur a répondu à un devis (FR-017).
    DevisRepondu,
}

impl GenreEvenement {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::StatutMission => "MISSION_STATUS",
            Self::DevisEmis => "QUOTE_SENT",
            Self::DevisExpire => "QUOTE_EXPIRED",
            Self::DevisRepondu => "QUOTE_ANSWERED",
        }
    }
}

impl fmt::Display for GenreEvenement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Ce qui voyage sur le canal, et jusqu'au navigateur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvenementMission {
    pub mission_id: Uuid,
    pub genre: GenreEvenement,
    /// Statut de la Mission quand le genre est `MISSION_STATUS`.
    ///
    /// Un mot du vocabulaire de FR-018, jamais une phrase : l'affichage est au
    /// client, qui connaît la langue de son utilisateur.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub statut: Option<String>,
    pub survenu_le: DateTime<Utc>,
}

impl EvenementMission {
    pub fn statut(mission_id: Uuid, statut: &str, survenu_le: DateTime<Utc>) -> Self {
        Self {
            mission_id,
            genre: GenreEvenement::StatutMission,
            statut: Some(statut.to_string()),
            survenu_le,
        }
    }

    pub fn devis_emis(mission_id: Uuid, survenu_le: DateTime<Utc>) -> Self {
        Self {
            mission_id,
            genre: GenreEvenement::DevisEmis,
            statut: None,
            survenu_le,
        }
    }

    /// Le demandeur a accepté ou refusé.
    ///
    /// Le statut voyage — `ACCEPTED` ou `REFUSED` — parce que c'est justement ce
    /// que le prestataire attend, et parce qu'il ne dit rien de plus que ce que
    /// la relecture lui montrera. Le motif du refus, lui, reste derrière la
    /// session : il n'a pas à s'afficher sur un écran verrouillé.
    pub fn devis_repondu(mission_id: Uuid, statut: &str, survenu_le: DateTime<Utc>) -> Self {
        Self {
            mission_id,
            genre: GenreEvenement::DevisRepondu,
            statut: Some(statut.to_string()),
            survenu_le,
        }
    }

    pub fn devis_expire(mission_id: Uuid, survenu_le: DateTime<Utc>) -> Self {
        Self {
            mission_id,
            genre: GenreEvenement::DevisExpire,
            statut: None,
            survenu_le,
        }
    }

    /// Charge JSON du `NOTIFY`.
    ///
    /// `NOTIFY` plafonne à huit kilo-octets ; celle-ci en fait moins de deux
    /// cents, et c'est une raison de plus de n'y mettre que des identifiants.
    pub fn en_json(&self) -> String {
        // Une structure de trois champs scalaires : la sérialisation ne peut
        // pas échouer, et un `expect` ici dirait la vérité plutôt que de
        // propager une erreur qui n'arrivera jamais.
        serde_json::to_string(self).expect("événement sérialisable")
    }

    pub fn depuis_json(charge: &str) -> Option<Self> {
        serde_json::from_str(charge).ok()
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
    fn happy_un_evenement_de_statut_fait_l_aller_retour() {
        let id = Uuid::new_v4();
        let origine = EvenementMission::statut(id, "ON_SITE", instant());
        let relu = EvenementMission::depuis_json(&origine.en_json()).expect("relecture");
        assert_eq!(relu, origine);
        assert_eq!(relu.statut.as_deref(), Some("ON_SITE"));
    }

    #[test]
    fn happy_un_evenement_de_devis_n_a_pas_de_statut() {
        let evenement = EvenementMission::devis_emis(Uuid::new_v4(), instant());
        assert_eq!(evenement.statut, None);
        // Le champ absent, et non `null` : le client n'a pas à distinguer deux
        // façons de dire la même chose.
        assert!(!evenement.en_json().contains("statut"));
    }

    #[test]
    fn negative_une_charge_illisible_ne_fait_pas_tomber_l_ecoute() {
        // Un `NOTIFY` peut venir d'ailleurs : un `psql` ouvert, un autre
        // service, une version plus récente du format. L'écoute doit
        // l'ignorer, pas s'arrêter.
        for charge in ["", "{}", "pas du json", r#"{"mission_id":"pas-un-uuid"}"#] {
            assert!(EvenementMission::depuis_json(charge).is_none(), "{charge}");
        }
    }

    #[test]
    fn edge_la_charge_reste_tres_en_deca_du_plafond_de_notify() {
        // Huit kilo-octets est la limite de PostgreSQL. Ce test tombera le jour
        // où quelqu'un voudra faire voyager une description dans l'événement,
        // et c'est le moment où il faut se demander pourquoi.
        let json =
            EvenementMission::statut(Uuid::new_v4(), "PROVIDER_EN_ROUTE", instant()).en_json();
        assert!(json.len() < 200, "charge de {} octets : {json}", json.len());
    }

    #[test]
    fn security_l_evenement_ne_peut_porter_ni_adresse_ni_montant() {
        // La structure n'a pas de champ pour cela : c'est le type qui porte la
        // garantie, comme `VuePrestataire` sans position. Ce test fixe la
        // conséquence pour qu'un ajout de champ le casse.
        let json = EvenementMission::devis_emis(Uuid::new_v4(), instant()).en_json();
        for interdit in ["latitude", "longitude", "montant", "description", "email"] {
            assert!(!json.contains(interdit), "{interdit} n'a rien à faire ici");
        }
    }

    #[test]
    fn security_le_vocabulaire_des_genres_est_stable() {
        // Ces codes sortent du service et se retrouvent dans du code client :
        // les renommer coûte plus que la cohérence gagnée.
        assert_eq!(GenreEvenement::StatutMission.as_str(), "MISSION_STATUS");
        assert_eq!(GenreEvenement::DevisEmis.as_str(), "QUOTE_SENT");
        assert_eq!(GenreEvenement::DevisExpire.as_str(), "QUOTE_EXPIRED");
        assert_eq!(GenreEvenement::DevisRepondu.as_str(), "QUOTE_ANSWERED");
    }
}
