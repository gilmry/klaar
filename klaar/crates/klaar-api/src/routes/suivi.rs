//! Suivi d'une Demande et d'une Mission (Story 4.10, FR-011, FR-018).
//!
//! Trois lectures, trois publics, et elles ne montrent pas la même chose. Le
//! détail du raisonnement est dans `klaar_application::usecases::consulter` ;
//! ce module ne fait que le transport.

use actix_web::{get, web, HttpResponse};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use klaar_application::usecases::consulter::{
    demande_du_demandeur, demandes_proposees, mission_du_prestataire, ErreurConsultation, VueDevis,
};

use crate::auth::Authentifie;
use crate::routes::auth::ErreurValidationDto;
use crate::EtatApplication;

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SuiviDemandeDto {
    pub id: String,
    pub secteur: String,
    pub description: String,
    pub urgence: String,
    pub statut: String,
    pub rayon_metres: f64,
    pub elargissements: u8,
    /// Le tour de diffusion est écoulé, même si le statut dit encore
    /// « diffusion » : le balayage n'est pas passé. L'exposer évite d'attendre
    /// pour rien.
    pub tour_ecoule: bool,
    /// Nom de l'entreprise attribuée, une fois la Mission créée.
    pub prestataire: Option<String>,
    pub mission_id: Option<String>,
    pub mission_statut: Option<String>,
    /// Dernier devis reçu, quel que soit son statut (FR-016).
    pub devis: Option<DevisDto>,
}

/// Le dernier devis d'une Mission, pour l'une comme pour l'autre partie.
///
/// Tous les montants en centimes : un devis n'est pas l'endroit où découvrir
/// que 0,1 + 0,2 ne fait pas 0,3 en binaire.
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DevisDto {
    pub id: String,
    pub montant_htva_cents: i64,
    pub taux_tva_bp: u16,
    pub tva_cents: i64,
    pub total_ttc_cents: i64,
    pub delai_minutes: i64,
    pub note: Option<String>,
    pub statut: String,
    pub secondes_restantes: i64,
    /// L'heure de validité est passée, même si le statut dit encore
    /// « en attente » : le balayage n'est pas venu. Exposé pour la même raison
    /// que `tour_ecoule`.
    pub echu: bool,
}

