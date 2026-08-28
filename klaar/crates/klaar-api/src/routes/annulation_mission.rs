//! Annulation d'une Mission en cours (Story 4.7, FR-022).
//!
//! Une seule route pour les deux parties : qui appelle est déduit du jeton, et
//! c'est cela qui détermine ce que l'annulation coûte. Deux routes distinctes
//! auraient demandé au client de savoir qui il est, ce que le serveur sait déjà.

use actix_web::{post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use klaar_application::ports::provider_repository::ProviderRepository;
use klaar_application::usecases::annuler_mission::{
    annuler_mission as annuler, Depots, ErreurAnnulationMission, MissionAnnulee,
};
use klaar_application::usecases::notifier::notifier_annulation_mission;

use klaar_application::usecases::langue::langue_de;

use crate::auth::{Authentifie, ErreurAuthDto};
use crate::routes::auth::ErreurValidationDto;
use crate::EtatApplication;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AnnulationMissionDto {
    /// `NO_LONGER_NEEDED`, `UNAVAILABLE`, `DISAGREEMENT`, `NO_ACCESS` ou
    /// `OTHER`. Facultatif.
    pub motif: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MissionAnnuleeDto {
    pub id: String,
    pub code: &'static str,
    /// `CANCELLED_USER` ou `CANCELLED_PROVIDER`.
    pub auteur: String,
    /// Forfait de déplacement retenu, en centimes. Zéro sauf si le prestataire
    /// était déjà sur place.
    pub forfait_deplacement_cents: i64,
    /// Ce qui revient au demandeur, en centimes.
    pub remboursement_cents: i64,
    /// Vrai si ce désistement a suspendu le prestataire (FR-022 `@edge`).
    pub prestataire_suspendu: bool,
    /// L'autre partie a-t-elle été jointe sur au moins un appareil.
    pub autre_partie_prevenue: bool,
}

fn statut(e: &ErreurAnnulationMission) -> actix_web::http::StatusCode {
    use actix_web::http::StatusCode;
    match e {
        ErreurAnnulationMission::Introuvable => StatusCode::NOT_FOUND,
        ErreurAnnulationMission::MotifInconnu => StatusCode::BAD_REQUEST,
        // 409 : la Mission existe, c'est son état qui refuse. FR-022
        // `@negative` demande ce code pour une intervention déjà faite, et
        // renvoie vers le litige.
        ErreurAnnulationMission::Domaine(_) | ErreurAnnulationMission::Concurrence => {
            StatusCode::CONFLICT
        }
        ErreurAnnulationMission::Indisponible(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// Annule une intervention en cours.
#[utoipa::path(
    post,
    path = "/api/v1/missions/{id}/cancel",
    tag = "missions",
    params(("id" = String, Path, description = "Identifiant de la Mission")),
    request_body = AnnulationMissionDto,
    responses(
        (status = 200, description = "Intervention annulée", body = MissionAnnuleeDto),
        (status = 400, description = "Identifiant illisible ou motif hors vocabulaire", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou invalide", body = ErreurAuthDto),
        (status = 404, description = "Mission inconnue, ou qui ne vous concerne pas", body = ErreurValidationDto),
        (status = 409, description = "Intervention déjà faite ou déjà annulée", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[post("/api/v1/missions/{id}/cancel")]
pub async fn annuler_intervention(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    chemin: web::Path<String>,
    corps: Option<web::Json<AnnulationMissionDto>>,
) -> HttpResponse {
    let Ok(mission_id) = Uuid::parse_str(&chemin) else {
        return HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "MISSION_ID_INVALID".to_string(),
        });
    };

    match annuler(
        Depots {
            demandes: etat.demandes.as_ref(),
            missions: etat.missions.as_ref(),
            devis: etat.devis.as_ref(),
            annulations: etat.annulations.as_ref(),
            prestataires: etat.prestataires.as_ref(),
            horloge: etat.horloge.as_ref(),
        },
        // Tiré du jeton : c'est lui qui dit si l'appelant est le demandeur ou
        // le prestataire, donc ce que l'annulation coûte.
        authentifie.utilisateur_id,
        mission_id,
        corps.as_ref().and_then(|c| c.motif.as_deref()),
    )
    .await
    {
        Ok(annulee) => {
            let autre_partie_prevenue = prevenir_l_autre_partie(&etat, &annulee).await;

            HttpResponse::Ok().json(MissionAnnuleeDto {
                id: annulee.annulation.id.to_string(),
                code: "MISSION_CANCELLED",
                auteur: annulee.annulation.auteur.as_str().to_string(),
                forfait_deplacement_cents: annulee
                    .annulation
                    .consequence
                    .forfait_deplacement
                    .cents(),
                remboursement_cents: annulee.annulation.consequence.remboursement.cents(),
                prestataire_suspendu: annulee.prestataire_suspendu,
                autre_partie_prevenue,
            })
        }
        Err(e) => {
            if matches!(e, ErreurAnnulationMission::Indisponible(_)) {
                tracing::error!(erreur = %e, "annulation impossible");
            }
            HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    }
}

/// Prévient celle des deux parties qui n'a pas annulé. Jamais bloquant.
async fn prevenir_l_autre_partie(etat: &EtatApplication, annulee: &MissionAnnulee) -> bool {
    let Some(sender) = etat.push.as_ref() else {
        return false;
    };
    // Le prestataire est identifié par sa fiche ; le compte à joindre est celui
    // de l'utilisateur derrière. Le demandeur, lui, **est** un compte.
    let compte = match annulee.annulation.auteur {
        klaar_intervention::AuteurAnnulation::Demandeur => {
            match etat.prestataires.par_id(annulee.a_prevenir).await {
                Ok(Some(p)) => p.utilisateur_id,
                _ => return false,
            }
        }
        klaar_intervention::AuteurAnnulation::Prestataire => annulee.a_prevenir,
    };

    notifier_annulation_mission(
        etat.abonnements.as_ref(),
        sender.as_ref(),
        compte,
        annulee.annulation.auteur,
        langue_de(etat.utilisateurs.as_ref(), compte).await,
    )
    .await
    .map(|bilan| bilan.notifies > 0)
    .unwrap_or_else(|e| {
        tracing::error!(erreur = %e, "avis d'annulation non délivré");
        false
    })
}
