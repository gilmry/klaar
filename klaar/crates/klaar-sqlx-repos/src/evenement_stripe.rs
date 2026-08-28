//! Journal des webhooks Stripe (Story 5.5, FR-028), en PostgreSQL.

use chrono::{DateTime, Utc};
use sqlx::Row;

use klaar_application::ports::erreurs::RepositoryError;
use klaar_application::ports::evenement_stripe_repository::{
    Consignation, EvenementStripeRepository,
};
use klaar_stripe_adapter::{Evenement, Suite};

use crate::erreur;
use crate::pool::PoolPg;

pub struct PgEvenementStripeRepository {
    pool: PoolPg,
}

impl PgEvenementStripeRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }
}

fn suite_en_texte(suite: Suite) -> &'static str {
    match suite {
        Suite::Appliquer => "APPLIED",
        Suite::Depasse => "SUPERSEDED",
        // Un doublon n'atteint pas l'écriture : l'insertion échoue avant.
        Suite::DejaTraite | Suite::Ignore => "IGNORED",
    }
}

impl EvenementStripeRepository for PgEvenementStripeRepository {
    async fn dernier_applique(
        &self,
        objet_id: &str,
    ) -> Result<Option<DateTime<Utc>>, RepositoryError> {
        let ligne = sqlx::query(
            "SELECT cree_le FROM evenement_stripe
             WHERE objet_id = $1 AND applique
             ORDER BY cree_le DESC LIMIT 1",
        )
        .bind(objet_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(ligne.map(|l| l.get("cree_le")))
    }

    async fn consigner(
        &self,
        evenement: &Evenement,
        suite: Suite,
        recu_le: DateTime<Utc>,
    ) -> Result<Consignation, RepositoryError> {
        // **`ON CONFLICT DO NOTHING` est l'idempotence.** Deux réceptions
        // simultanées du même événement : une seule écrit, l'autre obtient
        // `DejaVu` et n'applique rien. Lire d'abord pour décider ensuite
        // laisserait les deux passer, et la capture serait prélevée deux fois.
        let ecrit = sqlx::query(
            "INSERT INTO evenement_stripe
                 (id, type_, objet_id, cree_le, recu_le, applique, suite)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (id) DO NOTHING
             RETURNING id",
        )
        .bind(&evenement.id)
        .bind(evenement.type_.as_str())
        .bind(&evenement.objet_id)
        .bind(evenement.cree_le)
        .bind(recu_le)
        .bind(suite == Suite::Appliquer)
        .bind(suite_en_texte(suite))
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;

        Ok(if ecrit.is_some() {
            Consignation::Neuf
        } else {
            Consignation::DejaVu
        })
    }
}
