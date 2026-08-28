//! Conversation entre le demandeur et le prestataire (Story 6.1, FR-030, FR-032).

use actix_web::{get, post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use klaar_application::usecases::converser::{
    bilan_tentatives, ecrire, lire, Depots, ErreurConversation,
};
use klaar_application::usecases::notifier::notifier_message;

use klaar_application::usecases::langue::langue_de;

use crate::auth::{Authentifie, ErreurAuthDto};
use crate::routes::auth::ErreurValidationDto;
use crate::EtatApplication;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MessageDto {
    /// Quatre mille caractères au plus.
    pub corps: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MessageEnvoyeDto {
    pub id: String,
    pub code: &'static str,
    /// En RFC 3339.
    pub envoye_le: String,
    /// L'autre partie a-t-elle été jointe sur au moins un appareil.
    pub destinataire_prevenu: bool,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MessageLuDto {
    pub id: String,
    /// Vrai si c'est vous qui l'avez écrit. Rendu plutôt que l'identifiant de
    /// l'auteur : le client n'a besoin que de savoir de quel côté afficher la
    /// bulle, et un identifiant de compte n'a rien à faire dans un fil.
    pub de_moi: bool,
    pub corps: String,
    pub envoye_le: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FilDto {
    pub messages: Vec<MessageLuDto>,
}

/// Corps rendu quand des coordonnées sont refusées (FR-032).
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RefusCoordonneesDto {
    pub code: &'static str,
    /// Tentatives des trente derniers jours, celle-ci comprise.
    pub tentatives: i64,
    /// Vrai au-delà du seuil : le compte est signalé à l'exploitation.
    pub signale: bool,
}

fn statut(e: &ErreurConversation) -> actix_web::http::StatusCode {
    use actix_web::http::StatusCode;
    match e {
        ErreurConversation::Introuvable => StatusCode::NOT_FOUND,
        ErreurConversation::Domaine(d) => match d.code() {
            // 410 : la conversation a existé et s'est fermée. FR-030
            // `@negative` le demande.
            "CONVERSATION_CLOSED" => StatusCode::GONE,
            // 403 : la requête est comprise, et refusée pour ce qu'elle
            // contient. Ni 400 — elle est bien formée — ni 422 — la valeur
            // n'est pas invalide, elle est interdite.
            "CONTACT_INFO_FORBIDDEN" => StatusCode::FORBIDDEN,
            // 422 : trop long, vide, ou conversation pleine.
            _ => StatusCode::UNPROCESSABLE_ENTITY,
        },
        ErreurConversation::Indisponible(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// Écrit un message dans la conversation d'une intervention.
#[utoipa::path(
    post,
    path = "/api/v1/missions/{id}/messages",
    tag = "conversation",
    params(("id" = String, Path, description = "Identifiant de la Mission")),
    request_body = MessageDto,
    responses(
        (status = 201, description = "Message envoyé", body = MessageEnvoyeDto),
        (status = 400, description = "Identifiant illisible", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou invalide", body = ErreurAuthDto),
        (status = 403, description = "Coordonnées interdites", body = RefusCoordonneesDto),
        (status = 404, description = "Mission inconnue ou qui ne vous concerne pas", body = ErreurValidationDto),
        (status = 410, description = "Conversation fermée", body = ErreurValidationDto),
        (status = 422, description = "Message vide, trop long, ou conversation pleine", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[post("/api/v1/missions/{id}/messages")]
pub async fn envoyer_message(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    chemin: web::Path<String>,
    corps: web::Json<MessageDto>,
) -> HttpResponse {
    let Ok(mission_id) = Uuid::parse_str(&chemin) else {
        return HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "MISSION_ID_INVALID".to_string(),
        });
    };

    match ecrire(
        Depots {
            demandes: etat.demandes.as_ref(),
            missions: etat.missions.as_ref(),
            prestataires: etat.prestataires.as_ref(),
            messages: etat.messages.as_ref(),
            horloge: etat.horloge.as_ref(),
        },
        authentifie.utilisateur_id,
        mission_id,
        &corps.corps,
    )
    .await
    {
        Ok(envoye) => {
            let destinataire_prevenu = prevenir(&etat, envoye.destinataire).await;
            HttpResponse::Created().json(MessageEnvoyeDto {
                id: envoye.message.id.to_string(),
                code: "MESSAGE_SENT",
                envoye_le: envoye.message.envoye_le.to_rfc3339(),
                destinataire_prevenu,
            })
        }
        Err(e) if e.code() == "CONTACT_INFO_FORBIDDEN" => {
            // Le compteur est rendu à l'appelant : lui dire qu'il en est à sa
            // troisième tentative vaut mieux que de le lui apprendre le jour de
            // la sanction.
            let bilan = bilan_tentatives(
                etat.messages.as_ref(),
                etat.horloge.as_ref(),
                authentifie.utilisateur_id,
            )
            .await
            .unwrap_or(
                klaar_application::usecases::converser::RefusPourCoordonnees {
                    tentatives: 0,
                    a_signaler: false,
                },
            );
            if bilan.a_signaler {
                tracing::warn!(
                    tentatives = bilan.tentatives,
                    "tentatives répétées d'échange de coordonnées"
                );
            }
            HttpResponse::Forbidden().json(RefusCoordonneesDto {
                code: "CONTACT_INFO_FORBIDDEN",
                tentatives: bilan.tentatives,
                signale: bilan.a_signaler,
            })
        }
        Err(e) => {
            if matches!(e, ErreurConversation::Indisponible(_)) {
                tracing::error!(erreur = %e, "envoi de message impossible");
            }
            HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    }
}

/// Lit la conversation d'une intervention.
#[utoipa::path(
    get,
    path = "/api/v1/missions/{id}/messages",
    tag = "conversation",
    params(("id" = String, Path, description = "Identifiant de la Mission")),
    responses(
        (status = 200, description = "Le fil, du plus ancien au plus récent", body = FilDto),
        (status = 400, description = "Identifiant illisible", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou invalide", body = ErreurAuthDto),
        (status = 404, description = "Mission inconnue ou qui ne vous concerne pas", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[get("/api/v1/missions/{id}/messages")]
pub async fn lire_conversation(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    chemin: web::Path<String>,
) -> HttpResponse {
    let Ok(mission_id) = Uuid::parse_str(&chemin) else {
        return HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "MISSION_ID_INVALID".to_string(),
        });
    };

    match lire(
        etat.demandes.as_ref(),
        etat.missions.as_ref(),
        etat.prestataires.as_ref(),
        etat.messages.as_ref(),
        authentifie.utilisateur_id,
        mission_id,
    )
    .await
    {
        Ok(fil) => HttpResponse::Ok().json(FilDto {
            messages: fil
                .into_iter()
                .map(|m| MessageLuDto {
                    id: m.id.to_string(),
                    de_moi: m.auteur_id == authentifie.utilisateur_id,
                    corps: m.corps,
                    envoye_le: m.envoye_le.to_rfc3339(),
                })
                .collect(),
        }),
        Err(e) => {
            if matches!(e, ErreurConversation::Indisponible(_)) {
                tracing::error!(erreur = %e, "lecture de la conversation impossible");
            }
            HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    }
}

/// Prévient le destinataire. Jamais bloquant.
async fn prevenir(etat: &EtatApplication, destinataire: Uuid) -> bool {
    let Some(sender) = etat.push.as_ref() else {
        return false;
    };
    notifier_message(
        etat.abonnements.as_ref(),
        sender.as_ref(),
        destinataire,
        langue_de(etat.utilisateurs.as_ref(), destinataire).await,
    )
    .await
    .map(|bilan| bilan.notifies > 0)
    .unwrap_or_else(|e| {
        tracing::error!(erreur = %e, "avis de message non délivré");
        false
    })
}
