//! Soumission d'une Demande (Story 3.1, FR-011).

use actix_web::{post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use klaar_application::usecases::matcher::{chercher_candidats, ResultatMatching};
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
    /// Prestataires retenus pour notification (FR-012). Zéro signifie que
    /// personne n'a été trouvé dans le rayon, et la Demande passe en
    /// `NO_MATCH` — une réponse utile pour qui attend, plutôt qu'un silence.
    pub candidats: usize,
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
        Ok(ResultatSoumission::Creee(demande)) => {
            // Le matching est lancé **dans la requête**, alors que FR-011 le
            // décrit asynchrone. Il n'y a pas de file de travaux dans ce
            // périmètre, et la seule alternative — un binaire cadencé —
            // retarderait la diffusion de sa période entière, ce qui est le
            // contraire de ce qu'on veut sur un dépannage. La requête y perd
            // une requête spatiale indexée, de l'ordre de la milliseconde.
            let candidats = match chercher_candidats(
                etat.prestataires.as_ref(),
                etat.demandes.as_ref(),
                etat.traces.as_ref(),
                etat.horloge.as_ref(),
                &demande,
            )
            .await
            {
                Ok(ResultatMatching::Candidats(c)) => c.len(),
                Ok(ResultatMatching::Aucun) => 0,
                Err(e) => {
                    // Un matching en échec ne défait pas la Demande : elle
                    // existe, et un tour ultérieur pourra la reprendre. Faire
                    // l'inverse ferait perdre à l'utilisateur ce qu'il vient
                    // d'écrire pour une panne qui ne le concerne pas.
                    tracing::error!(erreur = %e, "matching impossible");
                    0
                }
            };

            HttpResponse::Created().json(DemandeCreeeDto {
                id: demande.id.to_string(),
                statut: demande.statut.as_str().to_string(),
                code: "REQUEST_CREATED",
                candidats,
            })
        }
        // 200 et non 409 : FR-011 `@edge` demande que la Demande existante soit
        // rendue. Un 409 obligerait le client à la retrouver lui-même, alors
        // que c'est précisément ce qu'il cherchait.
        // Aucun nouveau tour de matching sur un doublon : la Demande d'origine
        // a déjà été diffusée, et relancer réveillerait les mêmes prestataires
        // pour la même chose.
        Ok(ResultatSoumission::Doublon(demande)) => HttpResponse::Ok().json(DemandeCreeeDto {
            id: demande.id.to_string(),
            statut: demande.statut.as_str().to_string(),
            code: "REQUEST_DUPLICATE",
            candidats: 0,
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