impl From<VueDevis> for DevisDto {
    fn from(vue: VueDevis) -> Self {
        Self {
            id: vue.devis_id.to_string(),
            montant_htva_cents: vue.montant_htva_cents,
            taux_tva_bp: vue.taux_tva_bp,
            tva_cents: vue.tva_cents,
            total_ttc_cents: vue.total_ttc_cents,
            delai_minutes: vue.delai_minutes,
            note: vue.note,
            statut: vue.statut.as_str().to_string(),
            secondes_restantes: vue.secondes_restantes,
            echu: vue.echu,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DemandeProposeeDto {
    pub id: String,
    pub secteur: String,
    pub description: String,
    pub urgence: String,
    /// Distance qui sépare le prestataire du lieu, en mètres.
    ///
    /// **Il n'y a pas de champ pour l'adresse.** Elle n'est révélée qu'une fois
    /// la Mission attribuée : donner à dix entreprises l'adresse d'un foyer
    /// pour un dépannage que neuf ne feront pas n'a aucune justification.
    pub distance_metres: f64,
    /// Secondes restantes avant la fin du tour de diffusion.
    pub secondes_restantes: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SuiviMissionDto {
    pub id: String,
    pub statut: String,
    pub secteur: String,
    pub description: String,
    pub urgence: String,
    /// L'adresse d'intervention. Visible ici et nulle part avant : le
    /// prestataire attribué doit s'y rendre.
    pub latitude: f64,
    pub longitude: f64,
    /// Statuts atteignables depuis l'état courant.
    ///
    /// Rendus par le serveur pour que l'interface n'invente pas un bouton que
    /// le domaine refusera, et n'ait pas à recopier la machine à états.
    pub suites: Vec<String>,
    /// Dernier devis envoyé pour cette Mission (FR-016).
    pub devis: Option<DevisDto>,
    /// Devis encore envoyables avant que le plafond n'annule la Mission.
    pub devis_restants: usize,
}

fn statut(e: &ErreurConsultation) -> actix_web::http::StatusCode {
    use actix_web::http::StatusCode;
    match e {
        ErreurConsultation::Introuvable => StatusCode::NOT_FOUND,
        ErreurConsultation::PasPrestataire => StatusCode::FORBIDDEN,
        ErreurConsultation::Indisponible(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn refus(e: ErreurConsultation) -> HttpResponse {
    if matches!(e, ErreurConsultation::Indisponible(_)) {
        tracing::error!(erreur = %e, "consultation impossible");
    }
    HttpResponse::build(statut(&e)).json(ErreurValidationDto {
        code: e.code().to_string(),
    })
}

/// Suit sa propre Demande.
#[utoipa::path(
    get,
    path = "/api/v1/requests/{id}",
    tag = "demandes",
    params(("id" = String, Path, description = "Identifiant de la Demande")),
    responses(
        (status = 200, description = "État de la Demande", body = SuiviDemandeDto),
        (status = 400, description = "Identifiant illisible", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou invalide"),
        (status = 404, description = "Demande inconnue ou appartenant à quelqu'un d'autre", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[get("/api/v1/requests/{id}")]
pub async fn suivre_demande(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    chemin: web::Path<String>,
) -> HttpResponse {
    let Ok(demande_id) = Uuid::parse_str(&chemin) else {
        return HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "REQUEST_ID_INVALID".to_string(),
        });
    };

    match demande_du_demandeur(
        etat.demandes.as_ref(),
        etat.missions.as_ref(),
        etat.prestataires.as_ref(),
        etat.devis.as_ref(),
        etat.horloge.as_ref(),
        authentifie.utilisateur_id,
        demande_id,
    )
    .await
    {
        Ok(vue) => HttpResponse::Ok().json(SuiviDemandeDto {
            id: vue.demande.id.to_string(),
            secteur: vue.demande.secteur.to_string(),
            description: vue.demande.description,
            urgence: vue.demande.urgence.as_str().to_string(),
            statut: vue.demande.statut.as_str().to_string(),
            rayon_metres: vue.demande.rayon_metres,
            elargissements: vue.demande.elargissements,
            tour_ecoule: vue.tour_ecoule,
            prestataire: vue.prestataire,
            mission_id: vue.mission_id.map(|i| i.to_string()),
            mission_statut: vue.mission_statut.map(|s| s.as_str().to_string()),
            devis: vue.devis.map(DevisDto::from),
        }),
        Err(e) => refus(e),
    }
}

/// Liste les Demandes ouvertes proposées au prestataire.
#[utoipa::path(
    get,
    path = "/api/v1/providers/me/requests",
    tag = "prestataires",
    responses(
        (status = 200, description = "Demandes proposées", body = Vec<DemandeProposeeDto>),
        (status = 401, description = "Jeton absent ou invalide"),
        (status = 403, description = "Ce compte n'est pas un prestataire", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[get("/api/v1/providers/me/requests")]
pub async fn demandes_recues(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
) -> HttpResponse {
    match demandes_proposees(
        etat.prestataires.as_ref(),
        etat.demandes.as_ref(),
        etat.horloge.as_ref(),
        authentifie.utilisateur_id,
    )
    .await
    {
        Ok(vues) => HttpResponse::Ok().json(
            vues.into_iter()
                .map(|v| DemandeProposeeDto {
                    id: v.demande_id.to_string(),
                    secteur: v.secteur,
                    description: v.description,
                    urgence: v.urgence,
                    distance_metres: v.distance_metres,
                    secondes_restantes: v.secondes_restantes,
                })
                .collect::<Vec<_>>(),
        ),
        Err(e) => refus(e),
    }
}

/// Suit une Mission qui lui est attribuée.
#[utoipa::path(
    get,
    path = "/api/v1/missions/{id}",
    tag = "missions",
    params(("id" = String, Path, description = "Identifiant de la Mission")),
    responses(
        (status = 200, description = "État de la Mission", body = SuiviMissionDto),
        (status = 400, description = "Identifiant illisible", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou invalide"),
        (status = 403, description = "Ce compte n'est pas un prestataire", body = ErreurValidationDto),
        (status = 404, description = "Mission inconnue ou attribuée à quelqu'un d'autre", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[get("/api/v1/missions/{id}")]
pub async fn suivre_mission(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    chemin: web::Path<String>,
) -> HttpResponse {
    let Ok(mission_id) = Uuid::parse_str(&chemin) else {
        return HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "MISSION_ID_INVALID".to_string(),
        });
    };

    match mission_du_prestataire(
        etat.prestataires.as_ref(),
        etat.missions.as_ref(),
        etat.demandes.as_ref(),
        etat.devis.as_ref(),
        etat.horloge.as_ref(),
        authentifie.utilisateur_id,
        mission_id,
    )
    .await
    {
        Ok(vue) => HttpResponse::Ok().json(SuiviMissionDto {
            id: vue.mission_id.to_string(),
            statut: vue.statut.as_str().to_string(),
            secteur: vue.secteur,
            description: vue.description,
            urgence: vue.urgence,
            latitude: vue.position.lat(),
            longitude: vue.position.lon(),
            suites: vue.suites.into_iter().map(String::from).collect(),
            devis: vue.devis.map(DevisDto::from),
            devis_restants: vue.devis_restants,
        }),
        Err(e) => refus(e),
    }
}
