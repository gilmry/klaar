//! Trace de matching PostgreSQL (Story 3.2, FR-012, AI Act art. 12).

use klaar_application::ports::erreurs::RepositoryError;
use klaar_application::ports::trace_repository::{LigneTrace, TraceRepository};

use crate::erreur;
use crate::pool::PoolPg;

pub struct PgTraceRepository {
    pool: PoolPg,
}

impl PgTraceRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }
}

impl TraceRepository for PgTraceRepository {
    async fn consigner(&self, lignes: &[LigneTrace]) -> Result<(), RepositoryError> {
        if lignes.is_empty() {
            return Ok(());
        }
        // Toutes ou aucune : une trace partielle est pire qu'absente, elle
        // laisse croire que les candidats manquants n'ont jamais été examinés.
        let mut tx = self.pool.begin().await.map_err(erreur)?;

        for ligne in lignes {
            let ventilation = serde_json::to_value(ligne.score).map_err(|e| {
                RepositoryError::Contrainte(format!("ventilation non sérialisable : {e}"))
            })?;

            // `ON CONFLICT DO NOTHING` sur le couple Demande/prestataire : un
            // second tour de matching sur la même Demande ne doit pas écraser
            // la trace du premier. Ce qui a été décidé l'a été à un instant
            // donné, et le réécrire effacerait ce qu'on cherche justement à
            // pouvoir expliquer.
            sqlx::query(
                "INSERT INTO trace_matching
                     (demande_id, provider_id, score, distance_metres, ventilation,
                      retenu, motif_ecart, tracee_le)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT (demande_id, provider_id) DO NOTHING",
            )
            .bind(ligne.demande_id)
            .bind(ligne.provider_id)
            .bind(ligne.score.total)
            .bind(ligne.distance_metres)
            .bind(ventilation)
            .bind(ligne.retenu)
            .bind(ligne.motif_ecart.map(|m| m.as_str()))
            .bind(ligne.tracee_le)
            .execute(&mut *tx)
            .await
            .map_err(erreur)?;
        }

        tx.commit().await.map_err(erreur)?;
        Ok(())
    }
}
