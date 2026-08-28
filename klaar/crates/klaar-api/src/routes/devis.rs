//! Envoi d'un devis par le prestataire attribué (Story 4.1, FR-016).

use actix_web::{post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use klaar_application::ports::demande_repository::DemandeRepository;
use klaar_application::ports::devis_repository::DevisRepository;
use klaar_application::ports::mission_repository::MissionRepository;
use klaar_application::ports::provider_repository::ProviderRepository;
use klaar_application::usecases::emettre_devis::{emettre_devis, DevisEmis, ErreurEmissionDevis};
use klaar_application::usecases::notifier::{
    notifier_avancement, notifier_devis_recu, notifier_reponse_devis,
};
use klaar_application::usecases::repondre_devis::{
    code_reponse, repondre, DevisRepondu, ErreurReponse, Reponse,
};
use klaar_payment::Proposition;

use klaar_application::usecases::langue::langue_de;

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
        langue_de(etat.utilisateurs.as_ref(), demande.demandeur_id).await,
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
        langue_de(etat.utilisateurs.as_ref(), demande.demandeur_id).await,
    )
    .await
    {
        tracing::error!(erreur = %e, "avis d'annulation non délivré");
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RefusDto {
    /// `TOO_EXPENSIVE`, `DELAY_TOO_LONG`, `NO_LONGER_NEEDED` ou `OTHER`.
    ///
    /// Facultatif : exiger une raison obligerait à en choisir une pour dire
    /// non, ce qui n'est pas dû.
    pub motif: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReponseDevisDto {
    pub id: String,
    pub statut: String,
    pub code: &'static str,
    /// Le prestataire a-t-il été joint sur au moins un appareil.
    pub prestataire_prevenu: bool,
}

fn statut_reponse(e: &ErreurReponse) -> actix_web::http::StatusCode {
    use actix_web::http::StatusCode;
    match e {
        ErreurReponse::Introuvable => StatusCode::NOT_FOUND,
        ErreurReponse::MotifInconnu => StatusCode::BAD_REQUEST,
        ErreurReponse::Domaine(d) => match d.code() {
            // 410 : le devis a existé et n'existe plus comme offre. FR-017
            // `@edge` le demande, et cela distingue « c'est fini » de « c'est
            // déjà fait ».
            "QUOTE_EXPIRED" => StatusCode::GONE,
            _ => StatusCode::CONFLICT,
        },
        ErreurReponse::Concurrence => StatusCode::CONFLICT,
        ErreurReponse::Indisponible(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// Chemin commun aux deux réponses : la seule différence est la décision.
async fn enregistrer(
    etat: &EtatApplication,
    utilisateur_id: Uuid,
    devis_id: Uuid,
    reponse: Reponse<'_>,
) -> HttpResponse {
    match repondre(
        etat.demandes.as_ref(),
        etat.missions.as_ref(),
        etat.devis.as_ref(),
        etat.horloge.as_ref(),
        utilisateur_id,
        devis_id,
        reponse,
    )
    .await
    {
        Ok(repondu) => {
            let prestataire_prevenu = prevenir_le_prestataire(etat, &repondu).await;
            HttpResponse::Ok().json(ReponseDevisDto {
                id: repondu.devis.id.to_string(),
                statut: repondu.devis.statut.as_str().to_string(),
                code: code_reponse(repondu.devis.statut),
                prestataire_prevenu,
            })
        }
        Err(e) => {
            if matches!(e, ErreurReponse::Indisponible(_)) {
                tracing::error!(erreur = %e, "réponse au devis impossible");
            }
            HttpResponse::build(statut_reponse(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    }
}

/// Accepte le devis en attente de sa Mission.
#[utoipa::path(
    post,
    path = "/api/v1/missions/{id}/accept-quote",
    tag = "devis",
    params(("id" = String, Path, description = "Identifiant de la Mission")),
    responses(
        (status = 200, description = "Devis accepté", body = ReponseDevisDto),
        (status = 401, description = "Jeton absent ou invalide"),
        (status = 404, description = "Aucun devis en attente pour cette Mission", body = ErreurValidationDto),
        (status = 409, description = "Devis déjà répondu", body = ErreurValidationDto),
        (status = 410, description = "Devis expiré", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[post("/api/v1/missions/{id}/accept-quote")]
pub async fn accepter_devis(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    chemin: web::Path<String>,
) -> HttpResponse {
    match devis_en_attente(&etat, &chemin).await {
        Ok(devis_id) => {
            enregistrer(
                &etat,
                authentifie.utilisateur_id,
                devis_id,
                Reponse::Accepter,
            )
            .await
        }
        Err(refus) => refus.en_reponse(),
    }
}

/// Refuse le devis en attente de sa Mission.
#[utoipa::path(
    post,
    path = "/api/v1/missions/{id}/refuse-quote",
    tag = "devis",
    params(("id" = String, Path, description = "Identifiant de la Mission")),
    request_body = RefusDto,
    responses(
        (status = 200, description = "Devis refusé", body = ReponseDevisDto),
        (status = 400, description = "Motif hors vocabulaire", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou invalide"),
        (status = 404, description = "Aucun devis en attente pour cette Mission", body = ErreurValidationDto),
        (status = 409, description = "Devis déjà répondu", body = ErreurValidationDto),
        (status = 410, description = "Devis expiré", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    ),
    security(("bearer" = []))
)]
#[post("/api/v1/missions/{id}/refuse-quote")]
pub async fn refuser_devis(
    authentifie: Authentifie,
    etat: web::Data<EtatApplication>,
    chemin: web::Path<String>,
    corps: Option<web::Json<RefusDto>>,
) -> HttpResponse {
    match devis_en_attente(&etat, &chemin).await {
        Ok(devis_id) => {
            let motif = corps.as_ref().and_then(|c| c.motif.as_deref());
            enregistrer(
                &etat,
                authentifie.utilisateur_id,
                devis_id,
                Reponse::Refuser(motif),
            )
            .await
        }
        Err(refus) => refus.en_reponse(),
    }
}

/// Trouve le devis en attente d'une Mission, ou rend la réponse d'échec.
///
/// **Les routes portent l'identifiant de la Mission, pas celui du devis**, parce
/// que c'est ce que le PRD fixe et que c'est ce que le demandeur a sous les yeux
/// : son écran suit une intervention, pas une suite de documents. Un devis en
/// attente au plus par Mission rend la résolution non ambiguë — c'est l'index
/// unique partiel de V19 qui le garantit.
///
/// Aucun droit n'est vérifié ici : la lecture ne dit rien de plus que
/// « quelque chose existe », et le cas d'usage refuse ensuite tout ce qui ne
/// regarde pas l'appelant.
async fn devis_en_attente(etat: &EtatApplication, chemin: &str) -> Result<Uuid, SansDevis> {
    let Ok(mission_id) = Uuid::parse_str(chemin) else {
        return Err(SansDevis::IdentifiantInvalide);
    };

    match etat.devis.en_cours_pour_mission(mission_id).await {
        Ok(Some(devis)) => Ok(devis.id),
        Ok(None) => Err(SansDevis::Aucun),
        Err(e) => {
            tracing::error!(erreur = %e, "lecture du devis impossible");
            Err(SansDevis::Indisponible)
        }
    }
}

/// Pourquoi la résolution du devis n'a rien donné.
///
/// Un petit type plutôt qu'une `HttpResponse` portée dans un `Result` : une
/// réponse HTTP pèse plus de cent octets, et la faire voyager dans la variante
/// d'erreur de chaque appel alourdit tous les chemins, y compris celui qui
/// réussit.
enum SansDevis {
    IdentifiantInvalide,
    Aucun,
    Indisponible,
}

impl SansDevis {
    fn en_reponse(self) -> HttpResponse {
        let (statut, code) = match self {
            Self::IdentifiantInvalide => (
                actix_web::http::StatusCode::BAD_REQUEST,
                "MISSION_ID_INVALID",
            ),
            Self::Aucun => (actix_web::http::StatusCode::NOT_FOUND, "QUOTE_NOT_FOUND"),
            Self::Indisponible => (
                actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
                "SERVICE_UNAVAILABLE",
            ),
        };
        HttpResponse::build(statut).json(ErreurValidationDto {
            code: code.to_string(),
        })
    }
}

/// Prévient le prestataire de la réponse. Jamais bloquant.
async fn prevenir_le_prestataire(etat: &EtatApplication, repondu: &DevisRepondu) -> bool {
    let Some(sender) = etat.push.as_ref() else {
        return false;
    };
    let compte = match etat.prestataires.par_id(repondu.provider_id).await {
        Ok(Some(p)) => p.utilisateur_id,
        Ok(None) => return false,
        Err(e) => {
            tracing::error!(erreur = %e, "prestataire illisible pour l'avis de réponse");
            return false;
        }
    };
    notifier_reponse_devis(
        etat.abonnements.as_ref(),
        sender.as_ref(),
        compte,
        repondu.devis.statut,
        langue_de(etat.utilisateurs.as_ref(), compte).await,
    )
    .await
    .map(|bilan| bilan.notifies > 0)
    .unwrap_or_else(|e| {
        tracing::error!(erreur = %e, "avis de réponse non délivré");
        false
    })
}
