//! Annulation d'une Demande par son auteur (Story 3.5, FR-014).

use actix_web::{delete, web, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use klaar_application::ports::trace_repository::TraceRepository;
use klaar_application::usecases::annuler::{annuler, ErreurAnnulation};
use klaar_application::usecases::notifier::notifier_annulation;
use klaar_matching::MotifAnnulation;

use klaar_application::usecases::langue::langue_de;

use crate::auth::Authentifie;
use crate::routes::auth::ErreurValidationDto;
use crate::EtatApplication;

#[derive(Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
pub struct ParametresAnnulation {
    /// Motif facultatif : `RESOLVED_ITSELF`, `TOO_SLOW`, `FOUND_ELSEWHERE`,
    /// `MISTAKE` ou `OTHER`.
    ///
    /// Un vocabulaire fermé et non un texte libre : ce dernier inviterait à
    /// écrire une donnée personnelle dans un champ dont la finalité annoncée
    /// est statistique.
    pub motif: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AnnulationDto {
    pub id: String,
    pub statut: String,
    pub code: &'static str,
    /// Prestataires notifiés qui ont été prévenus du retrait.
    ///
    /// Zéro n'est pas un échec : ils verront l'état réel en ouvrant
    /// l'application.
    pub prestataires_prevenus: usize,
}

fn statut(e: &ErreurAnnulation) -> actix_web::http::StatusCode {
    use actix_web::http::StatusCode;
    match e {
        ErreurAnnulation::Introuvable => StatusCode::NOT_FOUND,
        ErreurAnnulation::DejaAttribuee => StatusCode::CONFLICT,
        // 409 aussi : la Demande existe, elle est déjà dans l'état demandé.
        // Répondre 200 ferait croire à une action qui n'a pas eu lieu.
        ErreurAnnulation::DejaAnnulee => StatusCode::CONFLICT,
        ErreurAnnulation::Indisponible(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// Retire une Demande avant qu'un prestataire l'accepte.
#[utoipa::path(
    delete,
    path = "/api/v1/requests/{id}",
    tag = "demandes",
    params(
        ("id" = String, Path, description = "Identifiant de la Demande"),
        ParametresAnnulation,
    ),
    responses(
        (status = 200, description = "Demande retirée", body = AnnulationDto),
        (status = 400, description = "Identifiant ou motif illisible", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou invalide"),
        (status = 404, description = "Demande inconnue ou appartenant à quelqu'un d'autre", body = ErreurValidationDto),
        (status = 409, description = "Déjà attribuée — annuler la Mission — ou déjà annulée", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[delete("/api/v1/requests/{id}")]
pub async fn annuler_demande(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    chemin: web::Path<String>,
    parametres: web::Query<ParametresAnnulation>,
) -> HttpResponse {
    let Ok(demande_id) = Uuid::parse_str(&chemin) else {
        return HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "REQUEST_ID_INVALID".to_string(),
        });
    };

    // Un motif hors vocabulaire est refusé plutôt que ramené sur `OTHER` : le
    // ramener silencieusement ferait passer pour un choix délibéré ce qui n'est
    // qu'une faute de frappe du client, et fausserait l'analyse.
    let motif = match parametres.motif.as_deref() {
        None => None,
        Some(valeur) => match MotifAnnulation::parse(valeur) {
            Some(m) => Some(m),
            None => {
                return HttpResponse::BadRequest().json(ErreurValidationDto {
                    code: "CANCELLATION_REASON_INVALID".to_string(),
                })
            }
        },
    };

    match annuler(
        etat.demandes.as_ref(),
        etat.journal.as_ref(),
        etat.horloge.as_ref(),
        // Tiré du jeton : la Demande d'un autre est introuvable, pas interdite.
        authentifie.utilisateur_id,
        demande_id,
        motif,
    )
    .await
    {
        Ok(demande) => {
            // Prévenir les prestataires suit l'annulation mais n'en fait pas
            // partie : une panne du service de push ne doit pas empêcher
            // quelqu'un de retirer sa Demande.
            let prestataires_prevenus = prevenir_les_candidats(&etat, &demande).await;

            HttpResponse::Ok().json(AnnulationDto {
                id: demande.id.to_string(),
                statut: demande.statut.as_str().to_string(),
                code: "REQUEST_CANCELLED",
                prestataires_prevenus,
            })
        }
        Err(e) => {
            if matches!(e, ErreurAnnulation::Indisponible(_)) {
                tracing::error!(erreur = %e, "annulation impossible");
            }
            HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    }
}

/// Prévient les prestataires notifiés du retrait. Jamais bloquant.
async fn prevenir_les_candidats(
    etat: &EtatApplication,
    demande: &klaar_matching::Demande,
) -> usize {
    let Some(sender) = etat.push.as_ref() else {
        return 0;
    };
    // `Uuid::nil` n'exclut personne : à l'annulation, tous les candidats
    // retenus sont à prévenir, aucun n'a été attribué.
    let comptes = match etat
        .traces
        .comptes_retenus_sauf(demande.id, Uuid::nil())
        .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(erreur = %e, "candidats illisibles pour l'avis d'annulation");
            return 0;
        }
    };
    notifier_annulation(
        etat.abonnements.as_ref(),
        sender.as_ref(),
        demande,
        &comptes,
        langue_de(etat.utilisateurs.as_ref(), demande.demandeur_id).await,
    )
    .await
    .map(|bilan| bilan.notifies)
    .unwrap_or_else(|e| {
        tracing::error!(erreur = %e, "avis d'annulation impossible");
        0
    })
}
