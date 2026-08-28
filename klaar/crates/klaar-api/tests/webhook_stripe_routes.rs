//! Story 5.5 — webhook Stripe (FR-028), contre un vrai PostgreSQL.
//!
//! **Ce qui ne se teste qu'ici :** que l'idempotence tienne dans la base et non
//! dans une lecture préalable, que le journal refuse d'être réécrit, et qu'un
//! événement arrivé après un plus récent ne défasse rien.
//!
//! **Aucun compte Stripe n'est nécessaire.** Les signatures sont fabriquées
//! avec le secret de test, exactement comme Stripe les fabrique : c'est le même
//! calcul, et c'est bien le nôtre qu'on vérifie.

use actix_web::{http::StatusCode, test};
use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use klaar_api::{app_de_test, etat_de_test, SECRET_WEBHOOK_DE_TEST};
use klaar_sqlx_repos::{creer_pool, PoolPg};
use serde_json::Value;
use sha2::Sha256;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

async fn pool() -> PoolPg {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI");
    creer_pool(&url).await.expect("connexion PostgreSQL")
}

macro_rules! bac {
    ($pool:expr) => {
        test::init_service(app_de_test(etat_de_test($pool.clone(), None))).await
    };
}

/// Un identifiant d'événement unique : la base garde les précédents, et deux
/// exécutions ne doivent pas se marcher dessus.
fn id_evenement() -> String {
    format!("evt_{}", Uuid::new_v4().simple())
}

fn charge(id: &str, type_: &str, objet: &str, cree_le: DateTime<Utc>) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "id": id,
        "type": type_,
        "created": cree_le.timestamp(),
        "data": { "object": { "id": objet } }
    }))
    .unwrap()
}

/// Signe une charge comme Stripe le fait : HMAC-SHA256 sur « t.corps ».
fn signer(corps: &[u8], quand: DateTime<Utc>, secret: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(quand.timestamp().to_string().as_bytes());
    mac.update(b".");
    mac.update(corps);
    let hex: String = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|o| format!("{o:02x}"))
        .collect();
    format!("t={},v1={}", quand.timestamp(), hex)
}

fn envoyer(corps: Vec<u8>, entete: String) -> test::TestRequest {
    test::TestRequest::post()
        .uri("/api/v1/webhooks/stripe")
        .insert_header(("Stripe-Signature", entete))
        .insert_header(("Content-Type", "application/json"))
        .set_payload(corps)
}

#[actix_web::test]
async fn happy_un_evenement_signe_est_accepte_et_consigne() {
    let pool = pool().await;
    let app = bac!(pool);
    let id = id_evenement();
    let objet = format!("pi_{}", Uuid::new_v4().simple());
    let corps = charge(&id, "payment_intent.succeeded", &objet, Utc::now());
    let entete = signer(&corps, Utc::now(), SECRET_WEBHOOK_DE_TEST);

    let corps_reponse: Value =
        test::call_and_read_body_json(&app, envoyer(corps, entete).to_request()).await;
    assert_eq!(corps_reponse["suite"], "APPLIED");
    // **Aucun argent n'a bougé, et l'API le dit.** Un 200 ne doit pas se lire
    // comme « la capture est enregistrée » tant qu'aucun séquestre n'existe.
    assert_eq!(corps_reponse["effet_applique"], false);

    let (type_, applique): (String, bool) =
        sqlx::query_as("SELECT type_, applique FROM evenement_stripe WHERE id = $1")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .expect("événement consigné");
    assert_eq!(type_, "payment_intent.succeeded");
    assert!(applique);
}

