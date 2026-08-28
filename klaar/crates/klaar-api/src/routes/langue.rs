//! Changer la langue de son compte (Story 9.1, FR-043).

use actix_web::{patch, web, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use klaar_application::ports::utilisateur_repository::UtilisateurRepository;
use klaar_application::usecases::langue::{interpreter, LANGUE_PAR_DEFAUT};

use crate::auth::{Authentifie, ErreurAuthDto};
use crate::routes::auth::ErreurValidationDto;
use crate::EtatApplication;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LangueDto {
    /// `fr`, `nl` ou `en`.
    pub locale: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LangueChoisieDto {
    /// La langue effectivement retenue.
    pub locale: String,
    pub code: &'static str,
    /// Vrai quand la langue demandée n'est pas parlée et qu'on est retombé sur
    /// le défaut (FR-043 `@negative`).
    pub repli: bool,
}

/// Change la langue de son compte.
///
/// **Une langue inconnue ne fait pas échouer la requête** (FR-043
/// `@negative`) : quelqu'un qui demande l'allemand doit se retrouver devant une
/// application qui marche, pas devant une erreur. Le repli est annoncé dans la
/// réponse, pour que le client puisse le dire plutôt que de laisser croire que
/// le changement a eu lieu.
#[utoipa::path(
    patch,
    path = "/api/v1/me/locale",
    tag = "compte",
    request_body = LangueDto,
    responses(
        (status = 200, description = "Langue retenue, éventuellement par repli", body = LangueChoisieDto),
        (status = 401, description = "Jeton absent ou invalide", body = ErreurAuthDto),
        (status = 404, description = "Compte introuvable", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[patch("/api/v1/me/locale")]
pub async fn changer_langue(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    corps: web::Json<LangueDto>,
) -> HttpResponse {
    let demandee = interpreter(&corps.locale);
    let retenue = demandee.unwrap_or(LANGUE_PAR_DEFAUT);

    match etat
        .utilisateurs
        .definir_locale(authentifie.utilisateur_id, retenue)
        .await
    {
        Ok(true) => HttpResponse::Ok().json(LangueChoisieDto {
            locale: retenue.as_str().to_string(),
            code: "LOCALE_SET",
            repli: demandee.is_none(),
        }),
        Ok(false) => HttpResponse::NotFound().json(ErreurValidationDto {
            code: "USER_NOT_FOUND".to_string(),
        }),
        Err(e) => {
            tracing::error!(erreur = %e, "changement de langue impossible");
            HttpResponse::ServiceUnavailable().json(ErreurValidationDto {
                code: "SERVICE_UNAVAILABLE".to_string(),
            })
        }
    }
}
