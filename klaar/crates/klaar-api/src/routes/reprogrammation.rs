//! Reprogrammation d'une intervention annulée (Story 4.8, FR-023).

use actix_web::{post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use klaar_application::usecases::reprogrammer::{proposer, repondre, ErreurReprogrammation};

use crate::auth::Authentifie;
use crate::routes::auth::ErreurValidationDto;
use crate::EtatApplication;

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReprogrammationDto {
    pub id: String,
    pub code: &'static str,
    pub statut: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReponseReprogrammationDto {
    /// `true` pour accepter, `false` pour décliner.
    pub accepte: bool,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RepriseDto {
    pub code: &'static str,
    /// La nouvelle intervention, quand la proposition est acceptée.
    pub nouvelle_mission_id: Option<String>,
}

fn statut(e: &ErreurReprogrammation) -> actix_web::http::StatusCode {
    use actix_web::http::StatusCode;
    match e {
        ErreurReprogrammation::Introuvable => StatusCode::NOT_FOUND,
        ErreurReprogrammation::DejaProposee
        | ErreurReprogrammation::DejaClose
        | ErreurReprogrammation::ProviderOccupe => StatusCode::CONFLICT,
        ErreurReprogrammation::Domaine(d) => match d.code() {
            // 410 : la fenêtre a existé et s'est refermée. FR-023 `@edge` le
            // demande.
            "RESCHEDULE_EXPIRED" => StatusCode::GONE,
            // 409 : le prestataire a déjà décliné. FR-023 `@negative`.
            "PROVIDER_DECLINED" => StatusCode::CONFLICT,
            _ => StatusCode::UNPROCESSABLE_ENTITY,
        },
        ErreurReprogrammation::Indisponible(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// Propose de reprendre une intervention annulée.
#[utoipa::path(
    post,
    path = "/api/v1/missions/{id}/reschedule",
    tag = "missions",
    params(("id" = String, Path, description = "Identifiant de la Mission annulée")),
    responses(
        (status = 201, description = "Proposition enregistrée", body = ReprogrammationDto),
        (status = 400, description = "Identifiant illisible", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou invalide"),
        (status = 404, description = "Mission inconnue ou qui ne vous concerne pas", body = ErreurValidationDto),
        (status = 409, description = "Déjà proposée, ou déjà déclinée", body = ErreurValidationDto),
        (status = 410, description = "Fenêtre de reprogrammation fermée", body = ErreurValidationDto),
        (status = 422, description = "Intervention non annulée, ou sans devis accepté", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[post("/api/v1/missions/{id}/reschedule")]
pub async fn proposer_reprogrammation(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    chemin: web::Path<String>,
) -> HttpResponse {
    let Ok(mission_id) = Uuid::parse_str(&chemin) else {
        return HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "MISSION_ID_INVALID".to_string(),
        });
    };

    match proposer(
        etat.reprogrammations.as_ref(),
        etat.horloge.as_ref(),
        authentifie.utilisateur_id,
        mission_id,
    )
    .await
    {
        Ok(p) => HttpResponse::Created().json(ReprogrammationDto {
            id: p.id.to_string(),
            code: "RESCHEDULE_PROPOSED",
            statut: p.statut.as_str().to_string(),
        }),
        Err(e) => {
            if matches!(e, ErreurReprogrammation::Indisponible(_)) {
                tracing::error!(erreur = %e, "proposition de reprogrammation impossible");
            }
            HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    }
}

/// Le prestataire accepte ou décline la reprogrammation.
#[utoipa::path(
    post,
    path = "/api/v1/missions/{id}/reschedule/answer",
    tag = "missions",
    params(("id" = String, Path, description = "Identifiant de la Mission annulée")),
    request_body = ReponseReprogrammationDto,
    responses(
        (status = 200, description = "Réponse enregistrée", body = RepriseDto),
        (status = 400, description = "Identifiant illisible", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou invalide"),
        (status = 404, description = "Mission inconnue ou qui ne vous concerne pas", body = ErreurValidationDto),
        (status = 409, description = "Déjà répondue, ou prestataire déjà engagé", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[post("/api/v1/missions/{id}/reschedule/answer")]
pub async fn repondre_reprogrammation(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    chemin: web::Path<String>,
    corps: web::Json<ReponseReprogrammationDto>,
) -> HttpResponse {
    let Ok(mission_id) = Uuid::parse_str(&chemin) else {
        return HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "MISSION_ID_INVALID".to_string(),
        });
    };

    match repondre(
        etat.reprogrammations.as_ref(),
        etat.prestataires.as_ref(),
        etat.horloge.as_ref(),
        authentifie.utilisateur_id,
        mission_id,
        corps.accepte,
    )
    .await
    {
        Ok(reprise) => HttpResponse::Ok().json(RepriseDto {
            code: if reprise.is_some() {
                "RESCHEDULE_ACCEPTED"
            } else {
                "RESCHEDULE_DECLINED"
            },
            nouvelle_mission_id: reprise.map(|r| r.nouvelle_mission.to_string()),
        }),
        Err(e) => {
            if matches!(e, ErreurReprogrammation::Indisponible(_)) {
                tracing::error!(erreur = %e, "réponse à la reprogrammation impossible");
            }
            HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    }
}
