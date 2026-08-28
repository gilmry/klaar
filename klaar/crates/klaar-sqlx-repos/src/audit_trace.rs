//! Lectures d'audit sur la trace de matching (Story 3.8, AI Act art. 12).
//!
//! Séparé de `PgTraceRepository`, qui n'écrit que : ces requêtes ne servent pas
//! le fonctionnement du service mais son contrôle, et les mélanger ferait
//! passer une API de lecture pour un besoin courant. Elles ne sont appelées que
//! par le binaire `klaar-audit-biais`.

use chrono::{DateTime, Utc};
use klaar_application::ports::erreurs::RepositoryError;
use klaar_audit_adapter::{contenu_canonique, SignataireTrace};
use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::erreur;
use crate::pool::PoolPg;

/// Résultat du rejeu de la chaîne de signatures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Integrite {
    pub lignes_totales: u64,
    /// Maillons dont la signature a été recalculée et retrouvée.
    pub verifiees: u64,
    /// Lignes sans signature : écrites avant le scellement, ou sur un
    /// déploiement sans clé. Comptées à part plutôt que rangées avec les
    /// vérifiées — un rapport rassurant sans preuve serait le pire résultat.
    pub non_signees: u64,
    /// Identifiant de la première ligne dont la chaîne ne tient plus.
    pub rompue_a: Option<i64>,
    pub cle_disponible: bool,
    /// Premier identifiant examiné. `None` quand le rejeu part de l'origine.
    ///
    /// Exposé parce qu'il change la portée de la conclusion : une fenêtre ne
    /// dit rien de ce qui a pu disparaître avant elle.
    pub depuis_id: Option<i64>,
}

/// Ce qu'un endroit a obtenu, sur la période.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Maille {
    /// Coin sud-ouest de la maille, en degrés, arrondi.
    pub maille: String,
    pub demandes: i64,
    pub attribuees: i64,
    pub sans_reponse: i64,
    pub taux_attribution: f64,
    /// Moyenne des candidats retenus par Demande.
    pub candidats_moyens: Option<f64>,
}

/// Agrégat géographique, k-anonymisé.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DispariteGeographique {
    pub demandes_totales: i64,
    pub mailles_retenues: usize,
    /// Mailles écartées faute d'atteindre le seuil. Annoncées plutôt que
    /// tues : les taire ferait passer une couverture partielle pour une
    /// couverture complète.
    pub mailles_supprimees_sous_le_seuil: u64,
    /// Écart entre la maille la mieux servie et la moins bien servie.
    ///
    /// C'est le chiffre qui dit s'il y a un problème ; il ne se lit pas dans
    /// une liste de cent mailles.
    pub ecart_de_taux_d_attribution: Option<f64>,
    pub mailles: Vec<Maille>,
}

/// Rejoue la chaîne de signatures.
///
/// S'arrête à la première rupture : au-delà, les maillons ne prouvent plus
/// rien, et continuer à les compter comme vérifiés serait mentir.
///
/// `depuis_id` borne le rejeu. `None` reprend tout depuis l'origine, ce que
/// fait l'audit semestriel. Un identifiant de départ vérifie une **fenêtre**,
/// et cette garantie est plus faible : elle prouve que la fenêtre est
/// cohérente avec elle-même, pas qu'aucun maillon n'a disparu avant son début.
/// Le premier maillon examiné est cru sur parole quant à son prédécesseur, et
/// il n'y a pas moyen de faire autrement sans repartir de l'origine.
///
/// Deux usages justifient la fenêtre. Le premier est le passage à l'échelle :
/// rejouer un million de lignes pour auditer six mois n'a pas de sens. Le
/// second est la **rotation de clé** — la chaîne étant globale, des lignes
/// signées avec deux clés différentes se suivent, et seule une fenêtre permet
/// de vérifier le segment d'une clé donnée.
pub async fn verifier_chaine(
    pool: &PoolPg,
    signataire: Option<&SignataireTrace>,
    depuis_id: Option<i64>,
) -> Result<Integrite, RepositoryError> {
    let lignes = sqlx::query(
        "SELECT id, demande_id, provider_id, score, distance_metres, retenu, motif_ecart,
                tracee_le, signature, signature_precedente
         FROM trace_matching WHERE id >= $1 ORDER BY id",
    )
    .bind(depuis_id.unwrap_or(0))
    .fetch_all(pool)
    .await
    .map_err(erreur)?;

    let mut integrite = Integrite {
        lignes_totales: lignes.len() as u64,
        verifiees: 0,
        non_signees: 0,
        rompue_a: None,
        cle_disponible: signataire.is_some(),
        depuis_id,
    };
    // Sur une fenêtre, le premier maillon signé donne le point de départ : son
    // prédécesseur déclaré est cru, faute de pouvoir remonter. Depuis
    // l'origine, la chaîne part bien de rien.
    let mut precedente: Option<Vec<u8>> = None;
    let mut premier_signe = depuis_id.is_some();

    for ligne in &lignes {
        let signature: Option<Vec<u8>> = ligne.get("signature");
        let Some(signature) = signature else {
            // Antérieure au scellement : elle n'interrompt pas la chaîne des
            // suivantes, qui reprend au dernier maillon signé.
            integrite.non_signees += 1;
            continue;
        };

        let Some(s) = signataire else {
            // Sans clé, on ne peut rien affirmer.
            integrite.non_signees += 1;
            precedente = Some(signature);
            continue;
        };

        // Le maillon déclaré doit être celui réellement observé : sans ce
        // contrôle, quelqu'un pourrait recoller une chaîne cohérente en
        // réécrivant les deux colonnes ensemble.
        let declaree: Option<Vec<u8>> = ligne.get("signature_precedente");
        if premier_signe {
            precedente = declaree.clone();
            premier_signe = false;
        }
        if declaree.as_deref() != precedente.as_deref() {
            integrite.rompue_a = Some(ligne.get::<i64, _>("id"));
            break;
        }

        let contenu = contenu_canonique(
            &ligne.get::<Uuid, _>("demande_id"),
            &ligne.get::<Uuid, _>("provider_id"),
            ligne.get::<f64, _>("score"),
            ligne.get::<f64, _>("distance_metres"),
            ligne.get::<bool, _>("retenu"),
            ligne.get::<Option<String>, _>("motif_ecart").as_deref(),
            ligne.get::<DateTime<Utc>, _>("tracee_le").timestamp(),
        );

        if s.verifier(precedente.as_deref(), &contenu, &signature) {
            integrite.verifiees += 1;
            precedente = Some(signature);
        } else {
            integrite.rompue_a = Some(ligne.get::<i64, _>("id"));
            break;
        }
    }

    Ok(integrite)
}

