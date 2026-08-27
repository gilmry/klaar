//! Adapters de persistance PostgreSQL (ADR-002 : `sqlx`, SQL pur, pas d'ORM).

mod journal_audit;
mod pool;
mod push_subscription;
mod utilisateur;

pub use journal_audit::PgJournalAudit;
pub use pool::{creer_pool, PoolPg};
pub use push_subscription::PgPushSubscriptionRepository;
pub use utilisateur::PgUtilisateurRepository;

use klaar_application::ports::erreurs::RepositoryError;

/// Traduit une erreur `sqlx` en erreur de port.
///
/// La distinction n'est pas cosmétique : une violation de contrainte est une
/// donnée refusée, qu'il ne sert à rien de réessayer, là où une indisponibilité
/// est transitoire. Les confondre fait réessayer en boucle ce qui ne passera
/// jamais, et abandonner ce qui serait passé à la seconde tentative.
pub(crate) fn erreur(e: sqlx::Error) -> RepositoryError {
    match &e {
        sqlx::Error::Database(db) if db.is_unique_violation() || db.is_foreign_key_violation() => {
            RepositoryError::Contrainte(db.message().to_string())
        }
        sqlx::Error::Database(db) if db.is_check_violation() => {
            RepositoryError::Contrainte(db.message().to_string())
        }
        _ => RepositoryError::Indisponible(e.to_string()),
    }
}
