//! Vérification de l'adresse email (Story 1.2, FR-001).

use actix_web::{post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use klaar_application::usecases::verifier_email::{
    verifier_email, ErreurVerification, ResultatVerification,
};

use crate::routes::auth::ErreurValidationDto;
use crate::EtatApplication;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VerificationDto {
    /// Jeton reçu par courriel, tel quel.
    pub jeton: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VerificationFaiteDto {
    /// `EMAIL_VERIFIED` à la première présentation du jeton,
    /// `EMAIL_ALREADY_VERIFIED` ensuite.
    pub code: &'static str,
}

/// Active un compte à partir du jeton reçu par courriel.
///
/// **`POST` et non `GET`**, contrairement au tableau des endpoints du PRD. Le
/// lien du courriel ouvre la page `/verifier-email` de la PWA, qui présente
/// ensuite le jeton ici. Les passerelles de messagerie d'entreprise visitent
/// les liens des courriels avant leur destinataire : un `GET` qui consomme le
/// jeton est consommé par l'antivirus, et l'utilisateur trouve un lien déjà
/// utilisé au moment où il clique.
#[utoipa::path(
    post,
    path = "/api/v1/auth/verify-email",
    tag = "authentification",
    request_body = VerificationDto,
    responses(
        (status = 200, description = "Adresse vérifiée, ou déjà vérifiée", body = VerificationFaiteDto),
        (status = 400, description = "Jeton absent", body = ErreurValidationDto),
        (status = 404, description = "Jeton inconnu", body = ErreurValidationDto),
        (status = 410, description = "Jeton expiré", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    )
)]
#[post("/api/v1/auth/verify-email")]
pub async fn verifier(
    etat: web::Data<EtatApplication>,
    corps: web::Json<VerificationDto>,
) -> HttpResponse {
    match verifier_email(
        etat.utilisateurs.as_ref(),
        etat.journal.as_ref(),
        etat.horloge.as_ref(),
        &corps.jeton,
    )
    .await
    {
        Ok(ResultatVerification::Verifie) => HttpResponse::Ok().json(VerificationFaiteDto {
            code: "EMAIL_VERIFIED",
        }),
        // 200 et non 409 : recharger la page après une vérification réussie est
        // le cas le plus banal du parcours, pas un conflit.
        Ok(ResultatVerification::DejaVerifie) => HttpResponse::Ok().json(VerificationFaiteDto {
            code: "EMAIL_ALREADY_VERIFIED",
        }),
        Err(e) => {
            let statut = match e {
                ErreurVerification::JetonManquant => actix_web::http::StatusCode::BAD_REQUEST,
                ErreurVerification::JetonInvalide => actix_web::http::StatusCode::NOT_FOUND,
                // 410 Gone : la ressource a existé et n'existe plus. C'est
                // exactement le cas d'un jeton périmé, et FR-001 le nomme.
                ErreurVerification::JetonExpire => actix_web::http::StatusCode::GONE,
                ErreurVerification::Indisponible(_) => {
                    tracing::error!(erreur = %e, "vérification d'adresse impossible");
                    actix_web::http::StatusCode::SERVICE_UNAVAILABLE
                }
            };
            HttpResponse::build(statut).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    }
}
