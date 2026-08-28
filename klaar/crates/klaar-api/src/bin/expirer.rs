//! Éteint les tours de diffusion écoulés (Story 3.6, FR-015) et les devis sans
//! réponse (Story 4.1, FR-016 `@edge`).
//!
//! **Un seul binaire pour les deux.** Ce sont deux balayages indépendants, mais
//! les lancer séparément doublerait la configuration, la surveillance et les
//! occasions d'en oublier un. Un échec de l'un n'empêche pas l'autre : les
//! demandeurs n'ont pas à attendre parce qu'un devis n'a pas pu s'éteindre.
//!
//! Un binaire à lancer périodiquement plutôt qu'une tâche de fond dans
//! `klaar-api`, pour la même raison que `klaar-effacer` : une tâche de fond
//! s'exécute autant de fois qu'il y a d'exemplaires du serveur, et se tait
//! quand le serveur redémarre au mauvais moment.
//!
//! **La cadence compte ici plus qu'ailleurs.** Un tour dure trente secondes ;
//! un balayage toutes les dix minutes laisserait des demandeurs attendre dix
//! minutes une nouvelle qui tient en une phrase. Une exécution toutes les dix
//! secondes est le bon ordre de grandeur, et le coût est une requête indexée
//! sur une population qui reste petite.
//!
//! Le retard éventuel n'est pas une perte : l'expiration se constate aussi à la
//! lecture (`Demande::est_acceptable`), donc aucun prestataire ne peut accepter
//! une Demande échue même si le balayage n'est pas encore passé. Ce que le
//! balayage apporte, c'est l'**avis** au demandeur.
//!
//! Idempotent : la sélection et l'extinction sont une seule opération, et un
//! second passage ne retrouve rien.

use std::process::ExitCode;
use std::sync::Arc;

use klaar_application::ports::horloge::HorlogeSysteme;
use klaar_application::usecases::expirer::expirer_les_tours;
use klaar_application::usecases::expirer_devis::expirer_les_devis;
use klaar_push_adapter::{ClesVapid, WebPushSender};
use klaar_shared_kernel::Locale;
use klaar_sqlx_repos::{
    creer_pool, PgDemandeRepository, PgDevisRepository, PgProviderRepository,
    PgPushSubscriptionRepository,
};

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt().json().init();

    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("DATABASE_URL requise");
            return ExitCode::FAILURE;
        }
    };

    let pool = match creer_pool(&database_url).await {
        Ok(pool) => pool,
        Err(e) => {
            eprintln!("connexion PostgreSQL impossible : {e}");
            return ExitCode::FAILURE;
        }
    };

    // Sans clé VAPID, les Demandes s'éteignent quand même : le statut est ce
    // qui compte, l'avis est un service en plus.
    let notifieur = match std::env::var("KLAAR_VAPID_PRIVATE_KEY") {
        Ok(cle) if !cle.is_empty() => {
            let sujet = std::env::var("KLAAR_VAPID_SUBJECT")
                .unwrap_or_else(|_| "mailto:ops@klaar.be".to_string());
            match ClesVapid::depuis_base64url(&cle, sujet) {
                Ok(cles) => Some(Arc::new(WebPushSender::new(cles))),
                Err(e) => {
                    eprintln!("clé VAPID invalide : {e}");
                    return ExitCode::FAILURE;
                }
            }
        }
        _ => {
            tracing::warn!(
                "KLAAR_VAPID_PRIVATE_KEY absente : les Demandes seront éteintes sans avis"
            );
            None
        }
    };

    let demandes = PgDemandeRepository::new(pool.clone());
    let devis = PgDevisRepository::new(pool.clone());
    let prestataires = PgProviderRepository::new(pool.clone());
    let abonnements = PgPushSubscriptionRepository::new(pool);

    // Les devis d'abord, et sans que leur sort n'engage celui des tours : les
    // deux balayages sont indépendants, et un échec ici ne doit pas laisser des
    // demandeurs devant un statut périmé.
    match expirer_les_devis(
        &devis,
        &prestataires,
        &abonnements,
        notifieur.as_deref(),
        &HorlogeSysteme,
        Locale::Fr,
    )
    .await
    {
        Ok(bilan) if bilan.eteints > 0 => {
            tracing::info!(
                eteints = bilan.eteints,
                notifies = bilan.notifies,
                "devis expirés"
            );
        }
        Ok(_) => {}
        Err(e) => tracing::error!(erreur = %e, "balayage des devis interrompu"),
    }

    match expirer_les_tours(
        &demandes,
        &abonnements,
        notifieur.as_deref(),
        &HorlogeSysteme,
        // La langue du demandeur vit sur son compte ; la lire ici demanderait
        // un dépôt de plus pour un message de deux lignes. Repli sur le
        // français, et c'est une limite écrite plutôt que découverte.
        Locale::Fr,
    )
    .await
    {
        Ok(bilan) if bilan.eteintes == 0 => {
            tracing::info!("aucun tour de diffusion à éteindre");
            ExitCode::SUCCESS
        }
        Ok(bilan) => {
            // Des nombres, jamais les identifiants : ce journal n'a pas à dire
            // qui cherchait un dépanneur.
            tracing::info!(
                eteintes = bilan.eteintes,
                notifies = bilan.notifies,
                "tours de diffusion éteints"
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            tracing::error!(erreur = %e, "balayage interrompu");
            ExitCode::FAILURE
        }
    }
}
