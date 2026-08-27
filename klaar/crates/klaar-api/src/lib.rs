//! API HTTP de Klaar (actix-web + utoipa — ADR-003, ADR-004).
//!
//! L'application est construite ici, et non dans `main`, pour que les tests
//! puissent la monter en mémoire avec un état choisi. Un endpoint testé
//! seulement en lançant un vrai serveur finit par n'être testé que dans les
//! cas nominaux.

use std::sync::Arc;

use actix_web::{web, App};
use utoipa::OpenApi;

use klaar_push_adapter::WebPushSender;
use klaar_sqlx_repos::PgPushSubscriptionRepository;

pub mod routes;
pub mod telemetry;

/// Dépendances partagées par les handlers.
pub struct EtatApplication {
    pub abonnements: Arc<PgPushSubscriptionRepository>,
    /// `None` quand aucune clé VAPID n'est configurée : le déploiement tourne
    /// alors sans notifications, ce qui est un mode de fonctionnement légitime
    /// et non une panne.
    pub push: Option<Arc<WebPushSender>>,
}

#[derive(OpenApi)]
#[openapi(
    paths(
        routes::health::health,
        routes::push::cle_publique,
        routes::push::enregistrer_abonnement,
        routes::push::supprimer_abonnement,
    ),
    components(schemas(
        routes::health::HealthDto,
        routes::push::ClePubliqueDto,
        routes::push::AbonnementDto,
        routes::push::ClesAbonnementDto,
        routes::push::AbonnementEnregistreDto,
        routes::push::DesabonnementDto,
        routes::push::ErreurDto,
    )),
    tags(
        (name = "sonde", description = "Disponibilité du service"),
        (name = "push", description = "Abonnements Web Push (ADR-010)"),
    )
)]
pub struct ApiDoc;

/// Enregistre toutes les routes. Séparé de la construction de `App` pour être
/// réutilisable par `actix_web::test::init_service`.
pub fn configurer(cfg: &mut web::ServiceConfig) {
    cfg.service(routes::health::health)
        .service(routes::push::cle_publique)
        .service(routes::push::enregistrer_abonnement)
        .service(routes::push::supprimer_abonnement);
}

/// Type de retour de `App::new()` sans middleware, pour les tests.
pub fn app_de_test(
    etat: web::Data<EtatApplication>,
) -> App<
    impl actix_web::dev::ServiceFactory<
        actix_web::dev::ServiceRequest,
        Config = (),
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
        InitError = (),
    >,
> {
    App::new().app_data(etat).configure(configurer)
}
