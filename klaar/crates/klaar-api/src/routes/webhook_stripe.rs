//! Endpoint de webhook Stripe (Story 5.5, FR-028).
//!
//! **Le seul endpoint public qui écrit.** Il n'a pas de jeton : Stripe appelle
//! depuis des adresses qui changent. C'est la signature HMAC qui authentifie,
//! et elle est vérifiée avant que la charge ne soit même analysée.
//!
//! **Le corps est lu brut.** Le désérialiser puis le re-sérialiser pour
//! vérifier la signature changerait un espace ou l'ordre d'une clé, et la
//! vérification échouerait sur des appels parfaitement valides. C'est l'erreur
//! classique de cette intégration ; `web::Bytes` l'évite par construction.

use actix_web::{post, web, HttpRequest, HttpResponse};
use serde::Serialize;
use utoipa::ToSchema;

use klaar_application::usecases::webhook_stripe::{recevoir, ErreurWebhook};
use klaar_stripe_adapter::Suite;

use crate::routes::auth::ErreurValidationDto;
use crate::EtatApplication;

/// En-tête que Stripe pose sur chaque appel.
const ENTETE_SIGNATURE: &str = "Stripe-Signature";

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AccuseWebhookDto {
    /// `APPLIED`, `DUPLICATE`, `SUPERSEDED` ou `IGNORED`.
    pub suite: &'static str,
    /// **Faux tant qu'aucun séquestre n'existe en base.** Rendu explicitement
    /// pour que personne ne déduise d'un 200 que l'argent a bougé.
    pub effet_applique: bool,
}

fn libelle(suite: Suite) -> &'static str {
    match suite {
        Suite::Appliquer => "APPLIED",
        Suite::DejaTraite => "DUPLICATE",
        Suite::Depasse => "SUPERSEDED",
        Suite::Ignore => "IGNORED",
    }
}

/// Reçoit un événement Stripe.
#[utoipa::path(
    post,
    path = "/api/v1/webhooks/stripe",
    tag = "paiement",
    request_body(
        content = String,
        description = "Charge JSON brute de Stripe, signée par l'en-tête `Stripe-Signature`",
        content_type = "application/json"
    ),
    responses(
        (status = 200, description = "Événement accusé — reçu, dupliqué, dépassé ou ignoré", body = AccuseWebhookDto),
        (status = 400, description = "Signature absente ou invalide, ou charge illisible", body = ErreurValidationDto),
        (status = 503, description = "Service ou webhook non configuré", body = ErreurValidationDto),
    )
)]
#[post("/api/v1/webhooks/stripe")]
pub async fn recevoir_webhook(
    etat: web::Data<EtatApplication>,
    requete: HttpRequest,
    corps: web::Bytes,
) -> HttpResponse {
    // Sans secret configuré, l'endpoint refuse **tout**, y compris ce qui
    // serait authentique. Le laisser passer « puisqu'il n'y a rien à vérifier »
    // ferait d'une configuration oubliée une porte ouverte.
    let Some(secret) = etat.secret_webhook_stripe.as_deref() else {
        return HttpResponse::ServiceUnavailable().json(ErreurValidationDto {
            code: "WEBHOOK_NOT_CONFIGURED".to_string(),
        });
    };

    let entete = requete
        .headers()
        .get(ENTETE_SIGNATURE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    match recevoir(
        etat.evenements_stripe.as_ref(),
        etat.horloge.as_ref(),
        &corps,
        entete,
        secret.as_bytes(),
    )
    .await
    {
        // **200 sur un doublon comme sur un neuf** (FR-028 `@negative`).
        // Répondre autre chose ferait réessayer Stripe indéfiniment pour un
        // événement déjà traité, puis désactiver l'endpoint.
        Ok(reception) => HttpResponse::Ok().json(AccuseWebhookDto {
            suite: libelle(reception.suite),
            effet_applique: reception.effet_applique,
        }),
        Err(ErreurWebhook::Indisponible(detail)) => {
            // 503 et non 400 : Stripe doit réessayer, c'est nous qui sommes
            // en panne. Un 400 lui ferait abandonner un événement réel.
            tracing::error!(erreur = %detail, "réception de webhook impossible");
            HttpResponse::ServiceUnavailable().json(ErreurValidationDto {
                code: "SERVICE_UNAVAILABLE".to_string(),
            })
        }
        Err(e) => {
            // Journalisé sans le corps ni l'en-tête : une charge non
            // authentifiée n'a rien à faire dans les journaux, et l'en-tête
            // porte une signature.
            tracing::warn!(code = e.code(), "webhook Stripe rejeté");
            HttpResponse::BadRequest().json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    }
}
