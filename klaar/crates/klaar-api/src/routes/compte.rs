//! Compte de l'utilisateur authentifié : effacement (Story 1.9, FR-005).

use actix_web::{post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use klaar_application::usecases::effacer::{
    annuler, demander, ErreurEffacement, ResultatDemande, CONFIRMATION_ATTENDUE,
};

use crate::auth::{Authentifie, ErreurAuthDto};
use crate::routes::auth::ErreurValidationDto;
use crate::EtatApplication;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EffacementDto {
    /// Doit valoir exactement `DELETE`.
    ///
    /// Un mot à reproduire plutôt qu'un booléen : `{"confirme": true}` se coche
    /// par mégarde ou se rejoue depuis un autre onglet, alors que recopier un
    /// mot demande une intention.
    pub confirmation: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EffacementProgrammeDto {
    /// `ERASURE_SCHEDULED` à la première demande, `ERASURE_ALREADY_SCHEDULED`
    /// ensuite.
    pub code: &'static str,
    /// Jours restants avant exécution, pendant lesquels l'annulation reste
    /// possible.
    pub dans_jours: i64,
}

fn statut(e: &ErreurEffacement) -> actix_web::http::StatusCode {
    use actix_web::http::StatusCode;
    match e {
        ErreurEffacement::ConfirmationInvalide => StatusCode::BAD_REQUEST,
        // 404 et non 403 : du point de vue du porteur du jeton, le compte
        // n'existe plus. Il n'y a rien à protéger, seulement rien à trouver.
        ErreurEffacement::CompteIntrouvable => StatusCode::NOT_FOUND,
        ErreurEffacement::AucuneDemande => StatusCode::CONFLICT,
        ErreurEffacement::Indisponible(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// Demande l'effacement de son propre compte (RGPD art. 17).
#[utoipa::path(
    post,
    path = "/api/v1/me/erase",
    tag = "compte",
    request_body = EffacementDto,
    responses(
        (status = 202, description = "Effacement programmé", body = EffacementProgrammeDto),
        (status = 400, description = "Confirmation absente ou fautive", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou invalide", body = ErreurAuthDto),
        (status = 404, description = "Compte introuvable", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[post("/api/v1/me/erase")]
pub async fn effacer_mon_compte(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    corps: web::Json<EffacementDto>,
) -> HttpResponse {
    match demander(
        etat.utilisateurs.as_ref(),
        etat.journal.as_ref(),
        etat.horloge.as_ref(),
        authentifie.utilisateur_id,
        &corps.confirmation,
    )
    .await
    {
        Ok(ResultatDemande::Programme { dans_jours }) => {
            HttpResponse::Accepted().json(EffacementProgrammeDto {
                code: "ERASURE_SCHEDULED",
                dans_jours,
            })
        }
        // 202 aussi : redemander n'est pas un conflit, c'est un second clic.
        // Répondre 409 ferait passer pour une erreur ce qui est l'état voulu.
        Ok(ResultatDemande::DejaProgramme) => {
            HttpResponse::Accepted().json(EffacementProgrammeDto {
                code: "ERASURE_ALREADY_SCHEDULED",
                dans_jours: klaar_identity::DELAI_EFFACEMENT_JOURS,
            })
        }
        Err(e) => {
            if matches!(e, ErreurEffacement::Indisponible(_)) {
                tracing::error!(erreur = %e, "effacement impossible");
            }
            HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    }
}

/// Annule une demande d'effacement pendant le délai de grâce.
///
/// N'existe pas dans FR-005, et découle pourtant de lui : un délai de trente
/// jours n'a de raison d'être que s'il est réversible.
#[utoipa::path(
    post,
    path = "/api/v1/me/erase/cancel",
    tag = "compte",
    responses(
        (status = 204, description = "Effacement annulé"),
        (status = 401, description = "Jeton absent ou invalide", body = ErreurAuthDto),
        (status = 404, description = "Compte introuvable", body = ErreurValidationDto),
        (status = 409, description = "Aucun effacement en attente", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[post("/api/v1/me/erase/cancel")]
pub async fn annuler_mon_effacement(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
) -> HttpResponse {
    match annuler(
        etat.utilisateurs.as_ref(),
        etat.journal.as_ref(),
        etat.horloge.as_ref(),
        authentifie.utilisateur_id,
    )
    .await
    {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(e) => {
            if matches!(e, ErreurEffacement::Indisponible(_)) {
                tracing::error!(erreur = %e, "annulation d'effacement impossible");
            }
            HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    }
}

/// Confirmation attendue, exposée pour que le frontend n'en invente pas une autre.
pub const MOT_DE_CONFIRMATION: &str = CONFIRMATION_ATTENDUE;
