//! Acceptation d'une Demande par un prestataire (Story 3.4, FR-013).

use actix_web::http::header::RETRY_AFTER;
use actix_web::{post, web, HttpResponse};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use klaar_application::ports::demande_repository::DemandeRepository;
use klaar_application::ports::horloge::Horloge;
use klaar_application::ports::trace_repository::TraceRepository;
use klaar_application::usecases::accepter::{accepter, ErreurAcceptation};
use klaar_application::usecases::notifier::notifier_match_pris;

use klaar_application::usecases::langue::langue_de;

use crate::auth::{Authentifie, ErreurAuthDto};
use crate::limitation::{Quota, Verdict};
use crate::routes::auth::ErreurValidationDto;
use crate::EtatApplication;

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MissionDto {
    /// Identifiant de la Mission créée.
    pub id: String,
    pub demande_id: String,
    pub statut: String,
    pub code: &'static str,
    /// Autres candidats prévenus que la Demande est prise (FR-013 `@happy`).
    ///
    /// Zéro n'est pas un échec : les autres verront l'état réel en ouvrant
    /// l'application. Le compter à part évite de faire croire que quatre
    /// personnes ont été informées quand aucune ne l'a été.
    pub autres_prevenus: usize,
}

fn statut(e: &ErreurAcceptation) -> actix_web::http::StatusCode {
    use actix_web::http::StatusCode;
    match e {
        ErreurAcceptation::NonEligible => StatusCode::FORBIDDEN,
        ErreurAcceptation::Introuvable => StatusCode::NOT_FOUND,
        // 409 : la Demande existe, c'est son état qui refuse. Le prestataire
        // n'a rien à corriger et rien à réessayer.
        ErreurAcceptation::DejaAttribuee | ErreurAcceptation::Occupe => StatusCode::CONFLICT,
        // 410 et non 404 : la Demande a existé, et le dire évite de faire
        // chercher une erreur de saisie là où il n'y a qu'un retard. Une
        // Demande retirée par son auteur relève du même 410 (FR-014 `@edge`) :
        // c'est fini, et ce n'est pas « quelqu'un d'autre l'a ».
        ErreurAcceptation::Expiree | ErreurAcceptation::Annulee => StatusCode::GONE,
        ErreurAcceptation::Indisponible(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// Accepte une Demande diffusée. Le premier arrivé l'obtient.
#[utoipa::path(
    post,
    path = "/api/v1/requests/{id}/accept",
    tag = "demandes",
    params(("id" = Uuid, Path, description = "Identifiant de la Demande")),
    responses(
        (status = 201, description = "Mission attribuée", body = MissionDto),
        (status = 400, description = "Identifiant illisible", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou invalide", body = ErreurAuthDto),
        (status = 403, description = "Prestataire non éligible", body = ErreurValidationDto),
        (status = 404, description = "Demande introuvable", body = ErreurValidationDto),
        (status = 409, description = "Déjà attribuée, ou prestataire déjà en Mission", body = ErreurValidationDto),
        (status = 410, description = "Tour écoulé, Demande sans réponse, ou retirée par son auteur", body = ErreurValidationDto),
        (status = 429, description = "Trop d'acceptations", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[post("/api/v1/requests/{id}/accept")]
pub async fn accepter_demande(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    chemin: web::Path<String>,
) -> HttpResponse {
    let Ok(demande_id) = Uuid::parse_str(&chemin) else {
        return HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "REQUEST_ID_INVALID".to_string(),
        });
    };

    // Le quota est compté par **compte**, pas par adresse : FR-013 le formule
    // ainsi, et c'est la bonne unité — une flotte derrière une seule sortie
    // NAT ne doit pas s'épuiser mutuellement, et changer d'adresse ne doit pas
    // remettre le compteur à zéro.
    let maintenant = etat.horloge.maintenant();
    let source = format!("accept:{}", authentifie.utilisateur_id);
    if let Verdict::Refuse { retry_after } =
        etat.limiteur
            .verifier_quota(&source, maintenant, Quota::acceptation())
    {
        return HttpResponse::TooManyRequests()
            .insert_header((RETRY_AFTER, retry_after.to_string()))
            .json(ErreurValidationDto {
                code: "RATE_LIMIT_EXCEEDED".to_string(),
            });
    }

    match accepter(
        etat.prestataires.as_ref(),
        etat.demandes.as_ref(),
        etat.missions.as_ref(),
        etat.horloge.as_ref(),
        // Tiré du jeton, jamais du chemin : accepter un identifiant de
        // prestataire en entrée laisserait accepter au nom d'autrui.
        authentifie.utilisateur_id,
        demande_id,
    )
    .await
    {
        Ok(attribution) => {
            // Prévenir les autres suit l'attribution mais n'en fait pas
            // partie : une panne du service de push ne doit pas défaire une
            // Mission déjà attribuée, ni faire échouer la réponse de celui qui
            // a gagné.
            let autres_prevenus = prevenir_les_autres(&etat, &attribution).await;

            HttpResponse::Created().json(MissionDto {
                id: attribution.mission.id.to_string(),
                demande_id: attribution.mission.demande_id.to_string(),
                statut: attribution.mission.statut.as_str().to_string(),
                code: "MATCH_ACCEPTED",
                autres_prevenus,
            })
        }
        Err(e) => {
            if matches!(e, ErreurAcceptation::Indisponible(_)) {
                tracing::error!(erreur = %e, "acceptation impossible");
            }
            HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    }
}

/// Envoie `MATCH_TAKEN` aux autres candidats retenus. Jamais bloquant.
async fn prevenir_les_autres(
    etat: &EtatApplication,
    attribution: &klaar_application::usecases::accepter::Attribution,
) -> usize {
    let Some(sender) = etat.push.as_ref() else {
        // Sans clé VAPID configurée, le service tourne sans notifications :
        // c'est un mode de fonctionnement légitime, pas une panne.
        return 0;
    };
    let demande = match etat.demandes.par_id(attribution.mission.demande_id).await {
        Ok(Some(d)) => d,
        Ok(None) => return 0,
        Err(e) => {
            tracing::error!(erreur = %e, "Demande illisible pour l'avis de prise");
            return 0;
        }
    };
    let comptes = match etat
        .traces
        .comptes_retenus_sauf(demande.id, attribution.provider_id)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(erreur = %e, "candidats illisibles pour l'avis de prise");
            return 0;
        }
    };
    notifier_match_pris(
        etat.abonnements.as_ref(),
        sender.as_ref(),
        &demande,
        &comptes,
        langue_de(etat.utilisateurs.as_ref(), demande.demandeur_id).await,
    )
    .await
    .map(|bilan| bilan.notifies)
    .unwrap_or_else(|e| {
        tracing::error!(erreur = %e, "avis de prise impossible");
        0
    })
}
