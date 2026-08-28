//! Bounded context Messaging : conversation, pièces jointes, anti-contournement
//! (FR-030 à FR-032).
//!
//! Livré à ce jour : la détection de coordonnées (FR-032) et la conversation
//! (FR-030). Les pièces jointes (FR-031) attendent un stockage d'objets.

mod anti_contournement;
mod conversation;

pub use anti_contournement::{detecter, Coordonnee};
pub use conversation::{
    Message, MessageError, CONVERSATION_FERMEE_JOURS, MESSAGES_MAX, MESSAGE_MAX_CARACTERES,
};