#[actix_web::test]
async fn security_un_evenement_rejoue_n_est_pas_retraite() {
    let pool = pool().await;
    let app = bac!(pool);
    let id = id_evenement();
    let objet = format!("pi_{}", Uuid::new_v4().simple());
    let corps = charge(&id, "payment_intent.succeeded", &objet, Utc::now());
    let entete = signer(&corps, Utc::now(), SECRET_WEBHOOK_DE_TEST);

    let premier: Value =
        test::call_and_read_body_json(&app, envoyer(corps.clone(), entete.clone()).to_request())
            .await;
    assert_eq!(premier["suite"], "APPLIED");

    // Exactement le même appel. Sans l'idempotence, la capture serait prélevée
    // deux fois — c'est le défaut que FR-028 `@negative` vise.
    let second: Value =
        test::call_and_read_body_json(&app, envoyer(corps, entete).to_request()).await;
    assert_eq!(second["suite"], "DUPLICATE");

    let lignes: i64 = sqlx::query_scalar("SELECT count(*) FROM evenement_stripe WHERE id = $1")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(lignes, 1, "un événement, une ligne");
}

#[actix_web::test]
async fn security_une_signature_fausse_est_refusee() {
    let pool = pool().await;
    let app = bac!(pool);
    let id = id_evenement();
    let corps = charge(&id, "payment_intent.succeeded", "pi_x", Utc::now());
    let entete = signer(&corps, Utc::now(), "un_autre_secret");

    let reponse = test::call_service(&app, envoyer(corps, entete).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::BAD_REQUEST);
    let corps_reponse: Value = test::read_body_json(reponse).await;
    assert_eq!(corps_reponse["code"], "INVALID_SIGNATURE");

    // Rien n'est consigné : une charge non authentifiée ne doit pas laisser de
    // trace exploitable par celui qui l'a envoyée.
    let lignes: i64 = sqlx::query_scalar("SELECT count(*) FROM evenement_stripe WHERE id = $1")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(lignes, 0);
}

#[actix_web::test]
async fn security_sans_signature_l_appel_est_refuse() {
    let pool = pool().await;
    let app = bac!(pool);
    let corps = charge(
        &id_evenement(),
        "payment_intent.succeeded",
        "pi_x",
        Utc::now(),
    );

    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/webhooks/stripe")
            .insert_header(("Content-Type", "application/json"))
            .set_payload(corps)
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn security_un_appel_rejoue_hors_fenetre_est_refuse() {
    let pool = pool().await;
    let app = bac!(pool);
    let id = id_evenement();
    let corps = charge(&id, "payment_intent.succeeded", "pi_x", Utc::now());
    // Signature authentique, mais vieille de dix minutes : c'est exactement
    // l'appel intercepté qu'on renvoie plus tard.
    let vieille = Utc::now() - Duration::minutes(10);
    let entete = signer(&corps, vieille, SECRET_WEBHOOK_DE_TEST);

    let reponse = test::call_service(&app, envoyer(corps, entete).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::BAD_REQUEST);
    let corps_reponse: Value = test::read_body_json(reponse).await;
    // Le même code qu'une signature fausse : distinguer les deux dirait à qui
    // essaie qu'il a trouvé le secret mais raté la fenêtre.
    assert_eq!(corps_reponse["code"], "INVALID_SIGNATURE");
}

#[actix_web::test]
async fn edge_un_evenement_depasse_est_consigne_sans_etre_applique() {
    let pool = pool().await;
    let app = bac!(pool);
    let objet = format!("pi_{}", Uuid::new_v4().simple());

    // Le remboursement arrive d'abord, daté de maintenant.
    let recent = id_evenement();
    let corps_recent = charge(&recent, "charge.refunded", &objet, Utc::now());
    let entete = signer(&corps_recent, Utc::now(), SECRET_WEBHOOK_DE_TEST);
    test::call_service(&app, envoyer(corps_recent, entete).to_request()).await;

    // La capture, plus ancienne, arrive après. L'appliquer rouvrirait une
    // capture déjà remboursée (FR-028 `@edge`).
    let ancien = id_evenement();
    let corps_ancien = charge(
        &ancien,
        "payment_intent.succeeded",
        &objet,
        Utc::now() - Duration::minutes(2),
    );
    let entete = signer(&corps_ancien, Utc::now(), SECRET_WEBHOOK_DE_TEST);
    let reponse: Value =
        test::call_and_read_body_json(&app, envoyer(corps_ancien, entete).to_request()).await;
    assert_eq!(reponse["suite"], "SUPERSEDED");

    let applique: bool = sqlx::query_scalar("SELECT applique FROM evenement_stripe WHERE id = $1")
        .bind(&ancien)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!applique, "consigné pour la trace, mais pas appliqué");
}

