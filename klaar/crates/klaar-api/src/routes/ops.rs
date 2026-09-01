//! Console d'exploitation : connexion, journal, gestion des comptes
//! (Story 8.4, FR-041, FR-042).
//!
//! **Un espace de noms à part, `/api/v1/ops`.** Ce n'est pas cosmétique : cela
//! permet de couper toute la console derrière un pare-feu ou un VPN d'un seul
//! préfixe, sans démêler des routes mélangées à celles du public.
//!
//! **Une session courte, et rien de plus long.** Les identifiants et le code
//! TOTP ne voyagent qu'une fois, au `POST /ops/login`, dans un corps de
//! requête ; la suite se fait au jeton porteur. C'est un changement par rapport
//! au premier état de cette console, où chaque requête les reprenait en
//! **paramètres d'URL** : tenable pour un appel en ligne de commande, intenable
//! pour un navigateur, où un mot de passe finit alors dans la barre d'adresse,
//! l'historique, l'en-tête `Referer` et les journaux d'accès du serveur.
//!
//! **Trente minutes, sans prolongation.** Un jeton d'exploitation volé donne
//! accès aux Demandes, aux litiges et aux montants de tout le monde ; sa durée
//! de vie est donc la variable à tenir courte. Repasser par le code TOTP toutes
//! les demi-heures est le prix de ces droits-là.

