//! Runner de migrations refinery (Story 0.3). Idempotent : ré-exécuter ne
//! ré-applique jamais une migration déjà passée. `klaar-api` embarquera ce
//! même mécanisme au démarrage une fois le binaire serveur câblé (Story 0.5).

mod embedded {
    refinery::embed_migrations!("../../migrations");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://klaar:klaar_dev_only@localhost:5432/klaar".to_string());

    let (mut client, connection) =
        tokio_postgres::connect(&database_url, tokio_postgres::NoTls).await?;
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("erreur de connexion postgres : {e}");
        }
    });

    let report = embedded::migrations::runner()
        .run_async(&mut client)
        .await?;
    if report.applied_migrations().is_empty() {
        println!("aucune migration à appliquer (déjà à jour)");
    }
    for migration in report.applied_migrations() {
        println!("migration appliquée : {migration}");
    }
    Ok(())
}
