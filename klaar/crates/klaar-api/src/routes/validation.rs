//! Validation de fin de Mission par le demandeur (Story 4.6, FR-021).

use actix_web::{post, web, HttpResponse};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use klaar_application::ports::provider_repository::ProviderRepository;
use klaar_application::usecases::notifier::notifier_liberation;
use klaar_application::usecases::valider_mission::{valider, ErreurValidation, MissionValidee};

use klaar_application::usecases::langue::langue_de;

use crate::auth::Authentifie;
use crate::routes::auth::ErreurValidationDto;
use crate::EtatApplication;

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LiberationDto {
    pub id: String,
    pub code: &'static str,
    /// `AUTHORISED`, ou `PENDING_OPS` au-delà de cinq cents euros.
    pub statut: String,
    /// Tous les montants en centimes, jamais en flottant.
    pub total_ttc_cents: i64,
    pub commission_ttc_cents: i64,
    /// Ce qui revient au prestataire.
    pub reversement_cents: i64,
    /// `USER_VALIDATION` ici ; `AUTO_RELEASE_72H` quand c'est le délai qui a
    /// tranché.
    pub origine: String,
    /// Le prestataire a-t-il été joint sur au moins un appareil.
    pub prestataire_prevenu: bool,
}

fn statut(e: &ErreurValidation) -> actix_web::http::StatusCode {
    use actix_web::http::StatusCode;
    match e {
        ErreurValidation::Introuvable => StatusCode::NOT_FOUND,
        // 409 : la Mission existe, c'est son état qui refuse.
        ErreurValidation::PasTerminee | ErreurValidation::DejaValidee => StatusCode::CONFLICT,
        // 422 : la requête est bien formée, mais il n'y a pas d'accord de prix
        // à honorer. C'est une règle métier, pas une erreur de saisie.
        ErreurValidation::Domaine(_) => StatusCode::UNPROCESSABLE_ENTITY,
        ErreurValidation::Indisponible(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// Valide la fin d'une intervention.
#[utoipa::path(
    post,
    path = "/api/v1/missions/{id}/validate",
    tag = "missions",
    params(("id" = String, Path, description = "Identifiant de la Mission")),
    responses(
        (status = 201, description = "Validation enregistrée", body = LiberationDto),
        (status = 400, description = "Identifiant illisible", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou invalide"),
        (status = 404, description = "Mission inconnue ou appartenant à quelqu'un d'autre", body = ErreurValidationDto),
        (status = 409, description = "Intervention pas terminée, ou déjà validée", body = ErreurValidationDto),
        (status = 422, description = "Aucun devis accepté à honorer", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[post("/api/v1/missions/{id}/validate")]
pub async fn valider_mission(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    chemin: web::Path<String>,
) -> HttpResponse {
    let Ok(mission_id) = Uuid::parse_str(&chemin) else {
        return HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "MISSION_ID_INVALID".to_string(),
        });
    };

    match valider(
        etat.demandes.as_ref(),
        etat.missions.as_ref(),
        etat.devis.as_ref(),
        etat.liberations.as_ref(),
        etat.horloge.as_ref(),
        // Tiré du jeton : accepter un identifiant en entrée laisserait valider
        // l'intervention d'autrui, donc décider du versement de son argent.
        authentifie.utilisateur_id,
        mission_id,
    )
    .await
    {
        Ok(validee) => {
            // Prévenir suit l'écriture : une panne du service de push ne doit
            // pas défaire une validation, et FR-021 `@happy` demande que le
            // prestataire soit averti.
            let prestataire_prevenu = prevenir_le_prestataire(&etat, &validee).await;

            HttpResponse::Created().json(LiberationDto {
                id: validee.liberation.id.to_string(),
                code: "MISSION_VALIDATED",
                statut: validee.liberation.statut.as_str().to_string(),
                total_ttc_cents: validee.liberation.repartition.total_ttc.cents(),
                commission_ttc_cents: validee.liberation.repartition.commission_ttc.cents(),
                reversement_cents: validee.liberation.repartition.reversement.cents(),
                origine: validee.liberation.origine.as_str().to_string(),
                prestataire_prevenu,
            })
        }
        Err(e) => {
            if matches!(e, ErreurValidation::Indisponible(_)) {
                tracing::error!(erreur = %e, "validation impossible");
            }
            HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    }
}

/// Prévient le prestataire que son intervention est validée. Jamais bloquant.
async fn prevenir_le_prestataire(etat: &EtatApplication, validee: &MissionValidee) -> bool {
    let Some(sender) = etat.push.as_ref() else {
        return false;
    };
    let compte = match etat.prestataires.par_id(validee.provider_id).await {
        Ok(Some(p)) => p.utilisateur_id,
        Ok(None) => return false,
        Err(e) => {
            tracing::error!(erreur = %e, "prestataire illisible pour l'avis de validation");
            return false;
        }
    };
    notifier_liberation(
        etat.abonnements.as_ref(),
        sender.as_ref(),
        compte,
        validee.liberation.statut,
        langue_de(etat.utilisateurs.as_ref(), compte).await,
    )
    .await
    .map(|bilan| bilan.notifies > 0)
    .unwrap_or_else(|e| {
        tracing::error!(erreur = %e, "avis de validation non délivré");
        false
    })
}
