//! Cycle de vie d'une Mission (Story 4.3, FR-018).

use actix_web::{patch, web, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use klaar_application::ports::demande_repository::DemandeRepository;
use klaar_application::usecases::notifier::notifier_avancement;
use klaar_application::usecases::transiter_mission::{
    transiter, Avancement, Declaration, ErreurTransition,
};
use klaar_shared_kernel::Geo;

use crate::auth::Authentifie;
use crate::routes::auth::ErreurValidationDto;
use crate::EtatApplication;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TransitionDto {
    /// `PROVIDER_EN_ROUTE`, `ON_SITE` ou `COMPLETED`.
    pub statut: String,
    /// Instant déclaré par le client, en RFC 3339.
    ///
    /// Facultatif. Sert à la synchronisation d'une transition faite hors
    /// connexion : sans lui, l'événement serait daté du moment où le réseau est
    /// revenu. Refusé au-delà de cinq minutes d'écart avec l'heure du serveur.
    pub horodate_le: Option<String>,
    /// Position au moment de la transition.
    ///
    /// **Facultative.** L'exiger rendrait l'autorisation de géolocalisation de
    /// fait obligatoire, alors que quelqu'un sans GPS doit pouvoir déclarer
    /// qu'il est arrivé.
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MissionAvanceeDto {
    pub id: String,
    pub statut: String,
    pub code: &'static str,
    /// Instant retenu pour l'événement, en RFC 3339.
    ///
    /// Celui du client quand il en a fourni un dans la tolérance, celui du
    /// serveur sinon. Le rendre évite au client de deviner lequel a été gardé.
    pub horodate_le: String,
    /// Vrai si la position déclarée sort de la Région (FR-018 `@edge`).
    ///
    /// N'empêche pas la transition : un prestataire qui coupe par le ring
    /// reste en intervention.
    pub hors_zone: bool,
    /// Le demandeur a-t-il été joint sur au moins un appareil.
    pub demandeur_prevenu: bool,
}

fn statut(e: &ErreurTransition) -> actix_web::http::StatusCode {
    use actix_web::http::StatusCode;
    match e {
        ErreurTransition::PasPrestataire => StatusCode::FORBIDDEN,
        // 403 et non 404 : le prestataire est bien chez lui, c'est ce geste
        // précis qui ne lui appartient pas. Rien n'est révélé par ce refus,
        // puisqu'il ne dépend d'aucune donnée.
        ErreurTransition::ReserveAuDemandeur => StatusCode::FORBIDDEN,
        ErreurTransition::Introuvable => StatusCode::NOT_FOUND,
        ErreurTransition::StatutInconnu => StatusCode::BAD_REQUEST,
        ErreurTransition::Domaine(d) => match d.code() {
            // 409 : la Mission existe, c'est son état qui refuse.
            "INVALID_TRANSITION" => StatusCode::CONFLICT,
            // 400 : la requête elle-même est mauvaise.
            _ => StatusCode::BAD_REQUEST,
        },
        ErreurTransition::Concurrence => StatusCode::CONFLICT,
        ErreurTransition::Indisponible(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// Fait avancer une Mission.
#[utoipa::path(
    patch,
    path = "/api/v1/missions/{id}/status",
    tag = "missions",
    params(("id" = String, Path, description = "Identifiant de la Mission")),
    request_body = TransitionDto,
    responses(
        (status = 200, description = "Mission avancée", body = MissionAvanceeDto),
        (status = 400, description = "Statut inconnu, position ou horodatage invalide", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou invalide"),
        (status = 403, description = "Ce compte n'est pas un prestataire", body = ErreurValidationDto),
        (status = 404, description = "Mission inconnue ou attribuée à quelqu'un d'autre", body = ErreurValidationDto),
        (status = 409, description = "Transition interdite depuis l'état courant", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[patch("/api/v1/missions/{id}/status")]
pub async fn avancer_mission(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    chemin: web::Path<String>,
    corps: web::Json<TransitionDto>,
) -> HttpResponse {
    let Ok(mission_id) = Uuid::parse_str(&chemin) else {
        return HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "MISSION_ID_INVALID".to_string(),
        });
    };

    let horodate_le = match corps.horodate_le.as_deref() {
        None => None,
        Some(brut) => match chrono::DateTime::parse_from_rfc3339(brut) {
            Ok(d) => Some(d.with_timezone(&chrono::Utc)),
            Err(_) => {
                return HttpResponse::BadRequest().json(ErreurValidationDto {
                    code: "TIMESTAMP_MALFORMED".to_string(),
                })
            }
        },
    };

    // Latitude et longitude vont ensemble : n'en donner qu'une est une erreur
    // de client, et la traiter comme « pas de position » masquerait un bogue
    // dont personne ne verrait la trace.
    let position = match (corps.latitude, corps.longitude) {
        (None, None) => None,
        (Some(lat), Some(lon)) => match Geo::new(lat, lon) {
            Ok(g) => Some(g),
            Err(_) => {
                return HttpResponse::BadRequest().json(ErreurValidationDto {
                    code: "POSITION_INVALID".to_string(),
                })
            }
        },
        _ => {
            return HttpResponse::BadRequest().json(ErreurValidationDto {
                code: "POSITION_INCOMPLETE".to_string(),
            })
        }
    };

    match transiter(
        etat.prestataires.as_ref(),
        etat.missions.as_ref(),
        etat.horloge.as_ref(),
        // Tiré du jeton : accepter un identifiant de prestataire en entrée
        // laisserait déclarer « je suis sur place » au nom d'un autre.
        authentifie.utilisateur_id,
        mission_id,
        Declaration {
            statut_cible: &corps.statut,
            horodate_le,
            position,
        },
    )
    .await
    {
        Ok(avancement) => {
            // Prévenir le demandeur suit la transition mais n'en fait pas
            // partie : une panne du service de push ne doit pas empêcher un
            // prestataire de déclarer qu'il est arrivé.
            let demandeur_prevenu = prevenir_le_demandeur(&etat, &avancement).await;

            HttpResponse::Ok().json(MissionAvanceeDto {
                id: avancement.mission.id.to_string(),
                statut: avancement.mission.statut.as_str().to_string(),
                code: "MISSION_STATUS_CHANGED",
                horodate_le: avancement.entree.horodate_le.to_rfc3339(),
                hors_zone: avancement.entree.hors_zone,
                demandeur_prevenu,
            })
        }
        Err(e) => {
            if matches!(e, ErreurTransition::Indisponible(_)) {
                tracing::error!(erreur = %e, "transition impossible");
            }
            HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    }
}

/// Envoie l'avis d'avancement au demandeur. Jamais bloquant.
async fn prevenir_le_demandeur(etat: &EtatApplication, avancement: &Avancement) -> bool {
    let Some(sender) = etat.push.as_ref() else {
        return false;
    };
    let demande = match etat.demandes.par_id(avancement.mission.demande_id).await {
        Ok(Some(d)) => d,
        Ok(None) => return false,
        Err(e) => {
            tracing::error!(erreur = %e, "Demande illisible pour l'avis d'avancement");
            return false;
        }
    };
    notifier_avancement(
        etat.abonnements.as_ref(),
        sender.as_ref(),
        &demande,
        avancement.mission.statut,
        klaar_shared_kernel::Locale::Fr,
    )
    .await
    .map(|bilan| bilan.notifies > 0)
    .unwrap_or_else(|e| {
        tracing::error!(erreur = %e, "avis d'avancement impossible");
        false
    })
}