/// Compte, par maille géographique, ce que chaque endroit a obtenu.
///
/// `maille_degres` fixe la finesse ; `k_anonymat` le nombre de Demandes en
/// dessous duquel une maille est supprimée du rapport, parce qu'elle
/// désignerait des personnes.
pub async fn disparite_geographique(
    pool: &PoolPg,
    depuis: DateTime<Utc>,
    maille_degres: f64,
    k_anonymat: i64,
) -> Result<DispariteGeographique, RepositoryError> {
    let lignes = sqlx::query(
        "SELECT
             floor(ST_Y(d.position::geometry) / $2) * $2 AS maille_lat,
             floor(ST_X(d.position::geometry) / $2) * $2 AS maille_lon,
             COUNT(*) AS demandes,
             COUNT(*) FILTER (WHERE d.statut = 'MATCHED') AS attribuees,
             COUNT(*) FILTER (WHERE d.statut = 'NO_MATCH') AS sans_reponse,
             AVG(t.retenus)::float8 AS candidats_moyens
         FROM demande d
         LEFT JOIN LATERAL (
             SELECT COUNT(*) FILTER (WHERE retenu) AS retenus
             FROM trace_matching WHERE demande_id = d.id
         ) t ON TRUE
         WHERE d.cree_le >= $1
         GROUP BY maille_lat, maille_lon
         ORDER BY maille_lat, maille_lon",
    )
    .bind(depuis)
    .bind(maille_degres)
    .fetch_all(pool)
    .await
    .map_err(erreur)?;

    let mut mailles = Vec::new();
    let mut supprimees = 0u64;
    let mut demandes_totales = 0i64;

    for ligne in &lignes {
        let demandes: i64 = ligne.get("demandes");
        demandes_totales += demandes;
        if demandes < k_anonymat {
            supprimees += 1;
            continue;
        }
        let attribuees: i64 = ligne.get("attribuees");
        mailles.push(Maille {
            maille: format!(
                "{:.2},{:.2}",
                ligne.get::<f64, _>("maille_lat"),
                ligne.get::<f64, _>("maille_lon")
            ),
            demandes,
            attribuees,
            sans_reponse: ligne.get("sans_reponse"),
            taux_attribution: attribuees as f64 / demandes as f64,
            candidats_moyens: ligne.get("candidats_moyens"),
        });
    }

    let ecart = if mailles.is_empty() {
        None
    } else {
        let taux: Vec<f64> = mailles.iter().map(|m| m.taux_attribution).collect();
        let min = taux.iter().copied().fold(f64::INFINITY, f64::min);
        let max = taux.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        Some(max - min)
    };

    Ok(DispariteGeographique {
        demandes_totales,
        mailles_retenues: mailles.len(),
        mailles_supprimees_sous_le_seuil: supprimees,
        ecart_de_taux_d_attribution: ecart,
        mailles,
    })
}
