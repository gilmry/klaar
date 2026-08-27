//! Port de notification push (Story 0.12, ADR-010).
//!
//! Le port ignore délibérément qu'il s'agit de Web Push : il parle
//! d'abonnements et de messages. C'est ce qui permettrait de changer de
//! protocole sans toucher aux use cases — et c'est aussi ce qui a rendu la
//! bascule d'ADR-007 (APNs/FCM directs) vers Web Push indolore de ce côté.

use std::fmt;

/// Abonnement d'un appareil, tel que le navigateur le produit
/// (`PushSubscription.toJSON()` côté client).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushSubscription {
    /// URL du service de push du navigateur. Elle identifie l'appareil : c'est
    /// une donnée à caractère personnel, à traiter comme telle.
    pub endpoint: String,
    /// Clé publique P-256 de l'agent utilisateur, forme non compressée,
    /// base64url sans remplissage (`keys.p256dh`).
    pub p256dh: String,
    /// Secret d'authentification de 16 octets, base64url sans remplissage
    /// (`keys.auth`).
    pub auth: String,
}

/// Contenu affiché par le service worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushMessage {
    pub titre: String,
    pub corps: String,
    /// Chemin ouvert au clic sur la notification.
    pub url: String,
    /// Regroupe les notifications qui se remplacent l'une l'autre : deux
    /// messages de même `tag` n'en affichent qu'un. Évite d'empiler dix
    /// alertes pour une même Mission.
    pub tag: Option<String>,
}

#[derive(Debug)]
pub enum PushError {
    /// Le service de push a répondu 404 ou 410 : l'abonnement n'existe plus.
    /// L'appelant **doit** le supprimer de sa base, sans quoi il réessaiera
    /// indéfiniment et gardera une donnée personnelle devenue inutile.
    AbonnementExpire,
    /// Abonnement mal formé (clé de mauvaise longueur, base64 invalide).
    AbonnementInvalide(String),
    /// Échec de chiffrement ou de signature.
    Cryptographie(String),
    /// Le service de push a répondu autre chose qu'un succès.
    ServiceDePush { status: u16, corps: String },
    /// La requête n'a pas abouti.
    Transport(String),
}

impl fmt::Display for PushError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AbonnementExpire => write!(f, "abonnement push expiré"),
            Self::AbonnementInvalide(d) => write!(f, "abonnement push invalide : {d}"),
            Self::Cryptographie(d) => write!(f, "échec cryptographique : {d}"),
            Self::ServiceDePush { status, corps } => {
                write!(f, "service de push : {status} {corps}")
            }
            Self::Transport(d) => write!(f, "transport : {d}"),
        }
    }
}

impl std::error::Error for PushError {}

/// Envoie une notification à un abonnement.
///
/// Volontairement non `async` dans sa définition : le trait reste utilisable
/// par un adaptateur synchrone de test. L'adaptateur réel expose une variante
/// asynchrone.
pub trait PushNotifier {
    fn envoyer(
        &self,
        abonnement: &PushSubscription,
        message: &PushMessage,
    ) -> Result<(), PushError>;
}
