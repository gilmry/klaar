//! Exécute les effacements arrivés à échéance (Story 1.9, FR-005).
//!
//! Un binaire à lancer périodiquement plutôt qu'une tâche de fond dans
//! `klaar-api` : une tâche de fond s'exécute autant de fois qu'il y a
//! d'exemplaires du serveur, et se tait quand le serveur redémarre au mauvais
//! moment. Un binaire séparé se planifie, se relance à la main, et son sort se
//! lit dans les journaux de l'ordonnanceur.
//!
//! Idempotent : un compte déjà effacé n'est plus retrouvé par la requête
//! d'échéance, et un second passage ne fait rien.

use std::process::ExitCode;
use std::sync::Arc;

use klaar_application::ports::horloge::HorlogeSysteme;
use klaar_application::usecases::effacer::executer_les_echus;
use klaar_sqlx_repos::{creer_pool, PgJournalAudit, PgUtilisateurRepository};

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt().json().init();

    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("DATABASE_URL requise");
            return ExitCode::FAILURE;
        }
    };

    let pool = match creer_pool(&database_url).await {
        Ok(pool) => pool,
        Err(e) => {
            eprintln!("connexion PostgreSQL impossible : {e}");
            return ExitCode::FAILURE;
        }
    };

    let depot = Arc::new(PgUtilisateurRepository::new(pool.clone()));
    let journal = PgJournalAudit::new(pool);

    match executer_les_echus(depot.as_ref(), &journal, &HorlogeSysteme).await {
        Ok(0) => {
            tracing::info!("aucun effacement à échéance");
            ExitCode::SUCCESS
        }
        Ok(n) => {
            // Le nombre, jamais les identifiants : ce journal n'a pas à dire
            // qui a exercé son droit à l'effacement.
            tracing::info!(comptes = n, "effacements exécutés");
            ExitCode::SUCCESS
        }
        Err(e) => {
            tracing::error!(erreur = %e, "effacements interrompus");
            ExitCode::FAILURE
        }
    }
}
