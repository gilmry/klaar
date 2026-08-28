//! Notation double sens après intervention (Story 7.1, FR-033).

use actix_web::{get, post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use klaar_application::ports::horloge::Horloge;
use klaar_application::usecases::noter::{noter, notes_visibles, Avis, Depots, ErreurNotation};

use crate::auth::{Authentifie, ErreurAuthDto};
use crate::routes::auth::ErreurValidationDto;
use crate::EtatApplication;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NoteDto {
    /// De 1 à 5 étoiles. Zéro n'existe pas : une intervention faite n'est pas
    /// rien, et l'échec total relève du litige.
    pub note: u8,
    pub commentaire: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NoteEcriteDto {
    pub id: String,
    pub code: &'static str,
    /// `PROVIDER` ou `USER` : qui a été noté. Déduit de l'appelant.
    pub cible: String,
    /// Vrai si les deux notes sont désormais visibles.
    ///
    /// Faux tant que l'autre partie n'a pas noté : publier la première
    /// laisserait l'autre ajuster la sienne.
    pub publiee: bool,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NoteVisibleDto {
    pub cible: String,
    pub note: u8,
    pub commentaire: Option<String>,
    /// En RFC 3339.
    pub cree_le: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NotesDeMissionDto {
    /// Vide tant que l'anti-représailles retient les deux notes.
    pub notes: Vec<NoteVisibleDto>,
}

fn statut(e: &ErreurNotation) -> actix_web::http::StatusCode {
    use actix_web::http::StatusCode;
    match e {
        ErreurNotation::Introuvable => StatusCode::NOT_FOUND,
        ErreurNotation::PasValidee => StatusCode::CONFLICT,
        ErreurNotation::DejaNotee => StatusCode::CONFLICT,
        ErreurNotation::Domaine(d) => match d.code() {
            // 410 : la fenêtre a existé et s'est refermée. FR-033 `@edge` le
            // demande, et cela distingue « c'est trop tard » de « c'est
            // refusé ».
            "RATING_WINDOW_CLOSED" => StatusCode::GONE,
            // 422 : la requête est bien formée, c'est la valeur qui est hors
            // échelle. FR-033 `@negative` le demande.
            _ => StatusCode::UNPROCESSABLE_ENTITY,
        },
        ErreurNotation::Indisponible(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// Note l'autre partie après une intervention validée.
#[utoipa::path(
    post,
    path = "/api/v1/missions/{id}/rating",
    tag = "notation",
    params(("id" = String, Path, description = "Identifiant de la Mission")),
    request_body = NoteDto,
    responses(
        (status = 201, description = "Note enregistrée", body = NoteEcriteDto),
        (status = 400, description = "Identifiant illisible", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou invalide", body = ErreurAuthDto),
        (status = 404, description = "Mission inconnue ou qui ne vous concerne pas", body = ErreurValidationDto),
        (status = 409, description = "Intervention non validée, ou déjà notée", body = ErreurValidationDto),
        (status = 410, description = "Fenêtre de notation fermée", body = ErreurValidationDto),
        (status = 422, description = "Note hors échelle ou commentaire trop long", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[post("/api/v1/missions/{id}/rating")]
pub async fn noter_intervention(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    chemin: web::Path<String>,
    corps: web::Json<NoteDto>,
) -> HttpResponse {
    let Ok(mission_id) = Uuid::parse_str(&chemin) else {
        return HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "MISSION_ID_INVALID".to_string(),
        });
    };

    match noter(
        Depots {
            demandes: etat.demandes.as_ref(),
            missions: etat.missions.as_ref(),
            prestataires: etat.prestataires.as_ref(),
            notations: etat.notations.as_ref(),
            horloge: etat.horloge.as_ref(),
        },
        // Tiré du jeton : c'est lui qui dit qui note, donc qui est noté.
        // Accepter une cible en entrée laisserait se noter soi-même.
        authentifie.utilisateur_id,
        mission_id,
        Avis {
            note: corps.note,
            commentaire: corps.commentaire.clone(),
        },
    )
    .await
    {
        Ok(ecrite) => HttpResponse::Created().json(NoteEcriteDto {
            id: ecrite.notation.id.to_string(),
            code: "RATING_RECORDED",
            cible: ecrite.notation.cible.as_str().to_string(),
            publiee: ecrite.publiee,
        }),
        Err(e) => {
            if matches!(e, ErreurNotation::Indisponible(_)) {
                tracing::error!(erreur = %e, "notation impossible");
            }
            HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    }
}

/// Lit les notes visibles d'une intervention.
#[utoipa::path(
    get,
    path = "/api/v1/missions/{id}/ratings",
    tag = "notation",
    params(("id" = String, Path, description = "Identifiant de la Mission")),
    responses(
        (status = 200, description = "Notes visibles, éventuellement aucune", body = NotesDeMissionDto),
        (status = 400, description = "Identifiant illisible", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou invalide", body = ErreurAuthDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[get("/api/v1/missions/{id}/ratings")]
pub async fn lire_notes(
    _authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    chemin: web::Path<String>,
) -> HttpResponse {
    let Ok(mission_id) = Uuid::parse_str(&chemin) else {
        return HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "MISSION_ID_INVALID".to_string(),
        });
    };

    // **Aucun contrôle de rôle sur cette lecture, et c'est voulu.** Une note
    // publiée est publique : c'est ce qui fait sa valeur pour qui choisit un
    // prestataire. Ce qu'elle ne dit pas — l'adresse, le montant, la
    // description — n'est pas dans cette réponse. Le jeton reste exigé pour que
    // la réputation ne s'aspire pas anonymement en masse.
    match notes_visibles(
        etat.notations.as_ref(),
        mission_id,
        etat.horloge.maintenant(),
    )
    .await
    {
        Ok(notes) => {
            let mut visibles = Vec::new();
            for notation in [notes.sur_le_prestataire, notes.sur_le_demandeur]
                .into_iter()
                .flatten()
            {
                visibles.push(NoteVisibleDto {
                    cible: notation.cible.as_str().to_string(),
                    note: notation.note,
                    commentaire: notation.commentaire,
                    cree_le: notation.cree_le.to_rfc3339(),
                });
            }
            HttpResponse::Ok().json(NotesDeMissionDto { notes: visibles })
        }
        Err(e) => {
            tracing::error!(erreur = %e, "lecture des notes impossible");
            HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    }
}
