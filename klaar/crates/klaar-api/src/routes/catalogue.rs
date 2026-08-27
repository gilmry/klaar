//! Lecture du catalogue (Story 2.2, FR-008).

use actix_web::http::header::{
    CacheControl, CacheDirective, ETag, EntityTag, Header, IfNoneMatch, RETRY_AFTER,
};
use actix_web::{get, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;

use klaar_application::ports::catalogue_repository::CatalogueRepository;
use klaar_application::ports::horloge::Horloge;
use klaar_shared_kernel::Locale;

use crate::limitation::{Quota, Verdict};
use crate::routes::auth::{adresse_source, ErreurValidationDto};
use crate::EtatApplication;

/// Durée de mise en cache, en secondes (FR-008 `@security`).
///
/// Cinq minutes : le catalogue change quelques fois par an, mais une durée plus
/// longue rendrait une correction de traduction invisible pendant des heures
/// chez les visiteurs qui l'ont déjà chargé. L'`ETag` rattrape le reste — une
/// revalidation ne coûte qu'un `304` de quelques octets.
pub const CACHE_SECONDES: u32 = 300;

/// Délai annoncé quand le catalogue est en maintenance (FR-008 `@edge`).
pub const RETRY_MAINTENANCE_SECONDES: i64 = 60;

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SkillDto {
    pub code: String,
    /// Libellé dans la langue servie.
    pub libelle: String,
}

/// Fourchette indicative, en centimes (FR-009).
///
/// En centimes et non en euros, comme tout montant traversant l'API : c'est au
/// client de choisir son format d'affichage, et un arrondi côté serveur ferait
/// diverger ce qui est montré de ce qui a été calculé.
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FourchetteDto {
    pub min_cents: i64,
    pub max_cents: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SecteurDto {
    pub code: String,
    pub libelle: String,
    /// Absente tant que l'historique ne permet pas d'en publier une. Absence
    /// veut dire « prix sur devis », pas « prix inconnu » (FR-009 `@negative`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fourchette: Option<FourchetteDto>,
    pub skills: Vec<SkillDto>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogueDto {
    /// Langue réellement servie, qui peut différer de celle demandée.
    pub locale: String,
    /// `LOCALE_FALLBACK` quand la langue demandée n'est pas prise en charge.
    ///
    /// Renvoyé dans le corps et non seulement journalisé, comme le demande
    /// FR-008 `@negative` : c'est au client d'apprendre qu'il n'aura pas la
    /// langue qu'il a réclamée, pas à l'exploitant de le découvrir dans ses
    /// journaux.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avertissement: Option<&'static str>,
    pub secteurs: Vec<SecteurDto>,
}

#[derive(Deserialize, ToSchema)]
pub struct ParametresCatalogue {
    /// `fr`, `nl` ou `en`. Toute autre valeur donne `fr`, avec un avertissement.
    pub locale: Option<String>,
}

/// Empreinte du contenu servi.
///
/// Calculée sur le corps rendu, et non sur une date de mise à jour : c'est le
/// contenu qui doit décider de la revalidation. Un horodatage changerait à
/// chaque redéploiement sans qu'une seule ligne du catalogue ait bougé, et
/// invaliderait tous les caches pour rien.
fn empreinte(corps: &[u8]) -> String {
    let condense = Sha256::digest(corps);
    condense
        .iter()
        .take(16)
        .map(|o| format!("{o:02x}"))
        .collect()
}

/// Liste les secteurs et leurs Skills.
#[utoipa::path(
    get,
    path = "/api/v1/catalog/sectors",
    tag = "catalogue",
    params(("locale" = Option<String>, Query, description = "fr, nl ou en ; repli sur fr")),
    responses(
        (status = 200, description = "Catalogue dans la langue servie", body = CatalogueDto),
        (status = 304, description = "Inchangé depuis l'ETag présenté"),
        (status = 429, description = "Trop de lectures depuis cette adresse", body = ErreurValidationDto),
        (status = 503, description = "Catalogue en maintenance", body = ErreurValidationDto),
    )
)]
#[get("/api/v1/catalog/sectors")]
pub async fn secteurs(
    requete: HttpRequest,
    etat: web::Data<EtatApplication>,
    parametres: web::Query<ParametresCatalogue>,
) -> HttpResponse {
    if etat.catalogue_en_maintenance {
        // 503 et non 500 : le service n'est pas en panne, il est
        // temporairement retiré. `Retry-After` dit quand revenir.
        return HttpResponse::ServiceUnavailable()
            .insert_header((RETRY_AFTER, RETRY_MAINTENANCE_SECONDES.to_string()))
            .json(ErreurValidationDto {
                code: "CATALOG_MAINTENANCE".to_string(),
            });
    }

    let maintenant = etat.horloge.maintenant();
    let source = format!(
        "catalogue:{}",
        adresse_source(&requete, etat.derriere_proxy)
    );
    if let Verdict::Refuse { retry_after } =
        etat.limiteur
            .verifier_quota(&source, maintenant, Quota::lecture_publique())
    {
        return HttpResponse::TooManyRequests()
            .insert_header((RETRY_AFTER, retry_after.to_string()))
            .json(ErreurValidationDto {
                code: "RATE_LIMIT_EXCEEDED".to_string(),
            });
    }

    let demandee = parametres.locale.as_deref();
    let (locale, avertissement) = match demandee {
        None => (Locale::Fr, None),
        Some(valeur) => match Locale::parse(valeur) {
            Ok(l) => (l, None),
            Err(_) => {
                tracing::warn!(
                    code = "LOCALE_FALLBACK",
                    demandee = valeur,
                    "langue non prise en charge, repli sur fr"
                );
                (Locale::Fr, Some("LOCALE_FALLBACK"))
            }
        },
    };

    let secteurs = match etat.catalogue.secteurs().await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(erreur = %e, "lecture du catalogue impossible");
            return HttpResponse::ServiceUnavailable().json(ErreurValidationDto {
                code: "SERVICE_UNAVAILABLE".to_string(),
            });
        }
    };

    if secteurs.is_empty() {
        // 200 avec une liste vide, pas une erreur : un catalogue non amorcé est
        // un état de démarrage légitime (FR-008 `@edge`). Le signaler dans les
        // journaux suffit à ce que l'exploitant s'en aperçoive.
        tracing::warn!(code = "CATALOG_EMPTY", "catalogue vide");
    }

    let corps = CatalogueDto {
        locale: locale.as_str().to_string(),
        avertissement,
        secteurs: secteurs
            .iter()
            .map(|s| SecteurDto {
                code: s.code.to_string(),
                libelle: s.libelles.pour(locale).to_string(),
                fourchette: s.fourchette.map(|f| FourchetteDto {
                    min_cents: f.min.cents(),
                    max_cents: f.max.cents(),
                }),
                skills: s
                    .skills
                    .iter()
                    .map(|k| SkillDto {
                        code: k.code.to_string(),
                        libelle: k.libelles.pour(locale).to_string(),
                    })
                    .collect(),
            })
            .collect(),
    };

    let serialise = match serde_json::to_vec(&corps) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(erreur = %e, "sérialisation du catalogue impossible");
            return HttpResponse::ServiceUnavailable().json(ErreurValidationDto {
                code: "SERVICE_UNAVAILABLE".to_string(),
            });
        }
    };
    let etiquette = EntityTag::new_strong(empreinte(&serialise));

    // `public` : le catalogue est le même pour tout le monde, aucun cache
    // intermédiaire ne risque de servir à Marie ce qui était destiné à Jan.
    // Les réponses personnalisées, elles, ne portent pas cet en-tête.
    let cache = CacheControl(vec![
        CacheDirective::Public,
        CacheDirective::MaxAge(CACHE_SECONDES),
    ]);

    if let Ok(IfNoneMatch::Items(presentees)) = IfNoneMatch::parse(&requete) {
        if presentees.iter().any(|e| e.weak_eq(&etiquette)) {
            // 304 sans corps : c'est tout l'intérêt de l'ETag. Le
            // `Cache-Control` est répété, sinon le client repartirait avec la
            // durée de la réponse précédente, qui a pu changer.
            return HttpResponse::NotModified()
                .insert_header(ETag(etiquette))
                .insert_header(cache)
                .finish();
        }
    }

    HttpResponse::Ok()
        .insert_header(ETag(etiquette))
        .insert_header(cache)
        .content_type("application/json")
        .body(serialise)
}
