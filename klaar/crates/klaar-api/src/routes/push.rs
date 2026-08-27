//! Abonnements Web Push (Story 0.12, ADR-010).
//!
//! Trois endpoints seulement : donner au navigateur la clé publique dont il a
//! besoin, enregistrer l'abonnement qu'il produit, et le retirer. Le reste du
//! protocole est côté adaptateur.

use actix_web::{delete, get, post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use klaar_application::ports::push::PushSubscription;
use klaar_application::ports::push_repository::PushSubscriptionRepository;

use crate::EtatApplication;

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ClePubliqueDto {
    /// Clé publique VAPID, base64url, forme non compressée. C'est
    /// l'`applicationServerKey` attendue par `PushManager.subscribe`.
    pub cle: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ClesAbonnementDto {
    pub p256dh: String,
    pub auth: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AbonnementDto {
    pub endpoint: String,
    pub keys: ClesAbonnementDto,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AbonnementEnregistreDto {
    pub id: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DesabonnementDto {
    pub endpoint: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ErreurDto {
    pub erreur: String,
}

/// Clé publique VAPID à passer à `PushManager.subscribe`.
#[utoipa::path(
    get,
    path = "/api/v1/push/cle-publique",
    tag = "push",
    responses(
        (status = 200, description = "Clé publique VAPID", body = ClePubliqueDto),
        (status = 503, description = "Push non configuré sur ce déploiement", body = ErreurDto),
    )
)]
#[get("/api/v1/push/cle-publique")]
pub async fn cle_publique(etat: web::Data<EtatApplication>) -> HttpResponse {
    match &etat.push {
        Some(sender) => HttpResponse::Ok().json(ClePubliqueDto {
            cle: sender.cle_publique_base64url(),
        }),
        // 503 et non 500 : ce n'est pas une panne, c'est une fonctionnalité
        // non configurée. Le client doit pouvoir le distinguer pour masquer
        // l'invitation à activer les notifications plutôt qu'afficher une
        // erreur.
        None => HttpResponse::ServiceUnavailable().json(ErreurDto {
            erreur: "notifications push non configurées".to_string(),
        }),
    }
}

/// Enregistre l'abonnement produit par le navigateur.
#[utoipa::path(
    post,
    path = "/api/v1/push/abonnements",
    tag = "push",
    request_body = AbonnementDto,
    responses(
        (status = 201, description = "Abonnement enregistré", body = AbonnementEnregistreDto),
        (status = 400, description = "Abonnement mal formé", body = ErreurDto),
        (status = 503, description = "Dépôt indisponible", body = ErreurDto),
    )
)]
#[post("/api/v1/push/abonnements")]
pub async fn enregistrer_abonnement(
    etat: web::Data<EtatApplication>,
    corps: web::Json<AbonnementDto>,
) -> HttpResponse {
    let abonnement = PushSubscription {
        endpoint: corps.endpoint.clone(),
        p256dh: corps.keys.p256dh.clone(),
        auth: corps.keys.auth.clone(),
    };

    // Valider ici plutôt qu'au premier envoi : un abonnement mal formé accepté
    // aujourd'hui devient une notification perdue dans six semaines, sans
    // rien pour relier les deux.
    if let Err(e) = klaar_push_adapter::valider_abonnement(&abonnement) {
        return HttpResponse::BadRequest().json(ErreurDto {
            erreur: e.to_string(),
        });
    }

    match etat.abonnements.enregistrer(&abonnement, None).await {
        Ok(enregistre) => HttpResponse::Created().json(AbonnementEnregistreDto {
            id: enregistre.id.to_string(),
        }),
        Err(e) => {
            tracing::error!(erreur = %e, "enregistrement d'abonnement push impossible");
            HttpResponse::ServiceUnavailable().json(ErreurDto {
                erreur: "dépôt indisponible".to_string(),
            })
        }
    }
}

/// Retire un abonnement.
#[utoipa::path(
    delete,
    path = "/api/v1/push/abonnements",
    tag = "push",
    request_body = DesabonnementDto,
    responses(
        (status = 204, description = "Abonnement retiré, ou déjà absent"),
        (status = 503, description = "Dépôt indisponible", body = ErreurDto),
    )
)]
#[delete("/api/v1/push/abonnements")]
pub async fn supprimer_abonnement(
    etat: web::Data<EtatApplication>,
    corps: web::Json<DesabonnementDto>,
) -> HttpResponse {
    match etat
        .abonnements
        .supprimer_par_endpoint(&corps.endpoint)
        .await
    {
        // 204 que la ligne ait existé ou non : répondre 404 sur un endpoint
        // absent transformerait cette route en oracle permettant de tester
        // l'existence d'un abonnement arbitraire.
        Ok(_) => HttpResponse::NoContent().finish(),
        Err(e) => {
            tracing::error!(erreur = %e, "suppression d'abonnement push impossible");
            HttpResponse::ServiceUnavailable().json(ErreurDto {
                erreur: "dépôt indisponible".to_string(),
            })
        }
    }
}
