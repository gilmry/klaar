//! Flux temps réel d'une Mission (Story 4.9).
//!
//! **Deux requêtes, et c'est délibéré.** Le client demande d'abord un billet
//! par une requête authentifiée normale, puis ouvre la socket avec ce billet.
//! Un navigateur ne peut pas poser d'en-tête `Authorization` sur une WebSocket,
//! et l'URL d'une socket finit dans les journaux du serveur, du proxy et du
//! navigateur : y mettre un jeton valable une heure reviendrait à l'y publier.
//! Le billet, lui, vaut trente secondes et une seule fois (voir `crate::billet`).
//!
//! **Les droits sont vérifiés à l'ouverture, pas à chaque message.** Une Mission
//! ne change pas de propriétaire : celui qui pouvait la voir au moment de la
//! poignée de main peut la voir pendant toute la vie de la socket. Revérifier à
//! chaque événement coûterait deux requêtes par message pour une garantie que
//! rien ne peut invalider.
//!
//! **La socket ne transporte aucun détail.** Elle dit qu'il s'est passé quelque
//! chose ; le client relit ce qu'il a le droit de voir par les routes qui
//! vérifient déjà ses droits. C'est ce qui permet à un même événement d'être
//! diffusé au demandeur et au prestataire, qui ne voient pas la même chose.

use std::time::Duration;

use actix_web::{get, post, web, HttpRequest, HttpResponse};
use futures_util::StreamExt as _;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;
use utoipa::ToSchema;
use uuid::Uuid;

use klaar_application::ports::demande_repository::DemandeRepository;
use klaar_application::ports::horloge::Horloge;
use klaar_application::ports::mission_repository::MissionRepository;
use klaar_application::ports::provider_repository::ProviderRepository;

use crate::auth::{Authentifie, ErreurAuthDto};
use crate::billet::VALIDITE_SECONDES;
use crate::routes::auth::ErreurValidationDto;
use crate::EtatApplication;

/// Cadence des battements de cœur.
///
/// Une socket coupée par un pare-feu ou un proxy ne se signale pas : elle
/// cesse simplement de livrer. Sans battement, le client croirait le service
/// silencieux et n'aurait aucune raison de se reconnecter. Vingt secondes
/// passent sous les délais d'inactivité habituels des proxys, qui commencent
/// vers soixante.
const BATTEMENT_SECONDES: u64 = 20;

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BilletDto {
    /// À passer en paramètre `billet` de l'URL de la socket.
    pub billet: String,
    /// Secondes de validité restantes.
    pub expire_dans: i64,
}

#[derive(Deserialize)]
pub struct ParametresFlux {
    billet: Option<String>,
}

