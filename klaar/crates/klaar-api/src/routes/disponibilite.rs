//! Disponibilité et rayon d'intervention du prestataire (Story 3.7).

use actix_web::{get, patch, web, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use klaar_application::usecases::disponibilite::{
    consulter, regler, ErreurDisponibilite, EtatDisponibilite,
};

use crate::auth::Authentifie;
use crate::routes::auth::ErreurValidationDto;
use crate::EtatApplication;

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DisponibiliteDto {
    pub provider_id: String,
    /// `PENDING_KYC`, `ACTIVE` ou `SUSPENDED`.
    pub statut: String,
    /// En service, ou en pause.
    pub disponible: bool,
    /// Distance au-delà de laquelle le prestataire ne se déplace pas.
    pub rayon_intervention_metres: f64,
    /// Une Mission en cours l'empêche d'en prendre une autre.
    ///
    /// Ne se règle pas : c'est un fait. L'exposer évite qu'un prestataire en
    /// service et pourtant jamais sollicité en conclue que le service est
    /// cassé.
    pub occupe: bool,
    /// Reçoit effectivement des Demandes en ce moment.
    ///
    /// La conjonction des trois : statut, disponibilité, occupation. C'est la
    /// seule réponse à la question qu'il se pose réellement.
    pub sollicitable: bool,
}

impl From<EtatDisponibilite> for DisponibiliteDto {
    fn from(e: EtatDisponibilite) -> Self {
        Self {
            provider_id: e.provider_id.to_string(),
            statut: e.statut.to_string(),
            disponible: e.disponible,
            rayon_intervention_metres: e.rayon_intervention_metres,
            occupe: e.occupe,
            sollicitable: e.sollicitable,
        }
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReglageDto {
    /// Se mettre en service ou en pause. Absent : inchangé.
    pub disponible: Option<bool>,
    /// Rayon d'intervention, entre 1 000 et 20 000 mètres. Absent : inchangé.
    pub rayon_intervention_metres: Option<f64>,
}

fn statut(e: &ErreurDisponibilite) -> actix_web::http::StatusCode {
    use actix_web::http::StatusCode;
    match e {
        ErreurDisponibilite::PasPrestataire => StatusCode::FORBIDDEN,
        ErreurDisponibilite::Reglage(_) => StatusCode::BAD_REQUEST,
        ErreurDisponibilite::Indisponible(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// Lit sa disponibilité.
#[utoipa::path(
    get,
    path = "/api/v1/providers/me/availability",
    tag = "prestataires",
    responses(
        (status = 200, description = "État de disponibilité", body = DisponibiliteDto),
        (status = 401, description = "Jeton absent ou invalide"),
        (status = 403, description = "Ce compte n'est pas un prestataire", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[get("/api/v1/providers/me/availability")]
pub async fn lire_disponibilite(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
) -> HttpResponse {
    repondre(
        consulter(
            etat.prestataires.as_ref(),
            etat.missions.as_ref(),
            // Tiré du jeton : il n'existe aucun chemin pour lire la fiche d'un
            // autre.
            authentifie.utilisateur_id,
        )
        .await,
    )
}

/// Se met en service ou en pause, et règle son rayon d'intervention.
#[utoipa::path(
    patch,
    path = "/api/v1/providers/me/availability",
    tag = "prestataires",
    request_body = ReglageDto,
    responses(
        (status = 200, description = "État après réglage", body = DisponibiliteDto),
        (status = 400, description = "Rayon hors bornes", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou invalide"),
        (status = 403, description = "Ce compte n'est pas un prestataire", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[patch("/api/v1/providers/me/availability")]
pub async fn regler_disponibilite(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    corps: web::Json<ReglageDto>,
) -> HttpResponse {
    repondre(
        regler(
            etat.prestataires.as_ref(),
            etat.missions.as_ref(),
            authentifie.utilisateur_id,
            corps.disponible,
            corps.rayon_intervention_metres,
        )
        .await,
    )
}

fn repondre(resultat: Result<EtatDisponibilite, ErreurDisponibilite>) -> HttpResponse {
    match resultat {
        Ok(etat) => HttpResponse::Ok().json(DisponibiliteDto::from(etat)),
        Err(e) => {
            if matches!(e, ErreurDisponibilite::Indisponible(_)) {
                tracing::error!(erreur = %e, "disponibilité indisponible");
            }
            HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    }
}
