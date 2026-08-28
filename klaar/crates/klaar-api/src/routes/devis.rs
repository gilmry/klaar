//! Envoi d'un devis par le prestataire attribué (Story 4.1, FR-016).

use actix_web::{post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use klaar_application::ports::demande_repository::DemandeRepository;
use klaar_application::ports::mission_repository::MissionRepository;
use klaar_application::usecases::emettre_devis::{emettre_devis, DevisEmis, ErreurEmissionDevis};
use klaar_application::usecases::notifier::{notifier_avancement, notifier_devis_recu};
use klaar_payment::Proposition;

use crate::auth::Authentifie;
use crate::routes::auth::ErreurValidationDto;
use crate::EtatApplication;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PropositionDto {
    /// Montant hors TVA, **en centimes**. Jamais un flottant : 0,1 + 0,2 ne
    /// fait pas 0,3 en binaire, et un devis n'est pas l'endroit où le
    /// découvrir.
    pub montant_htva_cents: i64,
    /// Taux de TVA en points de base : 2100, 1200 ou 600 (Architecture §6.5).
    pub taux_tva_bp: u16,
    /// Délai d'intervention annoncé, en minutes. Au plus 24 h.
    pub delai_minutes: i64,
    /// Note libre affichée au demandeur.
    pub note: Option<String>,
    /// Référence de la preuve justifiant un taux réduit. Obligatoire dès que le
    /// taux n'est pas 2100.
    pub preuve_tva_reduite: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DevisEmisDto {
    pub id: String,
    pub code: &'static str,
    pub montant_htva_cents: i64,
    pub taux_tva_bp: u16,
    /// Calculée à l'émission et conservée : un devis relu plus tard doit
    /// montrer ce qui a été présenté ce jour-là.
    pub tva_cents: i64,
    pub total_ttc_cents: i64,
    pub delai_minutes: i64,
    pub statut: String,
    /// Instant d'expiration, en RFC 3339.
    pub expire_le: String,
    /// Le demandeur a-t-il été joint sur au moins un appareil.
    pub demandeur_prevenu: bool,
}

fn statut(e: &ErreurEmissionDevis) -> actix_web::http::StatusCode {
    use actix_web::http::StatusCode;
    match e {
        ErreurEmissionDevis::NonEligible => StatusCode::FORBIDDEN,
        ErreurEmissionDevis::Introuvable => StatusCode::NOT_FOUND,
        // 409 : la Mission existe, c'est son état qui refuse.
        ErreurEmissionDevis::MissionClose | ErreurEmissionDevis::DevisEnCours => {
            StatusCode::CONFLICT
        }
        // 422 et non 409 : la requête est bien formée et la Mission acceptait
        // encore un devis il y a un instant. C'est une règle métier qui la
        // refuse, et FR-016 `@edge` demande ce code.
        ErreurEmissionDevis::PlafondAtteint { .. } => StatusCode::UNPROCESSABLE_ENTITY,
        ErreurEmissionDevis::Domaine(d) => match d.code() {
            // 422 : le délai est bien formé, c'est sa valeur qui est refusée.
            // FR-016 `@negative` le distingue explicitement des montants.
            "DELAY_TOO_LONG" => StatusCode::UNPROCESSABLE_ENTITY,
            _ => StatusCode::BAD_REQUEST,
        },
        ErreurEmissionDevis::Indisponible(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// Envoie un devis pour une Mission.
#[utoipa::path(
    post,
    path = "/api/v1/missions/{id}/quote",
    tag = "devis",
    params(("id" = String, Path, description = "Identifiant de la Mission")),
    request_body = PropositionDto,
    responses(
        (status = 201, description = "Devis envoyé", body = DevisEmisDto),
        (status = 400, description = "Montant, taux ou texte refusé", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou invalide"),
        (status = 403, description = "Ce compte ne peut pas émettre de devis", body = ErreurValidationDto),
        (status = 404, description = "Mission inconnue ou attribuée à quelqu'un d'autre", body = ErreurValidationDto),
        (status = 409, description = "Mission close, ou devis déjà en attente", body = ErreurValidationDto),
        (status = 422, description = "Délai trop long, ou plafond de devis atteint", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[post("/api/v1/missions/{id}/quote")]
pub async fn envoyer_devis(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    chemin: web::Path<String>,
    corps: web::Json<PropositionDto>,
) -> HttpResponse {
    let Ok(mission_id) = Uuid::parse_str(&chemin) else {
        return HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "MISSION_ID_INVALID".to_string(),
        });
    };

    match emettre_devis(
        etat.prestataires.as_ref(),
        etat.missions.as_ref(),
        etat.devis.as_ref(),
        etat.horloge.as_ref(),
        // Tiré du jeton : accepter un identifiant de prestataire en entrée
        // laisserait chiffrer une intervention au nom d'un autre.
        authentifie.utilisateur_id,
        mission_id,
        Proposition {
            montant_htva_cents: corps.montant_htva_cents,
            taux_tva_bp: corps.taux_tva_bp,
            delai_minutes: corps.delai_minutes,
            note: corps.note.clone(),
            preuve_tva_reduite: corps.preuve_tva_reduite.clone(),
        },
    )
    .await
    {
        Ok(emis) => {
            // Prévenir suit l'écriture et n'en fait pas partie : une panne du
            // service de push ne doit pas effacer un devis déjà émis. Le
            // demandeur le verra en ouvrant l'application.
            let demandeur_prevenu = prevenir_du_devis(&etat, &emis).await;

            HttpResponse::Created().json(DevisEmisDto {
                id: emis.devis.id.to_string(),
                code: "QUOTE_SENT",
                montant_htva_cents: emis.devis.montant_htva.cents(),
                taux_tva_bp: emis.devis.taux_tva.basis_points(),
                tva_cents: emis.devis.tva.cents(),
                total_ttc_cents: emis.devis.total_ttc.cents(),
                delai_minutes: emis.devis.delai_minutes,
                statut: emis.devis.statut.as_str().to_string(),
                expire_le: emis.devis.expire_le.to_rfc3339(),
                demandeur_prevenu,
            })
        }
        Err(e) => {
            if let ErreurEmissionDevis::PlafondAtteint {
                mission_annulee: true,
            } = e
            {
                // La Mission vient d'être annulée sous les pieds du demandeur.
                // Le lui dire est le minimum : il attend quelqu'un.
                prevenir_de_l_annulation(&etat, mission_id).await;
            }
            if matches!(e, ErreurEmissionDevis::Indisponible(_)) {
                tracing::error!(erreur = %e, "émission de devis impossible");
            }
            HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    }
}

/// Envoie l'avis « vous avez reçu un devis ». Jamais bloquant.
async fn prevenir_du_devis(etat: &EtatApplication, emis: &DevisEmis) -> bool {
    let Some(sender) = etat.push.as_ref() else {
        return false;
    };
    let demande = match etat.demandes.par_id(emis.demande_id).await {
        Ok(Some(d)) => d,
        Ok(None) => return false,
        Err(e) => {
            tracing::error!(erreur = %e, "Demande illisible pour l'avis de devis");
            return false;
        }
    };
    notifier_devis_recu(
        etat.abonnements.as_ref(),
        sender.as_ref(),
        &demande,
        klaar_shared_kernel::Locale::Fr,
    )
    .await
    .map(|bilan| bilan.notifies > 0)
    .unwrap_or_else(|e| {
        tracing::error!(erreur = %e, "avis de devis non délivré");
        false
    })
}

/// Prévient le demandeur que la Mission a été annulée au quatrième devis.
async fn prevenir_de_l_annulation(etat: &EtatApplication, mission_id: Uuid) {
    let Some(sender) = etat.push.as_ref() else {
        return;
    };
    let Ok(Some(mission)) = etat.missions.par_id(mission_id).await else {
        return;
    };
    let Ok(Some(demande)) = etat.demandes.par_id(mission.demande_id).await else {
        return;
    };
    if let Err(e) = notifier_avancement(
        etat.abonnements.as_ref(),
        sender.as_ref(),
        &demande,
        klaar_intervention::StatutMission::Annulee,
        klaar_shared_kernel::Locale::Fr,
    )
    .await
    {
        tracing::error!(erreur = %e, "avis d'annulation non délivré");
    }
}