use actix_web::{get, post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use klaar_application::ports::export_repository::ExportRepository;
use klaar_application::ports::ops_repository::OpsRepository;
use klaar_application::usecases::administrer_catalogue::{
    creer as creer_catalogue, desactiver as desactiver_catalogue, lister as lister_catalogue,
    publier as publier_catalogue, ErreurCatalogue,
};
use klaar_application::usecases::mediation::{
    dossier as lire_dossier, file as file_mediation, trancher as trancher_litige, ErreurMediation,
    VueDossier,
};
use klaar_application::usecases::ops::{
    autoriser_et_consigner, compte_de_session, connecter, fermer_session, lire_journal,
    ouvrir_session, secret_totp_neuf, ErreurOps, JOURNAL_PAR_PAGE,
};
use klaar_application::usecases::revue_kyc::{decider, file as file_revue, ErreurRevue};
use klaar_application::usecases::tableau_bord::{tableau_de_bord, FENETRE_JOURS};
use klaar_catalog::{CodeCatalogue, Libelles, SecteurACreer};
use klaar_identity::{CompteOps, DecisionKyc, MotDePasse, Permission};
use klaar_shared_kernel::Email;
use klaar_trust::Decision;

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
    /// Jeton de session, à présenter en `Authorization: Bearer`.
    ///
    /// **Rendu une seule fois, et jamais reconservé.** Seule son empreinte est
    /// écrite ; le service est incapable de le redonner, ce qui est le
    /// comportement attendu d'un secret de session.
    pub jeton: String,
    /// Échéance, en RFC 3339. Trente minutes, sans prolongation.
    pub expire_le: String,
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

/// Identifiants d'exploitation, dans le **corps** du `POST /ops/login`.
///
/// **Ils ne voyagent qu'une fois.** Les faire circuler à chaque requête, comme
/// c'était le cas en paramètres d'URL, mettrait un mot de passe dans la barre
/// d'adresse, l'historique du navigateur, l'en-tête `Referer` et les journaux
/// d'accès du serveur. Le jeton de session rendu ici les remplace pour la suite.
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
        (status = 400, description = "Corps illisible ou champ inconnu", body = ErreurValidationDto),
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
        Ok(compte) => {
            match ouvrir_session(etat.ops.as_ref(), etat.horloge.as_ref(), compte).await {
                Ok(session) => HttpResponse::Ok().json(SessionOpsDto {
                    id: session.compte.id.to_string(),
                    role: session.compte.role.as_str().to_string(),
                    code: "OPS_AUTHENTICATED",
                    jeton: session.jeton,
                    expire_le: session.expire_le.to_rfc3339(),
                }),
                Err(e) => {
                    tracing::error!(erreur = %e, "ouverture de session d'exploitation impossible");
                    HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                        code: e.code().to_string(),
                    })
                }
            }
        }
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
        (status = 400, description = "Corps illisible ou champ inconnu", body = ErreurValidationDto),
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
    requete: actix_web::HttpRequest,
    corps: web::Json<CreationOpsDto>,
) -> HttpResponse {
    let demandeur = match porteur(&etat, &requete).await {
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
    // **Le paramètre était exigé par le code et absent du contrat.** Un client
    // engendré depuis l'OpenAPI n'avait aucun moyen de savoir qu'il fallait
    // l'envoyer, et le fuzz de contrat recevait un 400 non documenté pour une
    // requête que le schéma déclarait pourtant complète.
    params(("utilisateur" = Uuid, Query, description = "Identifiant du compte à exporter")),
    responses(
        (status = 200, description = "Les données du compte", body = ExportRgpdDto),
        (status = 400, description = "Paramètre absent ou illisible", body = ErreurValidationDto),
        (status = 401, description = "Identifiants refusés", body = ErreurValidationDto),
        (status = 403, description = "Droit manquant", body = ErreurValidationDto),
        (status = 404, description = "Compte inconnu", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    )
)]
#[get("/api/v1/ops/exports/gdpr")]
pub async fn export_rgpd(
    etat: web::Data<EtatApplication>,
    requete: actix_web::HttpRequest,
    cible: web::Query<CibleExport>,
) -> HttpResponse {
    let demandeur = match porteur(&etat, &requete).await {
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
    // Mêmes paramètres exigés et non documentés que pour l'export RGPD.
    params(
        ("debut" = String, Query, description = "Début inclus, en RFC 3339"),
        ("fin" = String, Query, description = "Fin exclue, en RFC 3339"),
    ),
    responses(
        (status = 200, description = "CSV des lignes de TVA", content_type = "text/csv"),
        (status = 400, description = "Paramètre absent ou illisible", body = ErreurValidationDto),
        (status = 401, description = "Identifiants refusés", body = ErreurValidationDto),
        (status = 403, description = "Droit manquant", body = ErreurValidationDto),
        (status = 422, description = "Période incohérente", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    )
)]
#[get("/api/v1/ops/exports/vat")]
pub async fn export_tva(
    etat: web::Data<EtatApplication>,
    requete: actix_web::HttpRequest,
    periode: web::Query<PeriodeExport>,
) -> HttpResponse {
    let demandeur = match porteur(&etat, &requete).await {
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
    requete: actix_web::HttpRequest,
    filtre: web::Query<FiltreJournal>,
) -> HttpResponse {
    let demandeur = match porteur(&etat, &requete).await {
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

/// Les indicateurs, tels que la console les affiche (FR-040).
///
/// **Chaque taux voyage avec son assiette.** « 60 % » sur trois Demandes se lit
/// autrement que sur trois mille, et un tableau de bord qui ne rend que le taux
/// fait décider sur du bruit. Les deux nombres sont là, et le taux est `null`
/// quand il n'y a rien à mesurer.
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TableauBordDto {
    /// Début de la fenêtre observée, en RFC 3339.
    pub depuis: String,
    pub fenetre_jours: i64,
    /// Comptes ayant ouvert une session sur la fenêtre (MAU).
    pub comptes_actifs: i64,
    pub demandes: i64,
    pub demandes_attribuees: i64,
    /// `demandes_attribuees / demandes`. `null` à J0 : l'absence de Demande
    /// n'est pas un taux de remplissage nul.
    pub taux_remplissage: Option<f64>,
    pub gmv_htva_cents: i64,
    pub commission_htva_cents: i64,
    pub litiges_ouverts: i64,
    pub notes: i64,
    /// Moyenne sur cinq. `null` tant que personne n'a noté.
    ///
    /// **Ce n'est pas le NPS que demande FR-040.** Le NPS suppose la question
    /// « recommanderiez-vous », que le produit ne pose pas : la calculer à
    /// partir de notes sur cinq serait inventer une mesure et lui donner le nom
    /// d'une autre.
    pub note_moyenne: Option<f64>,
    /// Sorties de zone consignées sur la fenêtre (FR-018).
    pub sorties_de_zone: i64,
    /// Contrôles d'entreprise en attente (FR-038).
    pub kyc_en_attente: i64,
}

/// Lit les indicateurs d'exploitation.
#[utoipa::path(
    get,
    path = "/api/v1/ops/dashboard",
    tag = "exploitation",
    responses(
        (status = 200, description = "Les indicateurs de la fenêtre", body = TableauBordDto),
        (status = 401, description = "Identifiants refusés", body = ErreurValidationDto),
        (status = 403, description = "Droit manquant", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    )
)]
#[get("/api/v1/ops/dashboard")]
pub async fn lire_tableau_bord(
    etat: web::Data<EtatApplication>,
    requete: actix_web::HttpRequest,
) -> HttpResponse {
    let demandeur = match porteur(&etat, &requete).await {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    };

    // **La consultation est journalisée**, comme celle du journal lui-même
    // (FR-040 `@security`). Sans cible : le tableau ne porte sur personne, et
    // inventer une cible pour remplir la colonne rendrait le journal moins
    // lisible, pas plus.
    if let Err(e) = autoriser_et_consigner(
        etat.ops.as_ref(),
        etat.horloge.as_ref(),
        demandeur.id,
        Permission::LireTableauBord,
        None,
    )
    .await
    {
        return HttpResponse::build(statut(&e)).json(ErreurValidationDto {
            code: e.code().to_string(),
        });
    }

    match tableau_de_bord(etat.tableau_bord.as_ref(), etat.horloge.as_ref()).await {
        Ok(vue) => HttpResponse::Ok().json(TableauBordDto {
            depuis: vue.depuis.to_rfc3339(),
            fenetre_jours: FENETRE_JOURS,
            comptes_actifs: vue.indicateurs.comptes_actifs,
            demandes: vue.indicateurs.demandes,
            demandes_attribuees: vue.indicateurs.demandes_attribuees,
            taux_remplissage: vue.taux_remplissage,
            gmv_htva_cents: vue.indicateurs.gmv_htva_cents,
            commission_htva_cents: vue.indicateurs.commission_htva_cents,
            litiges_ouverts: vue.indicateurs.litiges_ouverts,
            notes: vue.indicateurs.notes,
            note_moyenne: vue.note_moyenne,
            sorties_de_zone: vue.indicateurs.sorties_de_zone,
            kyc_en_attente: vue.indicateurs.kyc_en_attente,
        }),
        Err(e) => {
            tracing::error!(erreur = %e, "tableau de bord indisponible");
            HttpResponse::ServiceUnavailable().json(ErreurValidationDto {
                code: "SERVICE_UNAVAILABLE".to_string(),
            })
        }
    }
}

/// Un litige, tel que la console de médiation le voit (FR-036).
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DossierLitigeDto {
    pub id: String,
    pub mission_id: String,
    /// `USER` ou `PROVIDER` : qui a ouvert.
    pub partie: String,
    pub motif: String,
    pub description: String,
    pub ouvert_le: String,
    pub age_jours: i64,
    /// Ouvert depuis plus de trente jours (FR-036 `@edge`).
    ///
    /// Calculé par le service et non par l'écran : deux calculs du même seuil
    /// finissent par diverger, et c'est l'alerte qui se tairait.
    pub a_escalader: bool,
    /// Montant du devis convenu, en centimes. Zéro si l'intervention n'en avait
    /// pas — un litige peut naître d'un travail jamais commencé.
    pub total_ttc_cents: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FileMediationDto {
    pub dossiers: Vec<DossierLitigeDto>,
}

/// La décision d'un médiateur.
/// Les quatre issues qu'une médiation peut prononcer.
///
/// **Déclarée au contrat, lue comme une chaîne.** Le champ reste un `String`
/// côté serveur : une valeur inconnue doit rendre un 422 « décision invalide »
/// et non un 400 « corps illisible », parce que la requête est bien formée et
/// que c'est la décision qui n'existe pas. Mais le contrat, lui, doit dire
/// lesquelles existent — sans quoi un client engendré depuis l'OpenAPI accepte
/// n'importe quelle chaîne, et le fuzz en essaie.
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IssueAdmise {
    UserFavor,
    ProviderFavor,
    PartialRefund,
    NoFault,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DecisionDto {
    /// `USER_FAVOR`, `PROVIDER_FAVOR`, `PARTIAL_REFUND` ou `NO_FAULT`.
    #[schema(value_type = IssueAdmise)]
    pub decision: String,
    /// Part remboursée, en points de base. Exigée pour `PARTIAL_REFUND`, et
    /// refusée pour les autres : un taux sur une décision qui n'en prend pas
    /// laisserait croire qu'il a été appliqué.
    ///
    /// La borne est déclarée au contrat : dix mille points de base font cent
    /// pour cent, et au-delà de soixante-cinq mille la lecture du corps échouait
    /// avant même d'arriver à la règle métier — un refus juste, mais rendu pour
    /// la mauvaise raison et sous le mauvais code.
    #[schema(minimum = 0, maximum = 10000)]
    pub part_bp: Option<u16>,
}

/// Ce qu'une décision produit.
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct IssueDto {
    pub statut: String,
    pub remboursement_cents: i64,
    pub reversement_cents: i64,
    /// **Aucun mouvement d'argent n'a lieu.** Le séquestre est chez Stripe, qui
    /// n'est pas provisionné (Epic 5) : la décision est écrite et le montant
    /// reste dû. Le dire ici évite d'annoncer un remboursement qui n'arrivera
    /// pas.
    pub execute: bool,
}

fn dossier_dto(v: VueDossier) -> DossierLitigeDto {
    DossierLitigeDto {
        id: v.dossier.id.to_string(),
        mission_id: v.dossier.mission_id.to_string(),
        partie: v.dossier.partie.as_str().to_string(),
        motif: v.dossier.motif.as_str().to_string(),
        description: v.dossier.description,
        ouvert_le: v.dossier.ouvert_le.to_rfc3339(),
        age_jours: v.age_jours,
        a_escalader: v.a_escalader,
        total_ttc_cents: v.dossier.total_ttc_cents,
    }
}

fn statut_mediation(e: &ErreurMediation) -> actix_web::http::StatusCode {
    use actix_web::http::StatusCode;
    match e {
        ErreurMediation::Introuvable => StatusCode::NOT_FOUND,
        ErreurMediation::Domaine(d) => match d.code() {
            // 409 : l'affaire est réglée, ce n'est pas une saisie invalide.
            "DISPUTE_ALREADY_RESOLVED" => StatusCode::CONFLICT,
            _ => StatusCode::UNPROCESSABLE_ENTITY,
        },
        ErreurMediation::Ops(o) => statut(o),
        ErreurMediation::Indisponible(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn echec_mediation(e: ErreurMediation) -> HttpResponse {
    if matches!(e, ErreurMediation::Indisponible(_)) {
        tracing::error!(erreur = %e, "médiation impossible");
    }
    HttpResponse::build(statut_mediation(&e)).json(ErreurValidationDto {
        code: e.code().to_string(),
    })
}

/// La file des litiges à trancher.
#[utoipa::path(
    get,
    path = "/api/v1/ops/disputes",
    tag = "exploitation",
    responses(
        (status = 200, description = "Les litiges ouverts, du plus ancien au plus récent", body = FileMediationDto),
        (status = 401, description = "Jeton absent ou expiré", body = ErreurValidationDto),
        (status = 403, description = "Droit manquant", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    )
)]
#[get("/api/v1/ops/disputes")]
pub async fn file_litiges(
    etat: web::Data<EtatApplication>,
    requete: actix_web::HttpRequest,
) -> HttpResponse {
    let demandeur = match porteur(&etat, &requete).await {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    };

    match file_mediation(
        etat.litiges.as_ref(),
        etat.ops.as_ref(),
        etat.horloge.as_ref(),
        demandeur.id,
    )
    .await
    {
        Ok(dossiers) => HttpResponse::Ok().json(FileMediationDto {
            dossiers: dossiers.into_iter().map(dossier_dto).collect(),
        }),
        Err(e) => echec_mediation(e),
    }
}

/// Un dossier de médiation.
#[utoipa::path(
    get,
    path = "/api/v1/ops/disputes/{id}",
    tag = "exploitation",
    params(("id" = Uuid, Path, description = "Identifiant du litige")),
    responses(
        (status = 200, description = "Le dossier", body = DossierLitigeDto),
        (status = 400, description = "Identifiant illisible", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou expiré", body = ErreurValidationDto),
        (status = 403, description = "Droit manquant", body = ErreurValidationDto),
        (status = 404, description = "Litige inconnu", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    )
)]
#[get("/api/v1/ops/disputes/{id}")]
pub async fn lire_litige(
    etat: web::Data<EtatApplication>,
    requete: actix_web::HttpRequest,
    chemin: web::Path<String>,
) -> HttpResponse {
    let demandeur = match porteur(&etat, &requete).await {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    };
    let Ok(litige_id) = Uuid::parse_str(&chemin) else {
        return HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "DISPUTE_ID_INVALID".to_string(),
        });
    };

    match lire_dossier(
        etat.litiges.as_ref(),
        etat.ops.as_ref(),
        etat.horloge.as_ref(),
        demandeur.id,
        litige_id,
    )
    .await
    {
        Ok(d) => HttpResponse::Ok().json(dossier_dto(d)),
        Err(e) => echec_mediation(e),
    }
}

/// Tranche un litige.
#[utoipa::path(
    post,
    path = "/api/v1/ops/disputes/{id}/resolve",
    tag = "exploitation",
    params(("id" = Uuid, Path, description = "Identifiant du litige")),
    request_body = DecisionDto,
    responses(
        (status = 200, description = "Décision enregistrée", body = IssueDto),
        (status = 400, description = "Identifiant illisible", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou expiré", body = ErreurValidationDto),
        (status = 403, description = "Droit manquant", body = ErreurValidationDto),
        (status = 404, description = "Litige inconnu", body = ErreurValidationDto),
        (status = 409, description = "Déjà tranché", body = ErreurValidationDto),
        (status = 422, description = "Décision inconnue, ou part hors bornes", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    )
)]
#[post("/api/v1/ops/disputes/{id}/resolve")]
pub async fn trancher_route(
    etat: web::Data<EtatApplication>,
    requete: actix_web::HttpRequest,
    chemin: web::Path<String>,
    corps: web::Json<DecisionDto>,
) -> HttpResponse {
    let demandeur = match porteur(&etat, &requete).await {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    };
    let Ok(litige_id) = Uuid::parse_str(&chemin) else {
        return HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "DISPUTE_ID_INVALID".to_string(),
        });
    };

    let decision = match (corps.decision.as_str(), corps.part_bp) {
        ("USER_FAVOR", None) => Decision::PourLeDemandeur,
        ("PROVIDER_FAVOR", None) => Decision::PourLePrestataire,
        ("NO_FAULT", None) => Decision::SansFaute,
        ("PARTIAL_REFUND", Some(part_bp)) => Decision::Partiel { part_bp },
        // **Un taux sur une décision qui n'en prend pas est refusé, pas
        // ignoré.** L'ignorer laisserait croire qu'il a été appliqué, et
        // quelqu'un compterait sur un remboursement partiel qui n'a pas eu lieu.
        _ => {
            return HttpResponse::UnprocessableEntity().json(ErreurValidationDto {
                code: "DECISION_INVALID".to_string(),
            })
        }
    };

    match trancher_litige(
        etat.litiges.as_ref(),
        etat.ops.as_ref(),
        etat.horloge.as_ref(),
        demandeur.id,
        litige_id,
        decision,
    )
    .await
    {
        Ok(issue) => HttpResponse::Ok().json(IssueDto {
            statut: issue.statut.as_str().to_string(),
            remboursement_cents: issue.remboursement_cents,
            reversement_cents: issue.reversement_cents,
            // Faux, et écrit comme tel : le séquestre n'est pas provisionné.
            execute: false,
        }),
        Err(e) => echec_mediation(e),
    }
}

/// Une entreprise en attente de contrôle (FR-038).
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DossierKycDto {
    pub provider_id: String,
    pub numero_bce: String,
    pub raison_sociale: String,
    pub secteurs: Vec<String>,
    pub inscrit_le: String,
    pub attente_jours: i64,
    /// En attente depuis plus de sept jours.
    pub attente_longue: bool,
    /// Un refus attend sa seconde paire d'yeux (FR-038 `@edge`).
    ///
    /// **Exposé pour que le second examinateur ne rédige pas un motif inutile.**
    /// Sans lui, il croirait le dossier intact et écrirait une raison qui ne
    /// serait jamais consignée.
    pub refus_en_attente: Option<RefusEnAttenteDto>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RefusEnAttenteDto {
    pub motif: String,
    pub propose_le: String,
    /// Vrai si c'est **vous** qui l'avez proposé : confirmer son propre refus
    /// n'est pas une seconde paire d'yeux.
    pub propose_par_moi: bool,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FileKycDto {
    pub dossiers: Vec<DossierKycDto>,
}

/// La décision d'un examinateur.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DecisionKycDto {
    /// `APPROVE` ou `REJECT`.
    pub decision: String,
    /// Exigé pour un refus, refusé pour une validation (FR-038 `@negative`).
    pub motif: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct IssueRevueDto {
    pub code: &'static str,
    /// Statut atteint, ou `null` tant que le refus attend sa confirmation.
    pub statut: Option<String>,
    pub attend_confirmation: bool,
    /// **Faux : aucun courriel n'est parti.** Le service de courriel est
    /// journalisé et non expédié tant qu'aucun fournisseur n'est provisionné.
    /// Le dire évite de laisser croire que l'entreprise a été prévenue.
    pub notifie: bool,
}

fn statut_revue(e: &ErreurRevue) -> actix_web::http::StatusCode {
    use actix_web::http::StatusCode;
    match e {
        ErreurRevue::Introuvable => StatusCode::NOT_FOUND,
        ErreurRevue::DejaProposee => StatusCode::CONFLICT,
        ErreurRevue::Domaine(d) => match d.code() {
            // 409 : l'entreprise s'est retirée, ou a déjà été traitée. Ce n'est
            // pas une saisie invalide.
            "PROVIDER_CANCELLED" | "REVIEW_ALREADY_DONE" => StatusCode::CONFLICT,
            // 400, comme FR-038 `@negative` le demande explicitement.
            "MOTIVE_REQUIRED" => StatusCode::BAD_REQUEST,
            "FOUR_EYES_REQUIRED" => StatusCode::FORBIDDEN,
            _ => StatusCode::UNPROCESSABLE_ENTITY,
        },
        ErreurRevue::Ops(o) => statut(o),
        ErreurRevue::Indisponible(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn echec_revue(e: ErreurRevue) -> HttpResponse {
    if matches!(e, ErreurRevue::Indisponible(_)) {
        tracing::error!(erreur = %e, "revue KYC impossible");
    }
    HttpResponse::build(statut_revue(&e)).json(ErreurValidationDto {
        code: e.code().to_string(),
    })
}

/// Les entreprises en attente de contrôle.
#[utoipa::path(
    get,
    path = "/api/v1/ops/kyc/pending",
    tag = "exploitation",
    responses(
        (status = 200, description = "Les dossiers en attente, du plus ancien au plus récent", body = FileKycDto),
        (status = 401, description = "Jeton absent ou expiré", body = ErreurValidationDto),
        (status = 403, description = "Droit manquant", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    )
)]
#[get("/api/v1/ops/kyc/pending")]
pub async fn file_kyc(
    etat: web::Data<EtatApplication>,
    requete: actix_web::HttpRequest,
) -> HttpResponse {
    let demandeur = match porteur(&etat, &requete).await {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    };

    match file_revue(
        etat.revues_kyc.as_ref(),
        etat.ops.as_ref(),
        etat.horloge.as_ref(),
        demandeur.id,
    )
    .await
    {
        Ok(dossiers) => HttpResponse::Ok().json(FileKycDto {
            dossiers: dossiers
                .into_iter()
                .map(|v| DossierKycDto {
                    provider_id: v.dossier.provider_id.to_string(),
                    numero_bce: v.dossier.numero_bce,
                    raison_sociale: v.dossier.raison_sociale,
                    secteurs: v.dossier.secteurs,
                    inscrit_le: v.dossier.inscrit_le.to_rfc3339(),
                    attente_jours: v.dossier.attente_jours,
                    attente_longue: v.attente_longue,
                    refus_en_attente: v.dossier.refus_en_attente.map(|r| RefusEnAttenteDto {
                        motif: r.motif,
                        propose_le: r.propose_le.to_rfc3339(),
                        propose_par_moi: r.propose_par == Some(demandeur.id),
                    }),
                })
                .collect(),
        }),
        Err(e) => echec_revue(e),
    }
}

/// Valide ou refuse une entreprise.
#[utoipa::path(
    post,
    path = "/api/v1/ops/kyc/{provider_id}/review",
    tag = "exploitation",
    params(("provider_id" = Uuid, Path, description = "Identifiant de l'entreprise")),
    request_body = DecisionKycDto,
    responses(
        (status = 200, description = "Décision enregistrée, ou refus en attente de confirmation", body = IssueRevueDto),
        (status = 400, description = "Refus sans motif", body = ErreurValidationDto),
        (status = 401, description = "Jeton absent ou expiré", body = ErreurValidationDto),
        (status = 403, description = "Droit manquant, ou refus à confirmer par un autre compte", body = ErreurValidationDto),
        (status = 404, description = "Entreprise inconnue", body = ErreurValidationDto),
        (status = 409, description = "Entreprise retirée ou déjà traitée", body = ErreurValidationDto),
        (status = 422, description = "Décision inconnue, ou motif hors propos", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    )
)]
#[post("/api/v1/ops/kyc/{provider_id}/review")]
pub async fn reviser_kyc(
    etat: web::Data<EtatApplication>,
    requete: actix_web::HttpRequest,
    chemin: web::Path<String>,
    corps: web::Json<DecisionKycDto>,
) -> HttpResponse {
    let demandeur = match porteur(&etat, &requete).await {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    };
    let Ok(provider_id) = Uuid::parse_str(&chemin) else {
        return HttpResponse::BadRequest().json(ErreurValidationDto {
            code: "PROVIDER_ID_INVALID".to_string(),
        });
    };
    let Some(decision) = DecisionKyc::parse(&corps.decision) else {
        return HttpResponse::UnprocessableEntity().json(ErreurValidationDto {
            code: "DECISION_INVALID".to_string(),
        });
    };

    match decider(
        etat.revues_kyc.as_ref(),
        etat.ops.as_ref(),
        etat.horloge.as_ref(),
        demandeur.id,
        provider_id,
        decision,
        corps.motif.as_deref(),
    )
    .await
    {
        Ok(issue) => HttpResponse::Ok().json(IssueRevueDto {
            code: if issue.attend_confirmation {
                "REVIEW_PENDING_CONFIRMATION"
            } else {
                "REVIEW_RECORDED"
            },
            statut: issue.statut.map(|s| s.as_str().to_string()),
            attend_confirmation: issue.attend_confirmation,
            notifie: issue.notifie,
        }),
        Err(e) => echec_revue(e),
    }
}

/// Un secteur, vu par l'exploitation (FR-010).
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SecteurAdminDto {
    pub code: String,
    pub libelle_fr: String,
    pub libelle_nl: String,
    pub libelle_en: String,
    pub ordre: i32,
    /// `DRAFT`, `PUBLISHED` ou `DISABLED`.
    pub statut: String,
    /// Vrai si c'est **vous** qui l'avez créé : un brouillon se publie par un
    /// autre compte, et l'écran doit pouvoir le dire avant le clic.
    pub cree_par_moi: bool,
    /// Interventions en cours dans ce secteur. Non nul, il empêche le retrait.
    pub missions_en_cours: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogueAdminDto {
    pub secteurs: Vec<SecteurAdminDto>,
}

/// Un secteur à créer.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreationSecteurDto {
    pub code: String,
    /// Les **trois** libellés, exigés dès la création : un secteur publié sans
    /// néerlandais s'afficherait en français à un néerlandophone.
    pub libelle_fr: String,
    pub libelle_nl: String,
    pub libelle_en: String,
    pub ordre: i32,
}

fn statut_catalogue(e: &ErreurCatalogue) -> actix_web::http::StatusCode {
    use actix_web::http::StatusCode;
    match e {
        ErreurCatalogue::Introuvable => StatusCode::NOT_FOUND,
        // 409 sur les deux : FR-010 `@negative` pour le doublon, `@edge` pour
        // les interventions en cours. Ce ne sont pas des saisies invalides,
        // c'est un état du monde.
        ErreurCatalogue::DejaFait => StatusCode::CONFLICT,
        ErreurCatalogue::Domaine(d) => match d.code() {
            "SECTOR_CODE_TAKEN" | "SECTOR_HAS_ACTIVE_MISSIONS" => StatusCode::CONFLICT,
            "FOUR_EYES_REQUIRED" => StatusCode::FORBIDDEN,
            _ => StatusCode::UNPROCESSABLE_ENTITY,
        },
        ErreurCatalogue::Ops(o) => statut(o),
        ErreurCatalogue::Indisponible(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn echec_catalogue(e: ErreurCatalogue) -> HttpResponse {
    if matches!(e, ErreurCatalogue::Indisponible(_)) {
        tracing::error!(erreur = %e, "administration du catalogue impossible");
    }
    HttpResponse::build(statut_catalogue(&e)).json(ErreurValidationDto {
        code: e.code().to_string(),
    })
}

/// Le catalogue entier, brouillons compris.
#[utoipa::path(
    get,
    path = "/api/v1/ops/catalog/sectors",
    tag = "exploitation",
    responses(
        (status = 200, description = "Tous les secteurs", body = CatalogueAdminDto),
        (status = 401, description = "Jeton absent ou expiré", body = ErreurValidationDto),
        (status = 403, description = "Droit manquant", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    )
)]
#[get("/api/v1/ops/catalog/sectors")]
pub async fn lister_secteurs(
    etat: web::Data<EtatApplication>,
    requete: actix_web::HttpRequest,
) -> HttpResponse {
    let demandeur = match porteur(&etat, &requete).await {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    };

    match lister_catalogue(
        etat.catalogue_admin.as_ref(),
        etat.ops.as_ref(),
        etat.horloge.as_ref(),
        demandeur.id,
    )
    .await
    {
        Ok(secteurs) => HttpResponse::Ok().json(CatalogueAdminDto {
            secteurs: secteurs
                .into_iter()
                .map(|s| SecteurAdminDto {
                    code: s.code,
                    libelle_fr: s.libelle_fr,
                    libelle_nl: s.libelle_nl,
                    libelle_en: s.libelle_en,
                    ordre: s.ordre,
                    statut: s.statut.as_str().to_string(),
                    cree_par_moi: s.cree_par == Some(demandeur.id),
                    missions_en_cours: s.missions_en_cours,
                })
                .collect(),
        }),
        Err(e) => echec_catalogue(e),
    }
}

/// Crée un secteur, en brouillon.
#[utoipa::path(
    post,
    path = "/api/v1/ops/catalog/sectors",
    tag = "exploitation",
    request_body = CreationSecteurDto,
    responses(
        (status = 400, description = "Corps illisible ou champ inconnu", body = ErreurValidationDto),
        (status = 201, description = "Secteur créé en brouillon"),
        (status = 401, description = "Jeton absent ou expiré", body = ErreurValidationDto),
        (status = 403, description = "Droit manquant", body = ErreurValidationDto),
        (status = 409, description = "Code déjà pris", body = ErreurValidationDto),
        (status = 422, description = "Code invalide ou libellé manquant", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    )
)]
#[post("/api/v1/ops/catalog/sectors")]
pub async fn creer_secteur(
    etat: web::Data<EtatApplication>,
    requete: actix_web::HttpRequest,
    corps: web::Json<CreationSecteurDto>,
) -> HttpResponse {
    let demandeur = match porteur(&etat, &requete).await {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    };

    let Ok(code) = CodeCatalogue::parse(&corps.code) else {
        return HttpResponse::UnprocessableEntity().json(ErreurValidationDto {
            code: "SECTOR_CODE_INVALID".to_string(),
        });
    };

    let secteur = SecteurACreer {
        code,
        libelles: Libelles {
            fr: corps.libelle_fr.clone(),
            nl: corps.libelle_nl.clone(),
            en: corps.libelle_en.clone(),
        },
        ordre: corps.ordre,
    };

    match creer_catalogue(
        etat.catalogue_admin.as_ref(),
        etat.ops.as_ref(),
        etat.horloge.as_ref(),
        demandeur.id,
        secteur,
    )
    .await
    {
        Ok(()) => HttpResponse::Created().finish(),
        Err(e) => echec_catalogue(e),
    }
}

/// Publie un brouillon — par un autre compte que son créateur.
#[utoipa::path(
    post,
    path = "/api/v1/ops/catalog/sectors/{code}/publish",
    tag = "exploitation",
    params(("code" = String, Path, description = "Code du secteur")),
    responses(
        (status = 204, description = "Secteur publié"),
        (status = 401, description = "Jeton absent ou expiré", body = ErreurValidationDto),
        (status = 403, description = "Droit manquant, ou publication de son propre brouillon", body = ErreurValidationDto),
        (status = 404, description = "Secteur inconnu", body = ErreurValidationDto),
        (status = 409, description = "Déjà publié, ou publié entre-temps", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    )
)]
#[post("/api/v1/ops/catalog/sectors/{code}/publish")]
pub async fn publier_secteur(
    etat: web::Data<EtatApplication>,
    requete: actix_web::HttpRequest,
    chemin: web::Path<String>,
) -> HttpResponse {
    let demandeur = match porteur(&etat, &requete).await {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    };

    match publier_catalogue(
        etat.catalogue_admin.as_ref(),
        etat.ops.as_ref(),
        etat.horloge.as_ref(),
        demandeur.id,
        &chemin,
    )
    .await
    {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(e) => echec_catalogue(e),
    }
}

/// Retire un secteur du public.
#[utoipa::path(
    post,
    path = "/api/v1/ops/catalog/sectors/{code}/disable",
    tag = "exploitation",
    params(("code" = String, Path, description = "Code du secteur")),
    responses(
        (status = 204, description = "Secteur retiré"),
        (status = 401, description = "Jeton absent ou expiré", body = ErreurValidationDto),
        (status = 403, description = "Droit manquant", body = ErreurValidationDto),
        (status = 404, description = "Secteur inconnu", body = ErreurValidationDto),
        (status = 409, description = "Interventions en cours dans ce secteur", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    )
)]
#[post("/api/v1/ops/catalog/sectors/{code}/disable")]
pub async fn desactiver_secteur(
    etat: web::Data<EtatApplication>,
    requete: actix_web::HttpRequest,
    chemin: web::Path<String>,
) -> HttpResponse {
    let demandeur = match porteur(&etat, &requete).await {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    };

    match desactiver_catalogue(
        etat.catalogue_admin.as_ref(),
        etat.ops.as_ref(),
        etat.horloge.as_ref(),
        demandeur.id,
        &chemin,
    )
    .await
    {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(e) => echec_catalogue(e),
    }
}

/// Ferme la session d'exploitation.
#[utoipa::path(
    post,
    path = "/api/v1/ops/logout",
    tag = "exploitation",
    responses(
        (status = 204, description = "Session close, ou déjà close"),
        (status = 401, description = "Jeton absent", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    )
)]
#[post("/api/v1/ops/logout")]
pub async fn deconnexion_ops(
    etat: web::Data<EtatApplication>,
    requete: actix_web::HttpRequest,
) -> HttpResponse {
    let Some(jeton) = jeton_porteur(&requete) else {
        return HttpResponse::Unauthorized().json(ErreurValidationDto {
            code: "OPS_CREDENTIALS_INVALID".to_string(),
        });
    };

    // **Idempotent, et sans vérifier d'abord que la session vit.** Refermer une
    // session close est le résultat attendu, pas une erreur ; répondre 404
    // dirait à qui présente un jeton au hasard s'il en a trouvé un vrai.
    match fermer_session(etat.ops.as_ref(), etat.horloge.as_ref(), jeton).await {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(e) => {
            tracing::error!(erreur = %e, "fermeture de session d'exploitation impossible");
            HttpResponse::build(statut(&e)).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    }
}

/// Le jeton porteur, s'il y en a un de bien formé.
fn jeton_porteur(requete: &actix_web::HttpRequest) -> Option<&str> {
    requete
        .headers()
        .get(actix_web::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|j| !j.is_empty())
}

/// Le chemin commun de toutes les routes protégées : un jeton de session.
///
/// **Pas d'en-tête, jeton inconnu, expiré ou révoqué : le même refus.**
/// Distinguer ces cas apprendrait à qui essaie qu'il a mis la main sur quelque
/// chose de réel.
async fn porteur(
    etat: &EtatApplication,
    requete: &actix_web::HttpRequest,
) -> Result<CompteOps, ErreurOps> {
    let jeton = jeton_porteur(requete).ok_or(ErreurOps::Refuse)?;

    compte_de_session(etat.ops.as_ref(), etat.horloge.as_ref(), jeton).await
}

/// L'authentification complète, réservée à l'ouverture de session.
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
