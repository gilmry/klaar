//! Serveur HTTP Klaar (Story 0.5 — harnais contrat API, ADR-003 actix-web,
//! ADR-004 utoipa ; Story 0.8 — observabilité). Un seul endpoint métier pour
//! l'instant (health) : le contrat API sera étendu use case par use case,
//! chacun tracé sur un FR du PRD.

use actix_web::{get, App, HttpResponse, HttpServer};
use actix_web_prom::PrometheusMetricsBuilder;
use serde::Serialize;
use tracing_actix_web::TracingLogger;
use tracing_subscriber::{fmt, fmt::format::FmtSpan, prelude::*, EnvFilter};
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct HealthDto {
    status: String,
}

/// Vérifie que le service répond.
#[utoipa::path(
    get,
    path = "/api/v1/health",
    responses((status = 200, description = "Service opérationnel", body = HealthDto))
)]
#[get("/api/v1/health")]
async fn health() -> HttpResponse {
    HttpResponse::Ok().json(HealthDto {
        status: "ok".to_string(),
    })
}

#[derive(OpenApi)]
#[openapi(paths(health), components(schemas(HealthDto)))]
struct ApiDoc;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Logs JSON structurés. ATTENTION (foyer/skills/gates.md : "PII jamais
    // loggées") : le root span par défaut de tracing-actix-web loggue
    // http.client_ip et http.user_agent — l'IP est une donnée personnelle
    // au sens RGPD. Sans conséquence tant que /api/v1/health est le seul
    // endpoint, mais À CORRIGER (root span builder custom qui les omet, ou
    // pipeline de logs qui les tronque) avant tout endpoint FR réel
    // (Epic 1+, docs/runbook-incident.md).
    // with_span_events(CLOSE) : sans ça, tracing-actix-web crée un span par
    // requête mais rien ne l'imprime — fmt::layer() par défaut n'émet une
    // ligne qu'à un tracing::info!() explicite, pas à la fermeture d'un
    // span. Vérifié en local : sans cette ligne, aucune requête n'apparaît
    // dans les logs malgré TracingLogger::default() actif.
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(fmt::layer().json().with_span_events(FmtSpan::CLOSE))
        .init();

    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    tracing::info!(%bind_addr, "klaar-api démarre — /api/v1/docs Swagger UI, /metrics Prometheus");

    let metrics = PrometheusMetricsBuilder::new("klaar_api")
        .endpoint("/metrics")
        .build()
        .expect("configuration Prometheus invalide");

    HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .wrap(metrics.clone())
            .service(health)
            .service(
                SwaggerUi::new("/api/v1/docs/{_:.*}")
                    .url("/api/v1/openapi.json", ApiDoc::openapi()),
            )
    })
    .bind(bind_addr)?
    .run()
    .await
}
