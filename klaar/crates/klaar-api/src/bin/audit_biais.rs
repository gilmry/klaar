//! Audit semestriel de la trace de matching (Story 3.8, AI Act art. 12).
//!
//! **Ce que cet audit vérifie, et ce qu'il refuse de vérifier.**
//!
//! FR-012 `@security` demande un rapport « vérifiant l'absence de biais
//! (genre, ethnie estimée, quartier) ». Deux de ces trois axes ne sont pas
//! auditables ici, et ce n'est pas une lacune :
//!
//! - **Le genre n'est pas collecté.** L'auditer supposerait de le demander,
//!   c'est-à-dire de créer la donnée qui rendrait la discrimination possible.
//! - **L'« ethnie estimée » suppose de l'estimer**, typiquement depuis un nom.
//!   C'est précisément la pratique que l'AI Act et le RGPD (art. 9, données
//!   sensibles) proscrivent. La produire pour vérifier qu'on ne s'en sert pas
//!   serait absurde.
//!
//! La garantie sur ces deux axes est **structurelle, et plus forte qu'un
//! audit statistique** : `klaar_matching::calculer` reçoit quatre nombres —
//! distance, rayon du tour, ancienneté du contrôle, note — et rien d'autre.
//! Elle ne peut pas discriminer sur un attribut qu'on ne lui donne pas, et un
//! test fixe cette signature.
//!
//! - **Le quartier, lui, est auditable et compte vraiment.** Le score est
//!   dominé par la proximité, donc la qualité du service varie avec la densité
//!   de prestataires, donc avec la géographie. C'est un biais réel, mesurable,
//!   et sur lequel on peut agir.
//!
//! **k-anonymat.** Une maille où deux Demandes ont été émises désignerait des
//! personnes. Les mailles sous le seuil sont supprimées, et leur nombre est
//! annoncé : les taire ferait passer une couverture partielle pour une
//! couverture complète.
//!
//! **Sortie.** Le rapport part sur la sortie standard, en JSON, pour être
//! redirigé vers un fichier et joint tel quel à une demande de l'APD. Un code
//! de sortie distinct signale une chaîne rompue : c'est ce qu'un ordonnanceur
//! remarque, là où une ligne de plus dans un rapport passe inaperçue.

use std::process::ExitCode;

use chrono::{Duration, Utc};
use klaar_audit_adapter::SignataireTrace;
use klaar_sqlx_repos::audit_trace::{disparite_geographique, verifier_chaine};
use klaar_sqlx_repos::creer_pool;

/// Période auditée par défaut, en jours. Six mois, comme FR-012 le demande.
const PERIODE_JOURS: i64 = 182;

/// Nombre minimal de Demandes pour qu'une maille figure au rapport.
///
/// Cinq : en dessous, une maille d'un kilomètre de côté désignerait des foyers.
/// Le seuil est un choix, il est ici plutôt qu'implicite, et il est annoncé
/// dans le rapport.
const K_ANONYMAT: i64 = 5;

/// Côté d'une maille, en degrés. Environ un kilomètre sous nos latitudes.
///
/// Une maille régulière plutôt que la commune : les limites communales
/// demanderaient un jeu de données de plus, et une maille ne privilégie aucun
/// découpage administratif.
const MAILLE_DEGRES: f64 = 0.01;

/// Code de sortie quand la chaîne de trace est rompue.
const SORTIE_CHAINE_ROMPUE: u8 = 2;

#[tokio::main]
async fn main() -> ExitCode {
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

    // Sans clé, l'audit tourne quand même : les agrégats géographiques ne
    // dépendent pas d'elle, et le rapport dit alors explicitement que rien n'a
    // pu être vérifié plutôt que de se taire.
    let signataire = match std::env::var("KLAAR_TRACE_HMAC_KEY") {
        Ok(cle) if !cle.is_empty() => match SignataireTrace::new(cle.as_bytes()) {
            Ok(s) => Some(s),
            Err(e) => {
                eprintln!("KLAAR_TRACE_HMAC_KEY invalide : {e}");
                return ExitCode::FAILURE;
            }
        },
        _ => {
            eprintln!(
                "KLAAR_TRACE_HMAC_KEY absente : l'intégrité de la trace ne sera pas vérifiée."
            );
            None
        }
    };

    let jusqu_a = Utc::now();
    let depuis = jusqu_a - Duration::days(PERIODE_JOURS);

    let integrite = match verifier_chaine(&pool, signataire.as_ref()).await {
        Ok(i) => i,
        Err(e) => {
            eprintln!("vérification de la chaîne impossible : {e}");
            return ExitCode::FAILURE;
        }
    };
    let geographie = match disparite_geographique(&pool, depuis, MAILLE_DEGRES, K_ANONYMAT).await {
        Ok(g) => g,
        Err(e) => {
            eprintln!("agrégation géographique impossible : {e}");
            return ExitCode::FAILURE;
        }
    };

    let rapport = serde_json::json!({
        "periode": {
            "depuis": depuis.to_rfc3339(),
            "jusqu_a": jusqu_a.to_rfc3339(),
            "jours": PERIODE_JOURS,
        },
        "integrite_de_la_trace": {
            "resultat": integrite,
            "portee": "détecte une altération faite depuis la base ; ne couvre pas une \
                       compromission du serveur, où la clé est lisible",
        },
        "disparite_geographique": {
            "resultat": geographie,
            "maille_degres": MAILLE_DEGRES,
            "seuil_k_anonymat": K_ANONYMAT,
            "lecture": "un écart de taux d'attribution élevé signale un endroit moins \
                        bien servi que les autres ; la cause est la densité de \
                        prestataires, pas le score",
        },
        "axes_non_audites": {
            "genre": "non collecté ; l'auditer supposerait de créer la donnée qui \
                      rendrait la discrimination possible",
            "ethnie_estimee": "l'estimer depuis un nom est la pratique même que l'AI Act \
                               et le RGPD art. 9 proscrivent",
            "garantie_a_la_place": "klaar_matching::calculer ne reçoit que quatre nombres \
                                    — distance, rayon du tour, ancienneté du contrôle, \
                                    note — et ne peut donc discriminer sur aucun attribut \
                                    protégé",
        },
    });

    match serde_json::to_string_pretty(&rapport) {
        Ok(texte) => println!("{texte}"),
        Err(e) => {
            eprintln!("rapport non sérialisable : {e}");
            return ExitCode::FAILURE;
        }
    }

    if let Some(id) = integrite.rompue_a {
        eprintln!("chaîne de trace rompue à la ligne {id} : rapport à transmettre à l'APD");
        return ExitCode::from(SORTIE_CHAINE_ROMPUE);
    }
    ExitCode::SUCCESS
}
