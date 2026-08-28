//! Story 9.1 — la langue de chacun (FR-043), contre un vrai PostgreSQL.
//!
//! **Ce qui ne se teste qu'ici :** que le changement de langue soit bien écrit
//! sur le compte, et que le repli d'une langue non parlée n'échoue pas.

use actix_web::{http::StatusCode, test};
use chrono::Utc;
use klaar_api::{app_de_test, etat_de_test};
use klaar_identity::{EmpreinteMotDePasse, MotDePasse, ParametresArgon2};
use klaar_sqlx_repos::{creer_pool, PoolPg};
use serde_json::Value;
use uuid::Uuid;

const MDP: &str = "Marie@2026Secure";
async fn pool() -> PoolPg {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI");
    creer_pool(&url).await.expect("connexion PostgreSQL")
}

async fn compte_actif(pool: &PoolPg, marqueur: &str) -> (Uuid, String) {
    let id = Uuid::new_v4();
    let email = format!("lng-{marqueur}-{id}@example.eu");
    let empreinte =
        EmpreinteMotDePasse::calculer(&MotDePasse::parse(MDP).unwrap(), ParametresArgon2::tests())
            .unwrap();
    sqlx::query(
        "INSERT INTO utilisateur (id, email, empreinte_mot_de_passe, statut, locale, cree_le)
         VALUES ($1, $2, $3, 'ACTIVE', 'fr', $4)",
    )
    .bind(id)
    .bind(&email)
    .bind(empreinte.as_str())
    .bind(Utc::now())
    .execute(pool)
    .await
    .expect("compte de test");
    (id, email)
}

macro_rules! bac {
    ($pool:expr) => {
        test::init_service(app_de_test(etat_de_test($pool.clone(), None))).await
    };
}

async fn jeton<S>(app: &S, email: &str) -> String
where
    S: actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
{
    let reponse = test::call_service(
        app,
        test::TestRequest::post()
            .uri("/api/v1/auth/login")
            .set_json(serde_json::json!({ "email": email, "mot_de_passe": MDP }))
            .to_request(),
    )
    .await;
    assert_eq!(
        reponse.status(),
        StatusCode::OK,
        "la connexion doit réussir"
    );
    let corps: Value = test::read_body_json(reponse).await;
    corps["jeton_acces"].as_str().expect("jeton").to_string()
}

// === @happy ===

#[actix_web::test]
async fn happy_le_compte_change_de_langue() {
    let pool = pool().await;
    let app = bac!(pool);
    let (id, email) = compte_actif(&pool, "bascule").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri("/api/v1/me/locale")
            .insert_header(("Authorization", format!("Bearer {jeton}")))
            .set_json(serde_json::json!({ "locale": "nl" }))
            .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["locale"], "nl");
    assert_eq!(corps["code"], "LOCALE_SET");
    assert_eq!(corps["repli"], false);

    let ecrite: String = sqlx::query_scalar("SELECT locale FROM utilisateur WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("compte relu");
    assert_eq!(ecrite, "nl");
}

// === @negative ===

#[actix_web::test]
async fn negative_une_langue_non_parlee_replie_sans_echouer() {
    // FR-043 `@negative` : quelqu'un qui demande l'allemand doit se retrouver
    // devant une application qui marche, pas devant une erreur.
    let pool = pool().await;
    let app = bac!(pool);
    let (id, email) = compte_actif(&pool, "repli").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri("/api/v1/me/locale")
            .insert_header(("Authorization", format!("Bearer {jeton}")))
            .set_json(serde_json::json!({ "locale": "de" }))
            .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["locale"], "fr");
    assert_eq!(corps["repli"], true, "le repli doit être annoncé");

    let ecrite: String = sqlx::query_scalar("SELECT locale FROM utilisateur WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("compte relu");
    assert_eq!(ecrite, "fr");
}

// === @edge ===

#[actix_web::test]
async fn edge_les_trois_langues_du_service_sont_acceptees() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "trois").await;
    let jeton = jeton(&app, &email).await;

    for langue in ["fr", "nl", "en"] {
        let reponse = test::call_service(
            &app,
            test::TestRequest::patch()
                .uri("/api/v1/me/locale")
                .insert_header(("Authorization", format!("Bearer {jeton}")))
                .set_json(serde_json::json!({ "locale": langue }))
                .to_request(),
        )
        .await;
        assert_eq!(reponse.status(), StatusCode::OK, "{langue}");
        let corps: Value = test::read_body_json(reponse).await;
        assert_eq!(corps["locale"], langue);
        assert_eq!(corps["repli"], false);
    }
}

// === @security ===

#[actix_web::test]
async fn security_la_langue_ne_se_change_que_pour_soi() {
    // La route est `/me/locale` : aucun identifiant n'entre, donc changer la
    // langue d'autrui n'est pas interdit — c'est impossible à écrire.
    let pool = pool().await;
    let app = bac!(pool);
    let (id_a, email_a) = compte_actif(&pool, "compte-a").await;
    let (id_b, _) = compte_actif(&pool, "compte-b").await;
    let jeton_a = jeton(&app, &email_a).await;

    test::call_service(
        &app,
        test::TestRequest::patch()
            .uri("/api/v1/me/locale")
            .insert_header(("Authorization", format!("Bearer {jeton_a}")))
            .set_json(serde_json::json!({ "locale": "en" }))
            .to_request(),
    )
    .await;

    let langues: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, locale FROM utilisateur WHERE id = ANY($1)")
            .bind(vec![id_a, id_b])
            .fetch_all(&pool)
            .await
            .expect("comptes relus");
    for (id, locale) in langues {
        if id == id_a {
            assert_eq!(locale, "en");
        } else {
            assert_eq!(locale, "fr", "le compte voisin ne doit pas avoir bougé");
        }
    }
}

#[actix_web::test]
async fn security_le_changement_de_langue_exige_un_jeton() {
    let pool = pool().await;
    let app = bac!(pool);

    let reponse = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri("/api/v1/me/locale")
            .set_json(serde_json::json!({ "locale": "nl" }))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn security_un_champ_inconnu_dans_le_corps_est_refuse() {
    // `deny_unknown_fields` : un `utilisateur_id` glissé dans le corps ne doit
    // pas être ignoré en silence.
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "champ-inconnu").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri("/api/v1/me/locale")
            .insert_header(("Authorization", format!("Bearer {jeton}")))
            .set_json(serde_json::json!({ "locale": "nl", "utilisateur_id": Uuid::new_v4() }))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::BAD_REQUEST);
}
