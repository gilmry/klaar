//! Port de persistance des sessions de rafraîchissement (FR-004).

use chrono::{DateTime, Utc};
use klaar_identity::EmpreinteJeton;
use uuid::Uuid;

use super::erreurs::RepositoryError;

/// Durée de vie du refresh, fixée par FR-004.
pub const VALIDITE_REFRESH_JOURS: i64 = 30;

/// Session à écrire.
///
/// `famille_id` relie tous les refresh issus d'une même authentification. La
/// rotation en crée un nouveau à chaque usage ; c'est la famille qui permet, à
/// la détection d'un rejeu, de couper la chaîne entière plutôt que le seul
/// jeton rejoué — sans quoi le voleur garde le sien.
#[derive(Debug, Clone)]
pub struct SessionAConserver {
    pub empreinte: EmpreinteJeton,
    pub utilisateur_id: Uuid,
    pub famille_id: Uuid,
    pub expire_le: DateTime<Utc>,
}

#[allow(async_fn_in_trait)]
pub trait SessionRepository {
    async fn ouvrir(&self, session: &SessionAConserver) -> Result<(), RepositoryError>;

    /// Révoque toutes les sessions d'un compte. Rend le nombre de sessions
    /// effectivement coupées.
    async fn revoquer_famille(
        &self,
        famille_id: Uuid,
        maintenant: DateTime<Utc>,
    ) -> Result<u64, RepositoryError>;
}
