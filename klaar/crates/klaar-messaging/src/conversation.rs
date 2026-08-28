//! Conversation entre le demandeur et le prestataire (FR-030, Story 6.1).
//!
//! **Il n'y a pas d'agrégat « Conversation ».** Une Mission en tient lieu : elle
//! désigne exactement deux personnes, elle a une date de naissance et une date
//! de fin, et c'est d'elle que dépendent l'ouverture et la fermeture des
//! échanges. Ajouter une entité qui n'aurait qu'un identifiant de Mission et
//! rien d'autre aurait été un détour.
//!
//! **La conversation se ferme, et de deux façons.** Sept jours après la fin de
//! l'intervention, et au centième message. La première borne évite qu'un fil
//! serve de messagerie gratuite six mois plus tard ; la seconde est une limite
//! assumée du périmètre, pas une règle métier — une vraie messagerie
//! demanderait de la pagination, de la recherche et une purge, et rien de tout
//! cela n'est ici.

use chrono::{DateTime, Duration, Utc};
use std::fmt;
use uuid::Uuid;

/// Longueur d'un message (FR-030 `@negative`).
pub const MESSAGE_MAX_CARACTERES: usize = 4_000;

/// Messages par conversation avant passage en lecture seule (FR-030 `@edge`).
pub const MESSAGES_MAX: i64 = 100;

/// Délai après lequel une conversation se ferme, en jours (FR-030 `@negative`).
pub const CONVERSATION_FERMEE_JOURS: i64 = 7;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageError {
    Vide,
    TropLong,
    /// L'intervention est close depuis plus de sept jours (FR-030 `@negative`).
    ConversationFermee,
    /// Cent messages atteints (FR-030 `@edge`).
    ConversationPleine,
    /// Le message contient des coordonnées (FR-032).
    CoordonneesInterdites(crate::Coordonnee),
}

impl MessageError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Vide => "MESSAGE_EMPTY",
            Self::TropLong => "MESSAGE_TOO_LONG",
            Self::ConversationFermee => "CONVERSATION_CLOSED",
            Self::ConversationPleine => "CONVERSATION_FULL",
            Self::CoordonneesInterdites(_) => "CONTACT_INFO_FORBIDDEN",
        }
    }
}

impl fmt::Display for MessageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vide => write!(f, "message vide"),
            Self::TropLong => write!(f, "message au-delà de {MESSAGE_MAX_CARACTERES} caractères"),
            Self::ConversationFermee => write!(
                f,
                "la conversation se ferme {CONVERSATION_FERMEE_JOURS} jours après l'intervention"
            ),
            Self::ConversationPleine => {
                write!(f, "conversation limitée à {MESSAGES_MAX} messages")
            }
            Self::CoordonneesInterdites(quoi) => write!(
                f,
                "les coordonnées ({quoi}) ne s'échangent pas ici : l'intervention et le \
                 recours passent par le service"
            ),
        }
    }
}

impl std::error::Error for MessageError {}

/// Un message, tel qu'il sera consigné.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub id: Uuid,
    pub mission_id: Uuid,
    /// Le compte qui écrit. Déduit du jeton, jamais reçu.
    pub auteur_id: Uuid,
    pub corps: String,
    pub envoye_le: DateTime<Utc>,
}

