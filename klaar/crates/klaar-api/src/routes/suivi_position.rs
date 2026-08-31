//! Suivi géolocalisé du trajet (Story 4.4, FR-019).
//!
//! Trois routes, deux publics : le prestataire consent et envoie, le demandeur
//! regarde. Le raisonnement est dans
//! `klaar_application::usecases::suivre_position` ; ce module ne fait que le
//! transport et la traduction des refus en codes HTTP.

use actix_web::{get, post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use klaar_application::usecases::suivre_position::{
    consentir, consulter, relever_position, ErreurSuivi,
};
use klaar_intervention::PERTE_POSITION_SECONDES;
use klaar_shared_kernel::Geo;

use crate::auth::{Authentifie, ErreurAuthDto};
use crate::routes::auth::ErreurValidationDto;
use crate::EtatApplication;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConsentementSuiviDto {
    /// `true` pour partager sa position pendant le trajet, `false` pour cesser.
    pub accepte: bool,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EtatConsentementDto {
    pub code: &'static str,
    pub consenti: bool,
}

#[derive(Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PositionDto {
    pub lat: f64,
    pub lon: f64,
}

/// Ce que l'application du prestataire reçoit en retour de son envoi.
///
/// **La position rendue est la position dégradée**, pas celle qui a été
/// envoyée : le prestataire voit ainsi exactement ce que le demandeur verra,
/// et la grille de 50 m cesse d'être une promesse invisible.
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReleveDto {
    pub code: &'static str,
    pub lat: f64,
    pub lon: f64,
    pub hors_zone: bool,
    pub relevee_le: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VueSuiviDto {
    /// `EN_ROUTE`, `POSITION_LOST`, `OUT_OF_ZONE` ou `STOPPED`.
    pub etat: String,
    /// Dernière position connue, déjà dégradée à 50 m. Absente tant que rien
    /// n'a été partagé : la carte dit alors « position non partagée » plutôt
    /// que de rester vide sans raison.
    pub position: Option<PositionDto>,
    pub relevee_le: Option<String>,
    /// Au-delà de ce délai sans relevé, la position est déclarée perdue. Exposé
    /// pour que le front n'ait pas à redéclarer la même constante.
    pub perte_apres_secondes: i64,
}

fn statut(e: &ErreurSuivi) -> actix_web::http::StatusCode {
    use actix_web::http::StatusCode;
    match e {
        ErreurSuivi::Introuvable => StatusCode::NOT_FOUND,
        ErreurSuivi::Domaine(d) => match d.code() {
            // 403 : le refus vient du prestataire lui-même, qui n'a pas
            // consenti ou s'est rétracté. Ce n'est pas une donnée invalide,
            // c'est un droit exercé.
            "TRACKING_NOT_CONSENTED" => StatusCode::FORBIDDEN,
            _ => StatusCode::UNPROCESSABLE_ENTITY,
        },
        ErreurSuivi::Indisponible(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn echec(e: ErreurSuivi, quoi: &str) -> HttpResponse {
    if matches!(e, ErreurSuivi::Indisponible(_)) {
        tracing::error!(erreur = %e, "{quoi}");
    }
    HttpResponse::build(statut(&e)).json(ErreurValidationDto {
        code: e.code().to_string(),
    })
}

/// Le prestataire accepte ou retire le partage de sa position.
#[utoipa::path(
    post,
    path = "/api/v1/missions/{id}/tracking/consent",
    tag = "missions",
    params(("id" = Uuid, Path, description = "Identifiant de la Mission")),
    request_body = ConsentementSuiviDto,
    responses(
        (status = 200, description = "Consentement enregistré", body = EtatConsentementDto),
        (status = 400, description = "Identifiant illisible", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou invalide", body = ErreurAuthDto),
        (status = 404, description = "Mission inconnue ou qui ne vous concerne pas", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[post("/api/v1/missions/{id}/tracking/consent")]
pub async fn consentir_suivi(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    chemin: web::Path<String>,
    corps: web::Json<ConsentementSuiviDto>,
) -> HttpResponse {
    let Ok(mission_id) = Uuid::parse_str(&chemin) else {
        return HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "MISSION_ID_INVALID".to_string(),
        });
    };

    match consentir(
        etat.missions.as_ref(),
        etat.prestataires.as_ref(),
        etat.suivis.as_ref(),
        etat.horloge.as_ref(),
        authentifie.utilisateur_id,
        mission_id,
        corps.accepte,
    )
    .await
    {
        Ok(consenti) => HttpResponse::Ok().json(EtatConsentementDto {
            code: if consenti {
                "TRACKING_CONSENTED"
            } else {
                "TRACKING_WITHDRAWN"
            },
            consenti,
        }),
        Err(e) => echec(e, "consentement au suivi impossible"),
    }
}

/// Le prestataire envoie sa position pendant le trajet.
#[utoipa::path(
    post,
    path = "/api/v1/missions/{id}/tracking",
    tag = "missions",
    params(("id" = Uuid, Path, description = "Identifiant de la Mission")),
    request_body = PositionDto,
    responses(
        (status = 201, description = "Position enregistrée, dégradée à 50 m", body = ReleveDto),
        (status = 400, description = "Identifiant ou coordonnées illisibles", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou invalide", body = ErreurAuthDto),
        (status = 403, description = "Partage non consenti ou retiré", body = ErreurValidationDto),
        (status = 404, description = "Mission inconnue ou qui ne vous concerne pas", body = ErreurValidationDto),
        (status = 422, description = "Intervention hors trajet", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[post("/api/v1/missions/{id}/tracking")]
pub async fn relever_suivi(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    chemin: web::Path<String>,
    corps: web::Json<PositionDto>,
) -> HttpResponse {
    let Ok(mission_id) = Uuid::parse_str(&chemin) else {
        return HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "MISSION_ID_INVALID".to_string(),
        });
    };
    let Ok(position) = Geo::new(corps.lat, corps.lon) else {
        return HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "POSITION_INVALID".to_string(),
        });
    };

    match relever_position(
        etat.missions.as_ref(),
        etat.prestataires.as_ref(),
        etat.suivis.as_ref(),
        etat.horloge.as_ref(),
        authentifie.utilisateur_id,
        mission_id,
        position,
    )
    .await
    {
        Ok(releve) => HttpResponse::Created().json(ReleveDto {
            code: "TRACKING_RECORDED",
            lat: releve.position.lat(),
            lon: releve.position.lon(),
            hors_zone: releve.hors_zone,
            relevee_le: releve.relevee_le.to_rfc3339(),
        }),
        Err(e) => echec(e, "relevé de position impossible"),
    }
}

/// Le demandeur regarde où en est le prestataire.
#[utoipa::path(
    get,
    path = "/api/v1/missions/{id}/tracking",
    tag = "missions",
    params(("id" = Uuid, Path, description = "Identifiant de la Mission")),
    responses(
        (status = 200, description = "État du trajet", body = VueSuiviDto),
        (status = 400, description = "Identifiant illisible", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou invalide", body = ErreurAuthDto),
        (status = 404, description = "Mission inconnue ou qui ne vous concerne pas", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[get("/api/v1/missions/{id}/tracking")]
pub async fn consulter_suivi(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    chemin: web::Path<String>,
) -> HttpResponse {
    let Ok(mission_id) = Uuid::parse_str(&chemin) else {
        return HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "MISSION_ID_INVALID".to_string(),
        });
    };

    match consulter(
        etat.demandes.as_ref(),
        etat.missions.as_ref(),
        etat.suivis.as_ref(),
        etat.horloge.as_ref(),
        authentifie.utilisateur_id,
        mission_id,
    )
    .await
    {
        Ok(vue) => HttpResponse::Ok().json(VueSuiviDto {
            etat: vue.etat.as_str().to_string(),
            position: vue.derniere.map(|p| PositionDto {
                lat: p.position.lat(),
                lon: p.position.lon(),
            }),
            relevee_le: vue.derniere.map(|p| p.relevee_le.to_rfc3339()),
            perte_apres_secondes: PERTE_POSITION_SECONDES,
        }),
        Err(e) => echec(e, "consultation du suivi impossible"),
    }
}
