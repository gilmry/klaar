//! Soumission d'une Demande (Story 3.1, FR-011).

use actix_web::{post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use klaar_application::usecases::soumettre_demande::{
    soumettre, CommandeSoumission, ErreurSoumission, ResultatSoumission,
};

use crate::auth::Authentifie;
use crate::routes::auth::ErreurValidationDto;
use crate::EtatApplication;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DemandeDto {
    /// Code de secteur du catalogue, par exemple `plomberie`.
    pub secteur: String,
    pub description: String,
    pub latitude: f64,
    pub longitude: f64,
    /// `LOW`, `NORMAL` ou `HIGH`.
    pub urgence: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DemandeCreeeDto {
    pub id: String,
    pub statut: String,
    /// `REQUEST_CREATED`, ou `REQUEST_DUPLICATE` quand une Demande identique
    /// existait déjà et est rendue à sa place.
    pub code: &'static str,
}

fn statut(e: &ErreurSoumission) -> actix_web::http::StatusCode {
    use actix_web::http::StatusCode;
    match e {
        // 422 et non 400 : la requête est bien formée, c'est l'état du compte
        // qui empêche de la traiter. FR-011 le nomme explicitement.
        ErreurSoumission::MethodePaiementAbsente => StatusCode::UNPROCESSABLE_ENTITY,
        ErreurSoumission::QuotaAtteint => StatusCode::TOO_MANY_REQUESTS,
        ErreurSoumission::Indisponible(_) => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::BAD_REQUEST,
    }
}

/// Soumet une Demande.
#[utoipa::path(
    post,
    path = "/api/v1/requests",
    tag = "demandes",
    request_body = DemandeDto,
    responses(
        (status = 201, description = "Demande créée en diffusion", body = DemandeCreeeDto),
        (status = 200, description = "Demande identique déjà en cours, rendue telle quelle", body = DemandeCreeeDto),
        (status = 400, description = "Saisie invalide", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou invalide"),
        (status = 422, description = "Méthode de paiement requise", body = ErreurValidationDto),
        (status = 429, description = "Quota de Demandes atteint", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[post("/api/v1/requests")]
pub async fn soumettre_demande(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    corps: web::Json<DemandeDto>,
) -> HttpResponse {
    match soumettre(
        etat.demandes.as_ref(),
        etat.catalogue.as_ref(),
        etat.paiements.as_ref(),
        etat.journal.as_ref(),
        etat.horloge.as_ref(),
        etat.exiger_methode_paiement,
        CommandeSoumission {
            // Tiré du jeton, jamais du corps : accepter un `demandeur_id` en
            // entrée laisserait soumettre une Demande au nom d'autrui.
            demandeur_id: authentifie.utilisateur_id,
            secteur: corps.secteur.clone(),
            description: corps.description.clone(),
            latitude: corps.latitude,
            longitude: corps.longitude,
            urgence: corps.urgence.clone(),
        },
    )
    .await
    {
        Ok(ResultatSoumission::Creee(demande)) => HttpResponse::Created().json(DemandeCreeeDto {
            id: demande.id.to_string(),
            statut: demande.statut.as_str().to_string(),
            code: "REQUEST_CREATED",
        }),
        // 200 et non 409 : FR-011 `@edge` demande que la Demande existante soit
        // rendue. Un 409 obligerait le client à la retrouver lui-même, alors
        // que c'est précisément ce qu'il cherchait.
        Ok(ResultatSoumission::Doublon(demande)) => HttpResponse::Ok().json(DemandeCreeeDto {
            id: demande.id.to_string(),
            statut: demande.statut.as_str().to_string(),
            code: "REQUEST_DUPLICATE",
        }),
        Err(e) => {
            if matches!(e, ErreurSoumission::Indisponible(_)) {
                tracing::error!(erreur = %e, "soumission de Demande impossible");
            }
            HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    }
}
