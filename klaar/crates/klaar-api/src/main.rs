//! Serveur HTTP Klaar.

use std::sync::Arc;

use actix_web::{web, App, HttpServer};
use actix_web_prom::PrometheusMetricsBuilder;
use tracing_actix_web::TracingLogger;
use tracing_subscriber::{fmt, fmt::format::FmtSpan, prelude::*, EnvFilter};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use klaar_api::telemetry::SpanSansDonneesPersonnelles;
use klaar_api::{configurer, ApiDoc, EtatApplication};
use klaar_push_adapter::{ClesVapid, WebPushSender};
use klaar_sqlx_repos::{creer_pool, PgPushSubscriptionRepository};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // with_span_events(CLOSE) : sans ça, tracing-actix-web crée un span par
    // requête mais rien ne l'imprime — fmt::layer() par défaut n'émet une
    // ligne qu'à un tracing::info!() explicite, pas à la fermeture d'un span.
    // Vérifié en local : sans cette ligne, aucune requête n'apparaît dans les
    // logs malgré TracingLogger::default() actif.
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(fmt::layer().json().with_span_events(FmtSpan::CLOSE))
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://klaar:klaar_dev_only@localhost:5433/klaar".to_string());
    let pool = creer_pool(&database_url).await.unwrap_or_else(|e| {
        // Échouer au démarrage plutôt qu'à la première requête : un service
        // qui démarre « vert » puis refuse tout est plus difficile à
        // diagnostiquer qu'un service qui refuse de démarrer.
        eprintln!("connexion PostgreSQL impossible ({database_url}) : {e}");
        std::process::exit(1);
    });

    // Le push est optionnel : sans clé configurée, le service tourne sans
    // notifications au lieu de refuser de démarrer.
    let push = match std::env::var("KLAAR_VAPID_PRIVATE_KEY") {
        Ok(cle) if !cle.is_empty() => {
            let sujet = std::env::var("KLAAR_VAPID_SUBJECT")
                .unwrap_or_else(|_| "mailto:ops@klaar.be".to_string());
            match ClesVapid::depuis_base64url(&cle, sujet) {
                Ok(cles) => Some(Arc::new(WebPushSender::new(cles))),
                Err(e) => {
                    eprintln!("clé VAPID invalide : {e}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            tracing::warn!(
                "KLAAR_VAPID_PRIVATE_KEY absente : les notifications push sont désactivées"
            );
            None
        }
    };

    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    tracing::info!(%bind_addr, "klaar-api démarre — /api/v1/docs Swagger UI, /metrics Prometheus");

    let metrics = PrometheusMetricsBuilder::new("klaar_api")
        .endpoint("/metrics")
        .build()
        .expect("configuration Prometheus invalide");

    HttpServer::new(move || {
        let etat = web::Data::new(EtatApplication {
            abonnements: Arc::new(PgPushSubscriptionRepository::new(pool.clone())),
            push: push.clone(),
        });
        App::new()
            // Span racine expurgé de l'IP et de l'agent utilisateur, cf.
            // klaar_api::telemetry.
            .wrap(TracingLogger::<SpanSansDonneesPersonnelles>::new())
            .wrap(metrics.clone())
            .app_data(etat)
            .configure(configurer)
            .service(
                SwaggerUi::new("/api/v1/docs/{_:.*}")
                    .url("/api/v1/openapi.json", ApiDoc::openapi()),
            )
    })
    .bind(bind_addr)?
    .run()
    .await
}
