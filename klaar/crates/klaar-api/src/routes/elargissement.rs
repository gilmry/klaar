//! Élargissement du rayon d'une Demande sans réponse (Story 3.6, FR-015).

use actix_web::{post, web, HttpResponse};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use klaar_application::usecases::elargir::{elargir, ErreurElargissement};
use klaar_application::usecases::matcher::{chercher_candidats, ResultatMatching};
use klaar_application::usecases::notifier::notifier;

use crate::auth::Authentifie;
use crate::routes::auth::ErreurValidationDto;
use crate::EtatApplication;

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ElargissementDto {
    pub id: String,
    pub statut: String,
    pub code: &'static str,
    /// Rayon du nouveau tour, en mètres.
    pub rayon_metres: f64,
    /// Élargissements consommés, sur trois.
    pub elargissements: u8,
    /// Prestataires retenus par le nouveau tour.
    pub candidats: usize,
    /// Appareils réellement joints.
    pub notifies: usize,
}

fn statut(e: &ErreurElargissement) -> actix_web::http::StatusCode {
    use actix_web::http::StatusCode;
    match e {
        ErreurElargissement::Introuvable => StatusCode::NOT_FOUND,
        // 409 : la Demande existe, c'est son état qui refuse.
        ErreurElargissement::PasEnAttente | ErreurElargissement::Close => StatusCode::CONFLICT,
        // 422 et non 409 : la requête est recevable, c'est la règle des trois
        // élargissements qui l'arrête. FR-015 `@security` le nomme ainsi.
        ErreurElargissement::RayonMaximalAtteint => StatusCode::UNPROCESSABLE_ENTITY,
        ErreurElargissement::Indisponible(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// Relance une Demande sans réponse sur un rayon plus large.
#[utoipa::path(
    post,
    path = "/api/v1/requests/{id}/expand-radius",
    tag = "demandes",
    params(("id" = String, Path, description = "Identifiant de la Demande")),
    responses(
        (status = 200, description = "Demande relancée sur un rayon plus large", body = ElargissementDto),
        (status = 400, description = "Identifiant illisible", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou invalide"),
        (status = 404, description = "Demande inconnue ou appartenant à quelqu'un d'autre", body = ErreurValidationDto),
        (status = 409, description = "Demande encore diffusée, attribuée ou annulée", body = ErreurValidationDto),
        (status = 422, description = "Rayon maximal atteint ; la Demande est annulée", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[post("/api/v1/requests/{id}/expand-radius")]
pub async fn elargir_rayon(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    chemin: web::Path<String>,
) -> HttpResponse {
    let Ok(demande_id) = Uuid::parse_str(&chemin) else {
        return HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "REQUEST_ID_INVALID".to_string(),
        });
    };

    match elargir(
        etat.demandes.as_ref(),
        etat.horloge.as_ref(),
        // Tiré du jeton : la Demande d'un autre est introuvable, pas interdite.
        authentifie.utilisateur_id,
        demande_id,
    )
    .await
    {
        Ok(demande) => {
            // Même choix qu'à la soumission : le matching tourne dans la
            // requête. Différer le nouveau tour de la période d'un cadenceur
            // ferait perdre à l'élargissement l'essentiel de son intérêt, qui
            // est d'aller vite.
            let (candidats, notifies) = match chercher_candidats(
                etat.prestataires.as_ref(),
                etat.demandes.as_ref(),
                etat.traces.as_ref(),
                etat.horloge.as_ref(),
                &demande,
            )
            .await
            {
                Ok(ResultatMatching::Candidats(retenus)) => {
                    let notifies = match &etat.push {
                        Some(sender) => notifier(
                            etat.abonnements.as_ref(),
                            sender.as_ref(),
                            &demande,
                            &retenus,
                            klaar_shared_kernel::Locale::Fr,
                        )
                        .await
                        .map(|bilan| bilan.notifies)
                        .unwrap_or_else(|e| {
                            tracing::error!(erreur = %e, "notification impossible");
                            0
                        }),
                        None => 0,
                    };
                    (retenus.len(), notifies)
                }
                Ok(ResultatMatching::Aucun) => (0, 0),
                Err(e) => {
                    // Un matching en échec ne défait pas l'élargissement : le
                    // rayon est écrit, et le balayage éteindra le tour comme
                    // les autres.
                    tracing::error!(erreur = %e, "matching impossible après élargissement");
                    (0, 0)
                }
            };

            // 200 et non 201 : rien n'est créé, une Demande existante repart.
            HttpResponse::Ok().json(ElargissementDto {
                id: demande.id.to_string(),
                statut: demande.statut.as_str().to_string(),
                code: "RADIUS_EXPANDED",
                rayon_metres: demande.rayon_metres,
                elargissements: demande.elargissements,
                candidats,
                notifies,
            })
        }
        Err(e) => {
            if matches!(e, ErreurElargissement::Indisponible(_)) {
                tracing::error!(erreur = %e, "élargissement impossible");
            }
            HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    }
}