#[actix_web::test]
async fn edge_un_webhook_vieux_de_deux_heures_est_traite() {
    let pool = pool().await;
    let app = bac!(pool);
    let id = id_evenement();
    let objet = format!("pi_{}", Uuid::new_v4().simple());
    // La **date Stripe** est ancienne, la signature est fraîche : c'est le cas
    // d'un webhook retardé par un incident réseau, pas d'un rejeu. FR-028
    // `@edge` demande qu'il soit traité normalement.
    let corps = charge(
        &id,
        "payment_intent.succeeded",
        &objet,
        Utc::now() - Duration::hours(2),
    );
    let entete = signer(&corps, Utc::now(), SECRET_WEBHOOK_DE_TEST);

    let reponse: Value =
        test::call_and_read_body_json(&app, envoyer(corps, entete).to_request()).await;
    assert_eq!(reponse["suite"], "APPLIED");
}

#[actix_web::test]
async fn edge_un_type_non_traite_est_accuse_sans_effet() {
    let pool = pool().await;
    let app = bac!(pool);
    let id = id_evenement();
    let corps = charge(&id, "invoice.created", "in_x", Utc::now());
    let entete = signer(&corps, Utc::now(), SECRET_WEBHOOK_DE_TEST);

    // 200 et non 400 : répondre autre chose ferait réessayer Stripe
    // indéfiniment pour un message dont on n'a que faire, puis désactiver
    // l'endpoint.
    let reponse: Value =
        test::call_and_read_body_json(&app, envoyer(corps, entete).to_request()).await;
    assert_eq!(reponse["suite"], "IGNORED");
}

#[actix_web::test]
async fn negative_une_charge_illisible_est_refusee() {
    let pool = pool().await;
    let app = bac!(pool);
    for brut in [
        &b"pas du json"[..],
        // JSON valide, champs manquants.
        b"{}",
        br#"{"id":"evt_1"}"#,
        // Identifiant hors format : il servirait de clé primaire.
        br#"{"id":"pi_1","type":"payment_intent.succeeded","created":1,"data":{"object":{"id":"x"}}}"#,
    ] {
        let corps = brut.to_vec();
        let entete = signer(&corps, Utc::now(), SECRET_WEBHOOK_DE_TEST);
        let reponse = test::call_service(&app, envoyer(corps, entete).to_request()).await;
        assert_eq!(
            reponse.status(),
            StatusCode::BAD_REQUEST,
            "charge acceptée à tort : {}",
            String::from_utf8_lossy(brut)
        );
    }
}

#[actix_web::test]
async fn security_le_journal_refuse_d_etre_reecrit() {
    let pool = pool().await;
    let app = bac!(pool);
    let id = id_evenement();
    let objet = format!("pi_{}", Uuid::new_v4().simple());
    let corps = charge(&id, "payment_intent.succeeded", &objet, Utc::now());
    let entete = signer(&corps, Utc::now(), SECRET_WEBHOOK_DE_TEST);
    test::call_service(&app, envoyer(corps, entete).to_request()).await;

    // **Même en SQL direct.** Remettre `applique` à faux permettrait de rejouer
    // une capture en effaçant sa trace, c'est-à-dire de contourner exactement
    // ce que cette table protège.
    let reecriture = sqlx::query("UPDATE evenement_stripe SET applique = FALSE WHERE id = $1")
        .bind(&id)
        .execute(&pool)
        .await;
    assert!(
        reecriture.is_err(),
        "le journal doit être en insertion seule"
    );

    let suppression = sqlx::query("DELETE FROM evenement_stripe WHERE id = $1")
        .bind(&id)
        .execute(&pool)
        .await;
    assert!(
        suppression.is_err(),
        "et ne pas se laisser effacer non plus"
    );
}