impl Message {
    /// Écrit un message, ou dit pourquoi il est refusé.
    ///
    /// `close_depuis` est l'instant où l'intervention s'est terminée, quand
    /// elle l'est. Tant qu'elle est en cours, la conversation reste ouverte
    /// quelle que soit son ancienneté : une intervention qui traîne est
    /// justement le moment où l'on a le plus besoin de se parler.
    pub fn ecrire(
        mission_id: Uuid,
        auteur_id: Uuid,
        corps: &str,
        deja_ecrits: i64,
        close_depuis: Option<DateTime<Utc>>,
        maintenant: DateTime<Utc>,
    ) -> Result<Self, MessageError> {
        // Les refus de forme d'abord : inutile d'analyser le contenu d'un
        // message qui ne sera pas envoyé de toute façon.
        let corps = corps.trim();
        if corps.is_empty() {
            return Err(MessageError::Vide);
        }
        if corps.chars().count() > MESSAGE_MAX_CARACTERES {
            return Err(MessageError::TropLong);
        }

        if let Some(fin) = close_depuis {
            if maintenant >= fin + Duration::days(CONVERSATION_FERMEE_JOURS) {
                return Err(MessageError::ConversationFermee);
            }
        }
        if deja_ecrits >= MESSAGES_MAX {
            return Err(MessageError::ConversationPleine);
        }

        // L'anti-contournement en dernier : c'est le seul refus qui porte sur
        // ce que la personne a voulu dire, et le lui reprocher avant de savoir
        // si son message avait une chance d'arriver serait mal élevé.
        if let Some(quoi) = crate::detecter(corps) {
            return Err(MessageError::CoordonneesInterdites(quoi));
        }

        Ok(Self {
            id: Uuid::new_v4(),
            mission_id,
            auteur_id,
            corps: corps.to_string(),
            envoye_le: maintenant,
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

    fn ecrire(corps: &str) -> Result<Message, MessageError> {
        Message::ecrire(Uuid::new_v4(), Uuid::new_v4(), corps, 0, None, t0())
    }

    // === @happy ===

    #[test]
    fn happy_un_message_ordinaire_passe() {
        let m = ecrire("Bonjour, où êtes-vous ?").unwrap();
        assert_eq!(m.corps, "Bonjour, où êtes-vous ?");
        assert_eq!(m.envoye_le, t0());
    }

    #[test]
    fn happy_une_conversation_reste_ouverte_pendant_l_intervention() {
        // Une intervention qui traîne est justement le moment où l'on a le plus
        // besoin de se parler.
        let tres_tard = t0() + Duration::days(30);
        assert!(Message::ecrire(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "toujours là ?",
            0,
            None,
            tres_tard
        )
        .is_ok());
    }

    // === @negative ===

    #[test]
    fn negative_un_message_trop_long_est_refuse() {
        // FR-030 `@negative` : 422 au-delà de quatre mille caractères.
        let refus = ecrire(&"x".repeat(MESSAGE_MAX_CARACTERES + 1));
        assert_eq!(refus, Err(MessageError::TropLong));
    }

    #[test]
    fn negative_un_message_vide_est_refuse() {
        assert_eq!(ecrire(""), Err(MessageError::Vide));
        assert_eq!(ecrire("   \n  "), Err(MessageError::Vide));
    }

    #[test]
    fn negative_une_conversation_close_depuis_sept_jours_refuse() {
        let refus = Message::ecrire(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "encore une question",
            0,
            Some(t0()),
            t0() + Duration::days(CONVERSATION_FERMEE_JOURS),
        );
        assert_eq!(refus, Err(MessageError::ConversationFermee));
    }

    // === @edge ===

    #[test]
    fn edge_le_septieme_jour_passe_encore() {
        let juste_avant = t0() + Duration::days(CONVERSATION_FERMEE_JOURS) - Duration::seconds(1);
        assert!(Message::ecrire(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "merci pour tout",
            0,
            Some(t0()),
            juste_avant
        )
        .is_ok());
    }

    #[test]
    fn edge_le_centieme_message_ferme_la_conversation() {
        // FR-030 `@edge` : limite assumée du périmètre.
        let quatre_vingt_dix_neuf = Message::ecrire(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "encore un",
            MESSAGES_MAX - 1,
            None,
            t0(),
        );
        assert!(quatre_vingt_dix_neuf.is_ok());

        let centieme = Message::ecrire(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "et un de trop",
            MESSAGES_MAX,
            None,
            t0(),
        );
        assert_eq!(centieme, Err(MessageError::ConversationPleine));
    }

    #[test]
    fn edge_le_message_est_debarrasse_de_ses_blancs() {
        let m = ecrire("  bonjour  ").unwrap();
        assert_eq!(m.corps, "bonjour");
    }

    #[test]
    fn edge_la_longueur_se_compte_en_caracteres_et_non_en_octets() {
        // Quatre mille emoji font seize mille octets : compter en octets
        // refuserait un message que l'utilisateur voit comme court.
        let m = ecrire(&"é".repeat(MESSAGE_MAX_CARACTERES)).unwrap();
        assert_eq!(m.corps.chars().count(), MESSAGE_MAX_CARACTERES);
    }

    // === @security ===

    #[test]
    fn security_un_message_avec_un_numero_est_refuse() {
        // FR-030 `@security`.
        let refus = ecrire("appelez-moi au 0470 12 34 56");
        assert_eq!(
            refus,
            Err(MessageError::CoordonneesInterdites(
                crate::Coordonnee::Telephone
            ))
        );
        assert_eq!(refus.unwrap_err().code(), "CONTACT_INFO_FORBIDDEN");
    }

    #[test]
    fn security_un_message_avec_une_adresse_est_refuse() {
        let refus = ecrire("écrivez à moi@exemple.eu");
        assert_eq!(
            refus,
            Err(MessageError::CoordonneesInterdites(
                crate::Coordonnee::Courriel
            ))
        );
    }

    #[test]
    fn security_les_refus_de_forme_priment_sur_l_anti_contournement() {
        // Reprocher à quelqu'un d'avoir voulu donner son numéro dans un message
        // qui n'aurait de toute façon pas été envoyé serait mal élevé, et
        // l'inciterait à réessayer plus court.
        let refus = ecrire(&format!(
            "0470123456 {}",
            "x".repeat(MESSAGE_MAX_CARACTERES)
        ));
        assert_eq!(refus, Err(MessageError::TropLong));
    }

    #[test]
    fn security_le_corps_est_conserve_tel_quel_sans_etre_interprete() {
        // Ni échappement, ni troncature, ni normalisation : c'est à l'affichage
        // de se protéger. Réécrire le message ici ferait mentir la conversation
        // sur ce qui a été dit.
        let hostile = "<script>alert(1)</script> & \"guillemets\"";
        assert_eq!(ecrire(hostile).unwrap().corps, hostile);
    }
}