/// Demande un billet d'ouverture de socket.
#[utoipa::path(
    post,
    path = "/api/v1/realtime/ticket",
    tag = "temps-réel",
    responses(
        (status = 201, description = "Billet à usage unique", body = BilletDto),
        (status = 401, description = "Jeton absent ou invalide", body = ErreurAuthDto),
        (status = 503, description = "Trop de billets vivants", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[post("/api/v1/realtime/ticket")]
pub async fn demander_billet(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
) -> HttpResponse {
    match etat
        .billets
        .emettre(authentifie.utilisateur_id, etat.horloge.maintenant())
    {
        Some(billet) => HttpResponse::Created().json(BilletDto {
            billet,
            expire_dans: VALIDITE_SECONDES,
        }),
        // La table est pleine : refuser vaut mieux que laisser la mémoire du
        // service suivre le rythme de celui qui insiste. Le client garde son
        // sondage, donc il ne perd que la vitesse.
        None => HttpResponse::ServiceUnavailable().json(ErreurValidationDto {
            code: "TICKET_UNAVAILABLE".to_string(),
        }),
    }
}

/// Dit si ce compte a le droit de suivre cette Mission.
///
/// Deux publics, une seule Mission : le demandeur de la Demande dont elle est
/// née, et le prestataire à qui elle est attribuée. Toute autre réponse est un
/// refus indistinct — la même précédence anti-énumération qu'ailleurs.
async fn peut_suivre(
    etat: &EtatApplication,
    utilisateur_id: Uuid,
    mission_id: Uuid,
) -> Result<bool, ()> {
    let Ok(Some(mission)) = etat.missions.par_id(mission_id).await else {
        return Ok(false);
    };

    // Le demandeur d'abord : c'est le cas le plus fréquent, et il évite de
    // charger la fiche prestataire d'un compte qui n'en a pas.
    if let Ok(Some(demande)) = etat.demandes.par_id(mission.demande_id).await {
        if demande.demandeur_id == utilisateur_id {
            return Ok(true);
        }
    }

    match etat.prestataires.par_utilisateur_id(utilisateur_id).await {
        Ok(Some(provider)) => Ok(mission.appartient_a(provider.id)),
        Ok(None) => Ok(false),
        Err(_) => Err(()),
    }
}

/// Ouvre le flux d'événements d'une Mission.
#[utoipa::path(
    get,
    path = "/api/v1/missions/{id}/events",
    tag = "temps-réel",
    params(
        ("id" = Uuid, Path, description = "Identifiant de la Mission"),
        ("billet" = String, Query, description = "Billet à usage unique", min_length = 1),
    ),
    responses(
        (status = 101, description = "Socket ouverte"),
        (status = 400, description = "Identifiant illisible ou billet absent", body = ErreurValidationDto),
        (status = 401, description = "Billet inconnu, périmé ou déjà utilisé", body = ErreurValidationDto),
        (status = 404, description = "Mission inconnue ou hors de portée", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    )
)]
#[get("/api/v1/missions/{id}/events")]
pub async fn suivre_en_direct(
    requete: HttpRequest,
    flux: web::Payload,
    etat: web::Data<EtatApplication>,
    chemin: web::Path<String>,
    parametres: web::Query<ParametresFlux>,
) -> Result<HttpResponse, actix_web::Error> {
    let Ok(mission_id) = Uuid::parse_str(&chemin) else {
        return Ok(HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "MISSION_ID_INVALID".to_string(),
        }));
    };

    let Some(billet) = parametres.billet.as_deref().filter(|b| !b.is_empty()) else {
        return Ok(HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "TICKET_MISSING".to_string(),
        }));
    };

    // Consommé avant tout le reste : un billet présenté est un billet dépensé,
    // qu'il donne accès ou non. Le garder en cas de refus laisserait essayer
    // des identifiants de Mission avec le même.
    let Some(utilisateur_id) = etat.billets.consommer(billet, etat.horloge.maintenant()) else {
        return Ok(HttpResponse::Unauthorized().json(ErreurValidationDto {
            code: "TICKET_INVALID".to_string(),
        }));
    };

    match peut_suivre(&etat, utilisateur_id, mission_id).await {
        // 404 et non 403 : un 403 apprendrait que cet identifiant est celui
        // d'une Mission qui existe.
        Ok(false) => {
            return Ok(HttpResponse::NotFound().json(ErreurValidationDto {
                code: "MISSION_NOT_FOUND".to_string(),
            }))
        }
        Err(()) => {
            return Ok(
                HttpResponse::ServiceUnavailable().json(ErreurValidationDto {
                    code: "SERVICE_UNAVAILABLE".to_string(),
                }),
            )
        }
        Ok(true) => {}
    }

    let (reponse, session, mut entrant) = actix_ws::handle(&requete, flux)?;
    // L'abonnement est pris **avant** que la poignée de main ne soit rendue :
    // s'abonner après laisserait passer les événements survenus entre les deux,
    // et c'est précisément la fenêtre où quelque chose bouge — le client vient
    // d'agir.
    let mut evenements = etat.evenements.abonner();

    actix_web::rt::spawn(async move {
        let mut session = session;
        let mut battement = tokio::time::interval(Duration::from_secs(BATTEMENT_SECONDES));

        loop {
            tokio::select! {
                message = entrant.next() => match message {
                    // Le client n'a rien à dire sur ce flux ; ce qu'il envoie
                    // est ignoré, sauf la fermeture et le ping du protocole.
                    Some(Ok(actix_ws::Message::Ping(charge))) => {
                        if session.pong(&charge).await.is_err() { break; }
                    }
                    Some(Ok(actix_ws::Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                },

                recu = evenements.recv() => match recu {
                    Ok(evenement) => {
                        // Le filtre est ici : le bus diffuse toutes les
                        // Missions, cette socket n'en a vérifié qu'une.
                        if evenement.mission_id != mission_id { continue; }
                        if session.text(evenement.en_json()).await.is_err() { break; }
                    }
                    // Distancé : des événements ont été perdus. Le dire est la
                    // seule réponse honnête — le client relira l'état complet
                    // par HTTP plutôt que d'afficher une suite trouée.
                    Err(RecvError::Lagged(_)) => {
                        if session.text(r#"{"genre":"RESYNC"}"#).await.is_err() { break; }
                    }
                    Err(RecvError::Closed) => break,
                },

                _ = battement.tick() => {
                    // Une socket coupée par un proxy ne se signale pas : sans
                    // ce ping, le client croirait le service silencieux.
                    if session.ping(b"").await.is_err() { break; }
                }
            }
        }

        let _ = session.close(None).await;
    });

    Ok(reponse)
}
