use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;

pub type PoolPg = PgPool;

/// Ouvre le pool de connexions.
///
/// `max_connections` est volontairement bas : un pool trop large ne rend pas
/// PostgreSQL plus rapide, il déplace la file d'attente du client vers le
/// serveur, où elle coûte un processus par connexion.
pub async fn creer_pool(url: &str) -> Result<PoolPg, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(url)
        .await
}
