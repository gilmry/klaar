//! Ouverture d'un litige sur une intervention (Story 7.2, FR-034).

use actix_web::{get, post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use klaar_application::ports::litige_repository::LitigeRepository;
use klaar_application::ports::provider_repository::ProviderRepository;
use klaar_application::usecases::ouvrir_litige::{ouvrir, ErreurLitige, Grief};

use crate::auth::Authentifie;
use crate::routes::auth::ErreurValidationDto;
use crate::EtatApplication;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LitigeDto {
    /// `QUALITY`, `NOT_DONE`, `AMOUNT_DISPUTED` pour le demandeur ;
    /// `USER_NO_SHOW`, `IMPOSSIBLE_CONDITIONS` pour le prestataire ; `OTHER`
    /// pour les deux.
    pub motif: String,
    /// Ce qui s'est passé, en vingt caractères au moins : « pas content » ne
    /// permet à personne de trancher.
    pub description: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LitigeOuvertDto {
    pub id: String,
    pub code: &'static str,
    /// `USER` ou `PROVIDER` : déduit de votre rôle dans l'intervention.
    pub partie: String,
    pub statut: String,
    /// Vrai quand plusieurs litiges ont été ouverts en peu de temps par ce
    /// compte. Ce n'est pas une sanction, c'est un examen.
    pub a_examiner: bool,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LitigeLuDto {
    pub id: String,
    pub partie: String,
    pub motif: String,
    pub description: String,
    pub statut: String,
    /// En RFC 3339.
    pub ouvert_le: String,
}

fn statut(e: &ErreurLitige) -> actix_web::http::StatusCode {
    use actix_web::http::StatusCode;
    match e {
        ErreurLitige::Introuvable => StatusCode::NOT_FOUND,
        ErreurLitige::PasTerminee => StatusCode::CONFLICT,
        ErreurLitige::DejaLitigee => StatusCode::CONFLICT,
        ErreurLitige::MotifInconnu => StatusCode::BAD_REQUEST,
        ErreurLitige::Domaine(d) => match d.code() {
            // 410 : la fenêtre a existé et s'est refermée. FR-034 `@negative`
            // le demande.
            "DISPUTE_WINDOW_CLOSED" => StatusCode::GONE,
            // 422 : la requête est bien formée, c'est son contenu qui ne suffit
            // pas — description trop courte, motif hors propos.
            _ => StatusCode::UNPROCESSABLE_ENTITY,
        },
        ErreurLitige::Indisponible(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// Ouvre un litige sur une intervention terminée.
#[utoipa::path(
    post,
    path = "/api/v1/missions/{id}/dispute",
    tag = "litige",
    params(("id" = String, Path, description = "Identifiant de la Mission")),
    request_body = LitigeDto,
    responses(
        (status = 201, description = "Litige ouvert", body = LitigeOuvertDto),
        (status = 400, description = "Identifiant illisible ou motif inconnu", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou invalide"),
        (status = 404, description = "Mission inconnue ou qui ne vous concerne pas", body = ErreurValidationDto),
        (status = 409, description = "Intervention non terminée, ou déjà litigée", body = ErreurValidationDto),
        (status = 410, description = "Fenêtre de litige fermée", body = ErreurValidationDto),
        (status = 422, description = "Description insuffisante ou motif hors propos", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[post("/api/v1/missions/{id}/dispute")]
pub async fn ouvrir_litige(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    chemin: web::Path<String>,
    corps: web::Json<LitigeDto>,
) -> HttpResponse {
    let Ok(mission_id) = Uuid::parse_str(&chemin) else {
        return HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "MISSION_ID_INVALID".to_string(),
        });
    };

    match ouvrir(
        etat.litiges.as_ref(),
        etat.prestataires.as_ref(),
        etat.horloge.as_ref(),
        // Tiré du jeton : c'est lui qui dit de quel côté on se plaint, et
        // accepter le rôle en entrée fausserait tout comptage de sanctions.
        authentifie.utilisateur_id,
        mission_id,
        Grief {
            motif: &corps.motif,
            description: &corps.description,
        },
    )
    .await
    {
        Ok(ouvert) => {
            if ouvert.a_examiner {
                // Un signal, pas une sanction : quelqu'un peut légitimement
                // tomber deux fois sur un mauvais prestataire.
                tracing::warn!("plusieurs litiges ouverts en peu de temps par le même compte");
            }
            HttpResponse::Created().json(LitigeOuvertDto {
                id: ouvert.litige.id.to_string(),
                code: "DISPUTE_OPENED",
                partie: ouvert.litige.partie.as_str().to_string(),
                statut: ouvert.litige.statut.as_str().to_string(),
                a_examiner: ouvert.a_examiner,
            })
        }
        Err(e) => {
            if matches!(e, ErreurLitige::Indisponible(_)) {
                tracing::error!(erreur = %e, "ouverture de litige impossible");
            }
            HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    }
}

/// Lit le litige d'une intervention, s'il y en a un.
#[utoipa::path(
    get,
    path = "/api/v1/missions/{id}/dispute",
    tag = "litige",
    params(("id" = String, Path, description = "Identifiant de la Mission")),
    responses(
        (status = 200, description = "Le litige", body = LitigeLuDto),
        (status = 400, description = "Identifiant illisible", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou invalide"),
        (status = 404, description = "Aucun litige, ou intervention hors de portée", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[get("/api/v1/missions/{id}/dispute")]
pub async fn lire_litige(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    chemin: web::Path<String>,
) -> HttpResponse {
    let Ok(mission_id) = Uuid::parse_str(&chemin) else {
        return HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "MISSION_ID_INVALID".to_string(),
        });
    };

    // Les droits sont vérifiés par le contexte, comme à l'ouverture : seules
    // les deux parties lisent le récit, qui est celui de l'une d'elles.
    let Ok(Some(contexte)) = etat.litiges.contexte(mission_id).await else {
        return HttpResponse::NotFound().json(ErreurValidationDto {
            code: "MISSION_NOT_FOUND".to_string(),
        });
    };
    let concerne = contexte.demandeur_id == authentifie.utilisateur_id
        || matches!(
            etat.prestataires.par_utilisateur_id(authentifie.utilisateur_id).await,
            Ok(Some(p)) if p.id == contexte.provider_id
        );
    if !concerne {
        return HttpResponse::NotFound().json(ErreurValidationDto {
            code: "MISSION_NOT_FOUND".to_string(),
        });
    }

    match etat.litiges.par_mission(mission_id).await {
        Ok(Some(l)) => HttpResponse::Ok().json(LitigeLuDto {
            id: l.id.to_string(),
            partie: l.partie.as_str().to_string(),
            motif: l.motif.as_str().to_string(),
            description: l.description,
            statut: l.statut.as_str().to_string(),
            ouvert_le: l.ouvert_le.to_rfc3339(),
        }),
        Ok(None) => HttpResponse::NotFound().json(ErreurValidationDto {
            code: "DISPUTE_NOT_FOUND".to_string(),
        }),
        Err(e) => {
            tracing::error!(erreur = %e, "lecture du litige impossible");
            HttpResponse::ServiceUnavailable().json(ErreurValidationDto {
                code: "SERVICE_UNAVAILABLE".to_string(),
            })
        }
    }
}
