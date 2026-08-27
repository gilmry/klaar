//! Journal d'audit PostgreSQL (Story 1.1, FR-001).

use klaar_application::ports::audit::{EntreeAudit, JournalAudit};
use klaar_application::ports::erreurs::RepositoryError;

use crate::erreur;
use crate::pool::PoolPg;

pub struct PgJournalAudit {
    pool: PoolPg,
}

impl PgJournalAudit {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }
}

impl JournalAudit for PgJournalAudit {
    async fn consigner(&self, entree: EntreeAudit) -> Result<(), RepositoryError> {
        sqlx::query("INSERT INTO journal_audit (code, sujet_id, horodatage) VALUES ($1, $2, $3)")
            .bind(entree.code.as_str())
            .bind(entree.sujet_id)
            .bind(entree.horodatage)
            .execute(&self.pool)
            .await
            .map_err(erreur)?;
        Ok(())
    }
}
