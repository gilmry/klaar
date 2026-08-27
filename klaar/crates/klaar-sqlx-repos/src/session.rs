//! Dépôt PostgreSQL des sessions de rafraîchissement (Story 1.3, FR-004).

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use klaar_application::ports::erreurs::RepositoryError;
use klaar_application::ports::session_repository::{
    ResultatRotation, SessionAConserver, SessionRepository,
};
use klaar_identity::EmpreinteJeton;

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
            "INSERT INTO session_refresh
                 (empreinte, utilisateur_id, famille_id, expire_le, empreinte_contexte)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(session.empreinte.as_str())
        .bind(session.utilisateur_id)
        .bind(session.famille_id)
        .bind(session.expire_le)
        .bind(session.empreinte_contexte.as_ref().map(|c| c.as_str()))
        .execute(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(())
    }

    async fn rotationner(
        &self,
        presentee: &EmpreinteJeton,
        nouvelle: &EmpreinteJeton,
        contexte: Option<&EmpreinteJeton>,
        expire_le: DateTime<Utc>,
        maintenant: DateTime<Utc>,
    ) -> Result<ResultatRotation, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(erreur)?;

        // FOR UPDATE : deux onglets qui rafraîchissent en même temps
        // sérialisent ici. Sans le verrou, les deux liraient `consomme_le IS
        // NULL`, obtiendraient chacun un refresh neuf, et le second serait
        // ensuite pris pour un vol par le premier rejeu venu.
        let ligne = sqlx::query(
            "SELECT utilisateur_id, famille_id, expire_le, consomme_le, revoque_le,
                    empreinte_contexte
             FROM session_refresh WHERE empreinte = $1 FOR UPDATE",
        )
        .bind(presentee.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(erreur)?;

        let Some(ligne) = ligne else {
            tx.rollback().await.map_err(erreur)?;
            return Ok(ResultatRotation::Inconnu);
        };

        let utilisateur_id: Uuid = ligne.get("utilisateur_id");
        let famille_id: Uuid = ligne.get("famille_id");
        let consomme_le: Option<DateTime<Utc>> = ligne.get("consomme_le");
        let revoque_le: Option<DateTime<Utc>> = ligne.get("revoque_le");
        let expire_ligne: DateTime<Utc> = ligne.get("expire_le");
        let contexte_attendu: Option<String> = ligne.get("empreinte_contexte");

        // Le rejeu est testé en premier, avant même la révocation : après une
        // coupure de famille, tous les maillons sont révoqués *et* certains
        // consommés. Tester la révocation d'abord répondrait « révoqué » à un
        // rejeu, ce qui est vrai mais moins informatif — et masquerait la
        // seconde tentative d'un voleur dans le journal.
        if consomme_le.is_some() {
            tx.rollback().await.map_err(erreur)?;
            return Ok(ResultatRotation::Rejeu {
                famille_id,
                utilisateur_id,
            });
        }
        if revoque_le.is_some() {
            tx.rollback().await.map_err(erreur)?;
            return Ok(ResultatRotation::Revoque);
        }
        if expire_ligne <= maintenant {
            tx.rollback().await.map_err(erreur)?;
            return Ok(ResultatRotation::Expire);
        }

        sqlx::query("UPDATE session_refresh SET consomme_le = $1 WHERE empreinte = $2")
            .bind(maintenant)
            .bind(presentee.as_str())
            .execute(&mut *tx)
            .await
            .map_err(erreur)?;

        // Le nouveau maillon hérite du contexte d'origine, et non de celui
        // présenté : sinon un voleur ferait glisser l'empreinte attendue vers
        // la sienne à chaque rotation, et l'anomalie cesserait d'être signalée.
        sqlx::query(
            "INSERT INTO session_refresh
                 (empreinte, utilisateur_id, famille_id, expire_le, empreinte_contexte)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(nouvelle.as_str())
        .bind(utilisateur_id)
        .bind(famille_id)
        .bind(expire_le)
        .bind(contexte_attendu.as_deref())
        .execute(&mut *tx)
        .await
        .map_err(erreur)?;

        tx.commit().await.map_err(erreur)?;

        let contexte_change = match (contexte_attendu.as_deref(), contexte) {
            (Some(attendu), Some(recu)) => attendu != recu.as_str(),
            // Contexte inconnu d'un côté ou de l'autre : rien à comparer, donc
            // rien à signaler. Traiter l'absence comme une anomalie ferait
            // sonner l'alarme pour toutes les sessions ouvertes avant la
            // migration V5.
            _ => false,
        };

        Ok(ResultatRotation::Rotationne {
            utilisateur_id,
            famille_id,
            contexte_change,
        })
    }

    async fn famille_de(
        &self,
        empreinte: &EmpreinteJeton,
    ) -> Result<Option<Uuid>, RepositoryError> {
        sqlx::query_scalar("SELECT famille_id FROM session_refresh WHERE empreinte = $1")
            .bind(empreinte.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(erreur)
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
