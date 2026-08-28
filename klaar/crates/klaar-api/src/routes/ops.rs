//! Console d'exploitation : connexion, journal, gestion des comptes
//! (Story 8.4, FR-041, FR-042).
//!
//! **Un espace de noms à part, `/api/v1/ops`.** Ce n'est pas cosmétique : cela
//! permet de couper toute la console derrière un pare-feu ou un VPN d'un seul
//! préfixe, sans démêler des routes mélangées à celles du public.
//!
//! **Aucune session persistante ici.** Chaque requête d'exploitation porte ses
//! identifiants et son code : c'est plus lourd à l'usage, et c'est délibéré —
//! un jeton d'exploitation volé donnerait accès aux Demandes, aux litiges et
//! aux montants de tout le monde pendant sa durée de vie. Le jour où la console
//! aura des sessions, elles devront être courtes et liées à l'appareil.

use actix_web::{get, post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use klaar_application::ports::ops_repository::OpsRepository;
use klaar_application::usecases::ops::{
    autoriser_et_consigner, connecter, lire_journal, secret_totp_neuf, ErreurOps, JOURNAL_PAR_PAGE,
};
use klaar_identity::{CompteOps, MotDePasse, Permission};
use klaar_shared_kernel::Email;

use crate::routes::auth::ErreurValidationDto;
use crate::EtatApplication;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnexionOpsDto {
    pub email: String,
    pub mot_de_passe: String,
    /// Code à six chiffres de l'application d'authentification.
    pub code: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionOpsDto {
    pub id: String,
    pub role: String,
    pub code: &'static str,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreationOpsDto {
    pub email: String,
    pub mot_de_passe: String,
    /// `SUPER_ADMIN`, `KYC_REVIEWER`, `MEDIATOR` ou `READER`.
    pub role: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CompteOpsCreeDto {
    pub id: String,
    pub role: String,
    pub code: &'static str,
    /// Secret à scanner dans l'application d'authentification, en base32.
    ///
    /// **Rendu une seule fois.** Il n'est plus jamais lisible ensuite : le
    /// réinitialiser demande un super-administrateur.
    pub secret_totp: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GesteOpsDto {
    pub acteur: Option<String>,
    pub geste: String,
    pub cible: Option<String>,
    /// En RFC 3339.
    pub fait_le: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JournalOpsDto {
    pub gestes: Vec<GesteOpsDto>,
    /// Nombre de lignes par page, fixé par le service.
    pub par_page: i64,
}

/// En-têtes d'authentification d'exploitation.
///
/// Trois valeurs plutôt qu'un jeton : voir l'en-tête du module.
#[derive(Deserialize)]
pub struct IdentifiantsOps {
    email: String,
    mot_de_passe: String,
    code: String,
}

fn statut(e: &ErreurOps) -> actix_web::http::StatusCode {
    use actix_web::http::StatusCode;
    match e {
        ErreurOps::Refuse => StatusCode::UNAUTHORIZED,
        // 403 : les identifiants sont bons, c'est l'état du compte ou son rôle
        // qui refuse.
        ErreurOps::Indisponible(_) | ErreurOps::Interdit => StatusCode::FORBIDDEN,
        ErreurOps::ServiceIndisponible(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// Ouvre une session d'exploitation.
#[utoipa::path(
    post,
    path = "/api/v1/ops/login",
    tag = "exploitation",
    request_body = ConnexionOpsDto,
    responses(
        (status = 200, description = "Identifiants acceptés", body = SessionOpsDto),
        (status = 401, description = "Adresse, mot de passe ou code refusé", body = ErreurValidationDto),
        (status = 403, description = "Compte désactivé ou seconde authentification à configurer", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    )
)]
#[post("/api/v1/ops/login")]
pub async fn connexion_ops(
    etat: web::Data<EtatApplication>,
    corps: web::Json<ConnexionOpsDto>,
) -> HttpResponse {
    match authentifier(
        &etat,
        &IdentifiantsOps {
            email: corps.email.clone(),
            mot_de_passe: corps.mot_de_passe.clone(),
            code: corps.code.clone(),
        },
    )
    .await
    {
        Ok(compte) => HttpResponse::Ok().json(SessionOpsDto {
            id: compte.id.to_string(),
            role: compte.role.as_str().to_string(),
            code: "OPS_AUTHENTICATED",
        }),
        Err(e) => HttpResponse::build(statut(&e)).json(ErreurValidationDto {
            code: e.code().to_string(),
        }),
    }
}

/// Crée un compte d'exploitation.
#[utoipa::path(
    post,
    path = "/api/v1/ops/accounts",
    tag = "exploitation",
    request_body = CreationOpsDto,
    responses(
        (status = 201, description = "Compte créé, secret à scanner", body = CompteOpsCreeDto),
        (status = 401, description = "Identifiants refusés", body = ErreurValidationDto),
        (status = 403, description = "Droit manquant", body = ErreurValidationDto),
        (status = 409, description = "Adresse déjà prise", body = ErreurValidationDto),
        (status = 422, description = "Rôle inconnu ou mot de passe trop faible", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    )
)]
#[post("/api/v1/ops/accounts")]
pub async fn creer_compte_ops(
    etat: web::Data<EtatApplication>,
    identifiants: web::Query<IdentifiantsOps>,
    corps: web::Json<CreationOpsDto>,
) -> HttpResponse {
    let demandeur = match authentifier(&etat, &identifiants).await {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    };

    if let Err(e) = autoriser_et_consigner(
        etat.ops.as_ref(),
        etat.horloge.as_ref(),
        demandeur.id,
        Permission::GererOps,
        Some(&corps.email),
    )
    .await
    {
        return HttpResponse::build(statut(&e)).json(ErreurValidationDto {
            code: e.code().to_string(),
        });
    }

    let (Ok(email), Ok(mot_de_passe)) = (
        Email::parse(&corps.email),
        MotDePasse::parse(&corps.mot_de_passe),
    ) else {
        return HttpResponse::UnprocessableEntity().json(ErreurValidationDto {
            code: "CREDENTIALS_INVALID".to_string(),
        });
    };
    let Ok(empreinte) = klaar_identity::EmpreinteMotDePasse::calculer(&mot_de_passe, etat.argon2)
    else {
        return HttpResponse::ServiceUnavailable().json(ErreurValidationDto {
            code: "SERVICE_UNAVAILABLE".to_string(),
        });
    };

    let maintenant = klaar_application::ports::horloge::Horloge::maintenant(etat.horloge.as_ref());
    let mut compte = match CompteOps::creer(email, empreinte, &corps.role, maintenant) {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::UnprocessableEntity().json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    };

    // Le secret est tiré ici et écrit avec le compte : un compte créé sans
    // secret obligerait à un second appel pour être utilisable, et c'est
    // exactement le moment où l'on oublie de le faire.
    let (secret, lisible) = secret_totp_neuf();
    compte.secret_totp = Some(secret);

    match etat.ops.creer(&compte).await {
        Ok(true) => HttpResponse::Created().json(CompteOpsCreeDto {
            id: compte.id.to_string(),
            role: compte.role.as_str().to_string(),
            code: "OPS_ACCOUNT_CREATED",
            secret_totp: lisible,
        }),
        Ok(false) => HttpResponse::Conflict().json(ErreurValidationDto {
            code: "EMAIL_TAKEN".to_string(),
        }),
        Err(e) => {
            tracing::error!(erreur = %e, "création de compte d'exploitation impossible");
            HttpResponse::ServiceUnavailable().json(ErreurValidationDto {
                code: "SERVICE_UNAVAILABLE".to_string(),
            })
        }
    }
}

#[derive(Deserialize)]
pub struct FiltreJournal {
    /// Restreindre à un acteur.
    acteur: Option<Uuid>,
    /// Page, à partir de zéro.
    page: Option<i64>,
}

/// Lit le journal d'exploitation.
#[utoipa::path(
    get,
    path = "/api/v1/ops/audit",
    tag = "exploitation",
    responses(
        (status = 200, description = "Une page du journal", body = JournalOpsDto),
        (status = 401, description = "Identifiants refusés", body = ErreurValidationDto),
        (status = 403, description = "Droit manquant", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    )
)]
#[get("/api/v1/ops/audit")]
pub async fn lire_audit(
    etat: web::Data<EtatApplication>,
    identifiants: web::Query<IdentifiantsOps>,
    filtre: web::Query<FiltreJournal>,
) -> HttpResponse {
    let demandeur = match authentifier(&etat, &identifiants).await {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    };

    // **La lecture du journal est elle-même journalisée.** Qui a consulté quoi
    // est précisément ce qu'un audit de sécurité vient chercher, et un journal
    // qui ne consigne pas ses propres lectures ne dit qu'une moitié de
    // l'histoire.
    if let Err(e) = autoriser_et_consigner(
        etat.ops.as_ref(),
        etat.horloge.as_ref(),
        demandeur.id,
        Permission::LireAudit,
        filtre.acteur.map(|a| a.to_string()).as_deref(),
    )
    .await
    {
        return HttpResponse::build(statut(&e)).json(ErreurValidationDto {
            code: e.code().to_string(),
        });
    }

    match lire_journal(etat.ops.as_ref(), filtre.acteur, filtre.page.unwrap_or(0)).await {
        Ok(gestes) => HttpResponse::Ok().json(JournalOpsDto {
            gestes: gestes
                .into_iter()
                .map(|g| GesteOpsDto {
                    acteur: g.ops_id.map(|a| a.to_string()),
                    geste: g.geste,
                    cible: g.cible,
                    fait_le: g.fait_le.to_rfc3339(),
                })
                .collect(),
            par_page: JOURNAL_PAR_PAGE,
        }),
        Err(e) => HttpResponse::build(statut(&e)).json(ErreurValidationDto {
            code: e.code().to_string(),
        }),
    }
}

/// Le chemin commun : mot de passe et code, à chaque requête.
async fn authentifier(
    etat: &EtatApplication,
    identifiants: &IdentifiantsOps,
) -> Result<CompteOps, ErreurOps> {
    let (Ok(email), Ok(mot_de_passe)) = (
        Email::parse(&identifiants.email),
        MotDePasse::parse(&identifiants.mot_de_passe),
    ) else {
        // Une adresse ou un mot de passe mal formés donnent le même refus qu'un
        // mauvais couple : le contraire dirait à qui essaie quelle forme est
        // attendue.
        return Err(ErreurOps::Refuse);
    };

    connecter(
        etat.ops.as_ref(),
        etat.horloge.as_ref(),
        &email,
        &mot_de_passe,
        &identifiants.code,
        etat.argon2,
    )
    .await
    .map(|s| s.compte)
}
