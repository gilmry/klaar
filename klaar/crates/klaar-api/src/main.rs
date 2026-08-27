//! Serveur HTTP Klaar.

use std::sync::Arc;

use actix_web::{web, App, HttpServer};
use actix_web_prom::PrometheusMetricsBuilder;
use tracing_actix_web::TracingLogger;
use tracing_subscriber::{fmt, fmt::format::FmtSpan, prelude::*, EnvFilter};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use klaar_api::jwt::JwtHs256;
use klaar_api::limitation::LimiteurMemoire;
use klaar_api::telemetry::SpanSansDonneesPersonnelles;
use klaar_api::{configurer, ApiDoc, EtatApplication};
use klaar_application::ports::horloge::HorlogeSysteme;
use klaar_email_adapter::CourrielJournalise;
use klaar_identity::ParametresArgon2;
use klaar_push_adapter::{ClesVapid, WebPushSender};
use klaar_sqlx_repos::{
    creer_pool, PgJournalAudit, PgPushSubscriptionRepository, PgSessionRepository,
    PgUtilisateurRepository,
};

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

    // Le secret de signature n'est pas optionnel, contrairement aux clés VAPID :
    // sans lui, personne ne peut se connecter. Refuser de démarrer vaut mieux
    // qu'en générer un à la volée, qui invaliderait toutes les sessions à
    // chaque redémarrage sans que personne ne comprenne pourquoi.
    let jetons = match std::env::var("KLAAR_JWT_SECRET") {
        Ok(secret) => match JwtHs256::new(secret.as_bytes()) {
            Ok(emetteur) => Arc::new(emetteur),
            Err(e) => {
                eprintln!("KLAAR_JWT_SECRET invalide : {e}");
                eprintln!("en générer un : openssl rand -base64 48");
                std::process::exit(1);
            }
        },
        Err(_) => {
            eprintln!("KLAAR_JWT_SECRET absente : klaar-api ne peut pas signer de session.");
            eprintln!("en générer un : openssl rand -base64 48");
            std::process::exit(1);
        }
    };

    // Vrai par défaut : un cookie de session sans `Secure` voyage en clair. Le
    // désactiver n'a de sens qu'en développement local sur HTTP, où le
    // navigateur refuserait sinon le cookie sans rien signaler.
    let cookie_securise = std::env::var("KLAAR_COOKIE_SECURE").as_deref() != Ok("0");
    if !cookie_securise {
        tracing::warn!(
            "KLAAR_COOKIE_SECURE=0 : le cookie de rafraîchissement part sans l'attribut \
             Secure. Développement local uniquement."
        );
    }

    // Un seul limiteur pour tout le processus. Le construire dans la fabrique
    // de `App` en donnerait un par fil d'exécution, et la limite annoncée
    // serait silencieusement multipliée par le nombre de coeurs.
    let limiteur = Arc::new(LimiteurMemoire::new());
    let courriel = Arc::new(CourrielJournalise::depuis_environnement());
    let derriere_proxy = std::env::var("KLAAR_DERRIERE_PROXY").as_deref() == Ok("1");
    if !derriere_proxy {
        tracing::info!(
            "KLAAR_DERRIERE_PROXY absente : X-Forwarded-For ignoré, la limitation \
             de débit compte par adresse de connexion directe"
        );
    }

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
            utilisateurs: Arc::new(PgUtilisateurRepository::new(pool.clone())),
            journal: Arc::new(PgJournalAudit::new(pool.clone())),
            sessions: Arc::new(PgSessionRepository::new(pool.clone())),
            jetons: jetons.clone(),
            courriel: courriel.clone(),
            horloge: Arc::new(HorlogeSysteme),
            limiteur: limiteur.clone(),
            argon2: ParametresArgon2::production(),
            derriere_proxy,
            cookie_securise,
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
