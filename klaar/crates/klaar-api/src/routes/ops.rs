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

use klaar_application::ports::export_repository::ExportRepository;
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

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportRgpdDto {
    pub utilisateur_id: String,
    pub code: &'static str,
    /// Toutes les données du compte, table par table.
    ///
    /// **Rendu en clair sur cette route.** FR-039 `@security` exige un
    /// chiffrement PGP avant tout envoi à une autorité ; il demande un trousseau
    /// que ce déploiement n'a pas. La route est donc réservée à un rôle
    /// d'exploitation, journalisée, et destinée à un usage interne : l'envoi
    /// chiffré viendra avec le trousseau. La limite est écrite plutôt que
    /// découverte.
    pub donnees: serde_json::Value,
}

#[derive(Deserialize)]
pub struct PeriodeExport {
    /// Début inclus, en RFC 3339.
    debut: String,
    /// Fin exclue, en RFC 3339.
    fin: String,
}

/// Exporte toutes les données d'un compte (RGPD art. 15, FR-039).
#[utoipa::path(
    get,
    path = "/api/v1/ops/exports/gdpr",
    tag = "exploitation",
    responses(
        (status = 200, description = "Les données du compte", body = ExportRgpdDto),
        (status = 401, description = "Identifiants refusés", body = ErreurValidationDto),
        (status = 403, description = "Droit manquant", body = ErreurValidationDto),
        (status = 404, description = "Compte inconnu", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    )
)]
#[get("/api/v1/ops/exports/gdpr")]
pub async fn export_rgpd(
    etat: web::Data<EtatApplication>,
    identifiants: web::Query<IdentifiantsOps>,
    cible: web::Query<CibleExport>,
) -> HttpResponse {
    let demandeur = match authentifier(&etat, &identifiants).await {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    };

    // **L'export est journalisé avant d'être produit** (FR-039 `@security`) :
    // sortir les données de quelqu'un est le geste le plus lourd de cette
    // console, et un export dont la trace n'a pas pu s'écrire ne doit pas
    // avoir lieu.
    if let Err(e) = autoriser_et_consigner(
        etat.ops.as_ref(),
        etat.horloge.as_ref(),
        demandeur.id,
        Permission::ExporterAudit,
        Some(&cible.utilisateur.to_string()),
    )
    .await
    {
        return HttpResponse::build(statut(&e)).json(ErreurValidationDto {
            code: e.code().to_string(),
        });
    }

    match etat.exports.donnees_personnelles(cible.utilisateur).await {
        Ok(Some(donnees)) => HttpResponse::Ok().json(ExportRgpdDto {
            utilisateur_id: cible.utilisateur.to_string(),
            code: "GDPR_EXPORT",
            donnees,
        }),
        // Un export vide et un export d'inexistant ne veulent pas dire la même
        // chose : une autorité qui reçoit le premier alors que c'était le
        // second en tirera la mauvaise conclusion.
        Ok(None) => HttpResponse::NotFound().json(ErreurValidationDto {
            code: "USER_NOT_FOUND".to_string(),
        }),
        Err(e) => {
            tracing::error!(erreur = %e, "export RGPD impossible");
            HttpResponse::ServiceUnavailable().json(ErreurValidationDto {
                code: "SERVICE_UNAVAILABLE".to_string(),
            })
        }
    }
}

#[derive(Deserialize)]
pub struct CibleExport {
    utilisateur: Uuid,
}

/// Exporte les lignes de TVA d'une période, en CSV (FR-039).
#[utoipa::path(
    get,
    path = "/api/v1/ops/exports/vat",
    tag = "exploitation",
    responses(
        (status = 200, description = "CSV des lignes de TVA", content_type = "text/csv"),
        (status = 401, description = "Identifiants refusés", body = ErreurValidationDto),
        (status = 403, description = "Droit manquant", body = ErreurValidationDto),
        (status = 422, description = "Période incohérente", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    )
)]
#[get("/api/v1/ops/exports/vat")]
pub async fn export_tva(
    etat: web::Data<EtatApplication>,
    identifiants: web::Query<IdentifiantsOps>,
    periode: web::Query<PeriodeExport>,
) -> HttpResponse {
    let demandeur = match authentifier(&etat, &identifiants).await {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    };

    let (Ok(debut), Ok(fin)) = (
        chrono::DateTime::parse_from_rfc3339(&periode.debut),
        chrono::DateTime::parse_from_rfc3339(&periode.fin),
    ) else {
        return HttpResponse::UnprocessableEntity().json(ErreurValidationDto {
            code: "PERIOD_INVALID".to_string(),
        });
    };
    // FR-039 `@negative` : une période à l'envers est une erreur de saisie, pas
    // un export vide. Le dire évite de conclure « aucune activité » d'un
    // intervalle impossible.
    if debut >= fin {
        return HttpResponse::UnprocessableEntity().json(ErreurValidationDto {
            code: "PERIOD_INVALID".to_string(),
        });
    }

    if let Err(e) = autoriser_et_consigner(
        etat.ops.as_ref(),
        etat.horloge.as_ref(),
        demandeur.id,
        Permission::ExporterAudit,
        Some(&format!("{}..{}", periode.debut, periode.fin)),
    )
    .await
    {
        return HttpResponse::build(statut(&e)).json(ErreurValidationDto {
            code: e.code().to_string(),
        });
    }

    match etat
        .exports
        .lignes_tva(
            debut.with_timezone(&chrono::Utc),
            fin.with_timezone(&chrono::Utc),
        )
        .await
    {
        Ok(lignes) => {
            // Tous les montants en centimes, y compris dans le CSV : un tableur
            // qui relit « 217,80 » selon sa locale produit tantôt 217,8 tantôt
            // 21780, et personne ne s'en aperçoit avant le contrôle.
            let mut csv = String::from(
                "devis_id;decidee_le;taux_tva_bp;montant_htva_cents;tva_cents;                 total_ttc_cents;commission_htva_cents;tva_commission_cents
",
            );
            for l in lignes {
                csv.push_str(&format!(
                    "{};{};{};{};{};{};{};{}
",
                    l.devis_id,
                    l.decidee_le.to_rfc3339(),
                    l.taux_tva_bp,
                    l.montant_htva_cents,
                    l.tva_cents,
                    l.total_ttc_cents,
                    l.commission_htva_cents,
                    l.tva_commission_cents
                ));
            }
            HttpResponse::Ok()
                .content_type("text/csv; charset=utf-8")
                .body(csv)
        }
        Err(e) => {
            tracing::error!(erreur = %e, "export TVA impossible");
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
