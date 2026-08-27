//! Dépôt PostgreSQL des abonnements Web Push (Story 0.12).

use sqlx::Row;
use uuid::Uuid;

use klaar_application::ports::push::PushSubscription;
use klaar_application::ports::push_repository::{
    AbonnementEnregistre, PushSubscriptionRepository, RepositoryError,
};

use crate::erreur;
use crate::pool::PoolPg;

pub struct PgPushSubscriptionRepository {
    pool: PoolPg,
}

impl PgPushSubscriptionRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }
}

fn depuis_ligne(ligne: &sqlx::postgres::PgRow) -> AbonnementEnregistre {
    AbonnementEnregistre {
        id: ligne.get("id"),
        abonnement: PushSubscription {
            endpoint: ligne.get("endpoint"),
            p256dh: ligne.get("p256dh"),
            auth: ligne.get("auth"),
        },
        sujet_id: ligne.get("sujet_id"),
    }
}

impl PushSubscriptionRepository for PgPushSubscriptionRepository {
    async fn enregistrer(
        &self,
        abonnement: &PushSubscription,
        sujet_id: Option<Uuid>,
    ) -> Result<AbonnementEnregistre, RepositoryError> {
        // ON CONFLICT sur l'endpoint : un navigateur peut renouveler ses clés
        // en gardant la même URL. Insérer à nouveau créerait un doublon, donc
        // deux notifications pour un seul appareil.
        let ligne = sqlx::query(
            r#"
            INSERT INTO push_subscription (id, endpoint, p256dh, auth, sujet_id)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (endpoint) DO UPDATE
                SET p256dh = EXCLUDED.p256dh,
                    auth = EXCLUDED.auth,
                    sujet_id = COALESCE(EXCLUDED.sujet_id, push_subscription.sujet_id)
            RETURNING id, endpoint, p256dh, auth, sujet_id
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(&abonnement.endpoint)
        .bind(&abonnement.p256dh)
        .bind(&abonnement.auth)
        .bind(sujet_id)
        .fetch_one(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(depuis_ligne(&ligne))
    }

    async fn lister_par_sujet(
        &self,
        sujet_id: Uuid,
    ) -> Result<Vec<AbonnementEnregistre>, RepositoryError> {
        let lignes = sqlx::query(
            "SELECT id, endpoint, p256dh, auth, sujet_id
             FROM push_subscription WHERE sujet_id = $1 ORDER BY cree_le",
        )
        .bind(sujet_id)
        .fetch_all(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(lignes.iter().map(depuis_ligne).collect())
    }

    async fn supprimer_par_endpoint(&self, endpoint: &str) -> Result<bool, RepositoryError> {
        let resultat = sqlx::query("DELETE FROM push_subscription WHERE endpoint = $1")
            .bind(endpoint)
            .execute(&self.pool)
            .await
            .map_err(erreur)?;
        Ok(resultat.rows_affected() > 0)
    }

    async fn compter(&self) -> Result<i64, RepositoryError> {
        let ligne = sqlx::query("SELECT COUNT(*) AS n FROM push_subscription")
            .fetch_one(&self.pool)
            .await
            .map_err(erreur)?;
        Ok(ligne.get("n"))
    }
}
