//! Dépôt PostgreSQL de la conversation (Story 6.1, FR-030, FR-032).

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use klaar_application::ports::erreurs::RepositoryError;
use klaar_application::ports::message_repository::{EtatConversation, MessageRepository};
use klaar_messaging::Message;

use crate::erreur;
use crate::pool::PoolPg;

pub struct PgMessageRepository {
    pool: PoolPg,
}

impl PgMessageRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }
}

fn depuis_ligne(ligne: &sqlx::postgres::PgRow) -> Message {
    Message {
        id: ligne.get("id"),
        mission_id: ligne.get("mission_id"),
        // Auteur effacé : le fil reste lisible, sans nom.
        auteur_id: ligne
            .get::<Option<Uuid>, _>("auteur_id")
            .unwrap_or(Uuid::nil()),
        corps: ligne.get("corps"),
        envoye_le: ligne.get("envoye_le"),
    }
}

impl MessageRepository for PgMessageRepository {
    async fn ecrire(&self, message: &Message) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO message (id, mission_id, auteur_id, corps, envoye_le)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(message.id)
        .bind(message.mission_id)
        .bind(message.auteur_id)
        .bind(&message.corps)
        .bind(message.envoye_le)
        .execute(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(())
    }

    async fn fil(&self, mission_id: Uuid) -> Result<Vec<Message>, RepositoryError> {
        let lignes = sqlx::query(
            "SELECT id, mission_id, auteur_id, corps, envoye_le
             FROM message WHERE mission_id = $1 ORDER BY envoye_le, id",
        )
        .bind(mission_id)
        .fetch_all(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(lignes.iter().map(depuis_ligne).collect())
    }

    async fn etat(&self, mission_id: Uuid) -> Result<EtatConversation, RepositoryError> {
        // La fin de l'intervention vient de l'historique et non du statut : une
        // Mission validée l'a d'abord été terminée, et c'est de la **fin des
        // travaux** que court le délai de sept jours, pas de la validation qui
        // peut arriver trois jours plus tard.
        let ligne = sqlx::query(
            "SELECT (SELECT count(*) FROM message WHERE mission_id = $1) AS deja_ecrits,
                    (SELECT min(horodate_le) FROM mission_transition
                     WHERE mission_id = $1 AND statut IN ('COMPLETED', 'CANCELLED')) AS close_depuis",
        )
        .bind(mission_id)
        .fetch_one(&self.pool)
        .await
        .map_err(erreur)?;

        Ok(EtatConversation {
            deja_ecrits: ligne.get("deja_ecrits"),
            close_depuis: ligne.get("close_depuis"),
        })
    }

    async fn consigner_tentative(
        &self,
        mission_id: Uuid,
        auteur_id: Uuid,
        genre: &str,
        tentee_le: DateTime<Utc>,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO tentative_contournement (mission_id, auteur_id, genre, tentee_le)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(mission_id)
        .bind(auteur_id)
        .bind(genre)
        .bind(tentee_le)
        .execute(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(())
    }

    async fn tentatives_depuis(
        &self,
        auteur_id: Uuid,
        depuis: DateTime<Utc>,
    ) -> Result<i64, RepositoryError> {
        let ligne = sqlx::query(
            "SELECT count(*) AS total FROM tentative_contournement
             WHERE auteur_id = $1 AND tentee_le >= $2",
        )
        .bind(auteur_id)
        .bind(depuis)
        .fetch_one(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(ligne.get("total"))
    }
}
