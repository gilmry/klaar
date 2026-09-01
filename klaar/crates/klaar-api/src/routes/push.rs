//! Abonnements Web Push (Story 0.12, ADR-010).
//!
//! Trois endpoints seulement : donner au navigateur la clé publique dont il a
//! besoin, enregistrer l'abonnement qu'il produit, et le retirer. Le reste du
//! protocole est côté adaptateur.

use actix_web::{delete, get, post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use klaar_application::ports::push::PushSubscription;
use klaar_application::ports::push_repository::PushSubscriptionRepository;

use crate::EtatApplication;

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ClePubliqueDto {
    /// Clé publique VAPID, base64url, forme non compressée. C'est
    /// l'`applicationServerKey` attendue par `PushManager.subscribe`.
    pub cle: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ClesAbonnementDto {
    /// Clé publique P-256 non compressée, soit soixante-cinq octets en
    /// base64url : quatre-vingt-sept caractères, toujours.
    #[schema(min_length = 87, max_length = 88)]
    pub p256dh: String,
    /// Secret d'authentification, seize octets en base64url : vingt-deux
    /// caractères.
    #[schema(min_length = 22, max_length = 24)]
    pub auth: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AbonnementDto {
    /// Adresse fournie par le service de push du navigateur.
    ///
    /// **Les contraintes sont déclarées.** Le contrat annonçait « une chaîne »
    /// là où le code exige une URL et des clés de longueur fixe : un client
    /// engendré depuis l'OpenAPI ne validait donc rien, et le fuzz envoyait des
    /// chaînes vides que l'API refusait à juste titre — un faux échec qui
    /// masquait les vrais.
    #[schema(format = "uri", min_length = 8, max_length = 2048)]
    pub endpoint: String,
    pub keys: ClesAbonnementDto,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AbonnementEnregistreDto {
    pub id: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DesabonnementDto {
    #[schema(format = "uri", min_length = 8, max_length = 2048)]
    pub endpoint: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ErreurDto {
    /// Code stable, comme partout ailleurs dans ce contrat.
    ///
    /// **Ce champ s'appelait `erreur` et portait une phrase.** Les trois routes
    /// de push étaient les seules, sur cinquante-neuf, à ne pas rendre
    /// `{"code": "…"}` : un client devait donc écrire deux analyseurs
    /// d'erreurs, et le message était une phrase française non traduite,
    /// parfois recopiée d'une erreur interne. Le fuzz de contrat butait dessus
    /// pour la même raison qu'un client l'aurait fait.
    pub code: String,
}

/// Bornes de l'adresse d'abonnement, telles que le contrat les annonce.
///
/// **Déclarer une contrainte sans la vérifier est le même mensonge que ne pas
/// la déclarer, dans l'autre sens.** `#[schema(...)]` documente, il ne valide
/// rien : la borne haute avait été écrite au contrat et le serveur acceptait
/// toujours une adresse de quatre mille caractères. Le fuzz l'a trouvée, et il
/// avait raison — une adresse de cette taille n'a jamais été produite par un
/// service de push, elle finit seulement en ligne de base de données.
///
/// Les bornes sont ici, en un seul endroit, pour que le contrat et le contrôle
/// ne puissent plus diverger sans qu'on le voie.
const ADRESSE_MIN: usize = 8;
const ADRESSE_MAX: usize = 2048;

fn adresse_plausible(adresse: &str) -> bool {
    if !(ADRESSE_MIN..=ADRESSE_MAX).contains(&adresse.len()) {
        return false;
    }
    // **Un préfixe ne suffit pas.** `https://` fait huit caractères et passe
    // donc la borne basse tout en ne désignant rien : ce qui manque n'est pas
    // de la longueur, c'est un hôte. La borne du contrat dit le minimum
    // atteignable (`http://x`), elle ne peut pas dire cela.
    ["https://", "http://"].iter().any(|schema| {
        adresse
            .strip_prefix(schema)
            .is_some_and(|reste| !reste.is_empty())
    })
}

/// Clé publique VAPID à passer à `PushManager.subscribe`.
#[utoipa::path(
    get,
    path = "/api/v1/push/cle-publique",
    tag = "push",
    responses(
        (status = 200, description = "Clé publique VAPID", body = ClePubliqueDto),
        (status = 503, description = "Push non configuré sur ce déploiement", body = ErreurDto),
    )
)]
#[get("/api/v1/push/cle-publique")]
pub async fn cle_publique(etat: web::Data<EtatApplication>) -> HttpResponse {
    match &etat.push {
        Some(sender) => HttpResponse::Ok().json(ClePubliqueDto {
            cle: sender.cle_publique_base64url(),
        }),
        // 503 et non 500 : ce n'est pas une panne, c'est une fonctionnalité
        // non configurée. Le client doit pouvoir le distinguer pour masquer
        // l'invitation à activer les notifications plutôt qu'afficher une
        // erreur.
        None => HttpResponse::ServiceUnavailable().json(ErreurDto {
            code: "PUSH_NOT_CONFIGURED".to_string(),
        }),
    }
}

/// Enregistre l'abonnement produit par le navigateur.
#[utoipa::path(
    post,
    path = "/api/v1/push/abonnements",
    tag = "push",
    request_body = AbonnementDto,
    responses(
        (status = 201, description = "Abonnement enregistré", body = AbonnementEnregistreDto),
        (status = 400, description = "Corps illisible ou champ inconnu", body = ErreurDto),
        (status = 422, description = "Abonnement lisible mais inutilisable", body = ErreurDto),
        (status = 503, description = "Dépôt indisponible", body = ErreurDto),
    )
)]
#[post("/api/v1/push/abonnements")]
pub async fn enregistrer_abonnement(
    etat: web::Data<EtatApplication>,
    corps: web::Json<AbonnementDto>,
) -> HttpResponse {
    let abonnement = PushSubscription {
        endpoint: corps.endpoint.clone(),
        p256dh: corps.keys.p256dh.clone(),
        auth: corps.keys.auth.clone(),
    };

    // La même borne qu'à la suppression, et pour la même raison : ce que le
    // contrat annonce, le serveur l'exige.
    if !adresse_plausible(&corps.endpoint) {
        return HttpResponse::UnprocessableEntity().json(ErreurDto {
            code: "ENDPOINT_INVALID".to_string(),
        });
    }

    // Valider ici plutôt qu'au premier envoi : un abonnement mal formé accepté
    // aujourd'hui devient une notification perdue dans six semaines, sans
    // rien pour relier les deux.
    if let Err(e) = klaar_push_adapter::valider_abonnement(&abonnement) {
        // Le détail est journalisé, pas rendu : le message de validation
        // décrit la structure attendue, ce qui n'apprend rien d'utile à un
        // client légitime et guide qui essaie autre chose.
        // **422 et non 400.** Le corps est lisible et ses champs ont la bonne
        // forme ; c'est leur contenu qui n'est pas un abonnement utilisable —
        // une clé qui ne décode pas en point de courbe non compressé, par
        // exemple. « Je ne vous comprends pas » et « je vous comprends et ça
        // ne marchera pas » n'appellent pas la même correction côté client, et
        // aucun schéma JSON ne sait exprimer la seconde.
        tracing::warn!(erreur = %e, "abonnement push refusé");
        return HttpResponse::UnprocessableEntity().json(ErreurDto {
            code: "SUBSCRIPTION_INVALID".to_string(),
        });
    }

    match etat.abonnements.enregistrer(&abonnement, None).await {
        Ok(enregistre) => HttpResponse::Created().json(AbonnementEnregistreDto {
            id: enregistre.id.to_string(),
        }),
        Err(e) => {
            tracing::error!(erreur = %e, "enregistrement d'abonnement push impossible");
            HttpResponse::ServiceUnavailable().json(ErreurDto {
                code: "SERVICE_UNAVAILABLE".to_string(),
            })
        }
    }
}

/// Retire un abonnement.
#[utoipa::path(
    delete,
    path = "/api/v1/push/abonnements",
    tag = "push",
    request_body = DesabonnementDto,
    responses(
        (status = 204, description = "Abonnement retiré, ou déjà absent"),
        (status = 400, description = "Corps illisible ou champ inconnu", body = ErreurDto),
        (status = 422, description = "Adresse d'abonnement hors forme", body = ErreurDto),
        (status = 503, description = "Dépôt indisponible", body = ErreurDto),
    )
)]
#[delete("/api/v1/push/abonnements")]
pub async fn supprimer_abonnement(
    etat: web::Data<EtatApplication>,
    corps: web::Json<DesabonnementDto>,
) -> HttpResponse {
    // **Le contrat annonce une URL bornée, le serveur l'exige.** Cette route
    // rendait 204 sur n'importe quelle chaîne — l'idempotence porte sur
    // l'existence de l'abonnement, pas sur la forme de son adresse.
    if !adresse_plausible(&corps.endpoint) {
        return HttpResponse::UnprocessableEntity().json(ErreurDto {
            code: "ENDPOINT_INVALID".to_string(),
        });
    }
    match etat
        .abonnements
        .supprimer_par_endpoint(&corps.endpoint)
        .await
    {
        // 204 que la ligne ait existé ou non : répondre 404 sur un endpoint
        // absent transformerait cette route en oracle permettant de tester
        // l'existence d'un abonnement arbitraire.
        Ok(_) => HttpResponse::NoContent().finish(),
        Err(e) => {
            tracing::error!(erreur = %e, "suppression d'abonnement push impossible");
            HttpResponse::ServiceUnavailable().json(ErreurDto {
                code: "SERVICE_UNAVAILABLE".to_string(),
            })
        }
    }
}
