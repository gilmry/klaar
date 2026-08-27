//! Dépôt PostgreSQL des sessions de rafraîchissement (Story 1.3, FR-004).

use chrono::{DateTime, Utc};
use uuid::Uuid;

use klaar_application::ports::erreurs::RepositoryError;
use klaar_application::ports::session_repository::{SessionAConserver, SessionRepository};

use crate::erreur;
use crate::pool::PoolPg;

pub struct PgSessionRepository {
    pool: PoolPg,
}

impl PgSessionRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }
}

impl SessionRepository for PgSessionRepository {
    async fn ouvrir(&self, session: &SessionAConserver) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO session_refresh (empreinte, utilisateur_id, famille_id, expire_le)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(session.empreinte.as_str())
        .bind(session.utilisateur_id)
        .bind(session.famille_id)
        .bind(session.expire_le)
        .execute(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(())
    }

    async fn revoquer_famille(
        &self,
        famille_id: Uuid,
        maintenant: DateTime<Utc>,
    ) -> Result<u64, RepositoryError> {
        // `revoque_le IS NULL` : une révocation ne réécrit pas la date d'une
        // révocation antérieure. Sinon, couper deux fois une famille effacerait
        // l'instant où le vol a réellement été détecté.
        let resultat = sqlx::query(
            "UPDATE session_refresh SET revoque_le = $1
             WHERE famille_id = $2 AND revoque_le IS NULL",
        )
        .bind(maintenant)
        .bind(famille_id)
        .execute(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(resultat.rows_affected())
    }
}
