//! Adapters de persistance PostgreSQL (ADR-002 : `sqlx`, SQL pur, pas d'ORM).

mod pool;
mod push_subscription;

pub use pool::{creer_pool, PoolPg};
pub use push_subscription::PgPushSubscriptionRepository;
