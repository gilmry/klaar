//! Adapters de persistance PostgreSQL (ADR-002 : `sqlx`, SQL pur, pas d'ORM).

pub mod annulation;
pub mod audit_trace;
mod catalogue;
mod catalogue_admin;
mod demande;
pub mod demonstration;
mod devis;
mod effacement;
mod evenement_stripe;
mod export;
mod journal_audit;
mod liberation;
mod litige;
mod message;
mod mission;
mod notation;
mod ops;
mod pool;
mod provider;
mod push_subscription;
mod reprogrammation;
mod revue_kyc;
mod session;
mod suivi;
mod tableau_bord;
mod trace;
mod utilisateur;

pub use annulation::PgAnnulationRepository;
pub use catalogue::PgCatalogueRepository;
pub use catalogue_admin::PgCatalogueAdminRepository;
pub use demande::{PgDemandeRepository, PgPaiementRepository};
pub use devis::PgDevisRepository;
pub use evenement_stripe::PgEvenementStripeRepository;
pub use export::PgExportRepository;
pub use journal_audit::PgJournalAudit;
pub use liberation::PgLiberationRepository;
pub use litige::PgLitigeRepository;
pub use message::PgMessageRepository;
pub use mission::PgMissionRepository;
pub use notation::PgNotationRepository;
pub use ops::PgOpsRepository;
pub use pool::{creer_pool, PoolPg};
pub use provider::PgProviderRepository;
pub use push_subscription::PgPushSubscriptionRepository;
pub use reprogrammation::PgReprogrammationRepository;
pub use revue_kyc::PgRevueKycRepository;
pub use session::PgSessionRepository;
pub use suivi::PgSuiviRepository;
pub use tableau_bord::PgTableauBordRepository;
pub use trace::PgTraceRepository;
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

/// Émet un événement de Mission sur le canal `LISTEN`/`NOTIFY` (Story 4.9).
///
/// **Toujours dans une transaction**, et c'est le cœur de la garantie :
/// PostgreSQL ne délivre un `NOTIFY` qu'au `COMMIT`. Une transaction abandonnée
/// n'annonce donc rien, et une transaction commise annonce toujours — les deux
/// fenêtres qu'un envoi « après l'écriture » laisserait ouvertes.
///
/// `pg_notify` plutôt que `NOTIFY` : la charge est un paramètre lié, là où
/// `NOTIFY canal, 'charge'` demanderait de la coller dans le texte SQL, ce qui
/// est le chemin habituel vers l'injection.
pub(crate) async fn notifier(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    evenement: &klaar_application::ports::evenements::EvenementMission,
) -> Result<(), RepositoryError> {
    sqlx::query("SELECT pg_notify($1, $2)")
        .bind(klaar_application::ports::evenements::CANAL)
        .bind(evenement.en_json())
        .execute(&mut **tx)
        .await
        .map_err(erreur)?;
    Ok(())
}
