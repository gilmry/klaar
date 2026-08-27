//! Port d'émission du jeton d'accès (FR-004).
//!
//! Le format — JWT, algorithme, en-têtes — est un détail de transport et
//! n'apparaît donc pas ici. Le cas d'usage demande « un jeton pour ce compte,
//! valable jusque-là » ; ce qui le signe est remplaçable sans le toucher.

use chrono::{DateTime, Utc};
use std::fmt;
use uuid::Uuid;

/// Durée de vie du jeton d'accès, fixée par FR-004.
///
/// Courte parce qu'un jeton d'accès n'est pas révocable : le seul moyen d'en
/// limiter la portée après coup est qu'il expire. C'est le refresh, lui
/// révocable, qui porte la durée longue.
pub const VALIDITE_ACCES_SECONDES: i64 = 3600;

/// Ce que le jeton affirme. Volontairement minimal : tout champ ajouté ici est
/// un champ que le porteur peut lire, et qu'un service tiers pourrait croire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimsAcces {
    pub utilisateur_id: Uuid,
    pub emis_le: DateTime<Utc>,
    pub expire_le: DateTime<Utc>,
}

#[derive(Debug)]
pub struct ErreurJeton(pub String);

impl fmt::Display for ErreurJeton {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "jeton d'accès non émis : {}", self.0)
    }
}

impl std::error::Error for ErreurJeton {}

pub trait EmetteurJetonAcces: Send + Sync {
    fn emettre(&self, claims: &ClaimsAcces) -> Result<String, ErreurJeton>;

    /// Vérifie un jeton reçu et rend ce qu'il affirme.
    ///
    /// Une signature invalide et un jeton périmé sont tous deux des refus : les
    /// distinguer dans la réponse n'aide que l'attaquant, qui apprendrait que
    /// sa forgerie est bien formée.
    fn verifier(&self, jeton: &str) -> Result<ClaimsAcces, ErreurJeton>;
}
