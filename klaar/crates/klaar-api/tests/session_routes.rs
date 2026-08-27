//! Story 1.3 — connexion, montée en mémoire sur une base réelle.

use actix_web::{http::StatusCode, test};
use chrono::Utc;
use klaar_api::routes::session::COOKIE_REFRESH;
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

/// Crée un compte avec un vrai mot de passe haché, actif ou non.
async fn compte(pool: &PoolPg, marqueur: &str, actif: bool) -> (Uuid, String) {
    let id = Uuid::new_v4();
    let email = format!("login-{marqueur}-{id}@example.eu");
    let mdp = MotDePasse::parse(MDP).unwrap();
    let empreinte = EmpreinteMotDePasse::calculer(&mdp, ParametresArgon2::tests()).unwrap();

    sqlx::query(
        "INSERT INTO utilisateur (id, email, empreinte_mot_de_passe, statut, locale, cree_le)
         VALUES ($1, $2, $3, $4, 'fr', $5)",
    )
    .bind(id)
    .bind(&email)
    .bind(empreinte.as_str())
    .bind(if actif {
        "ACTIVE"
    } else {
        "PENDING_EMAIL_VERIFY"
    })
    .bind(Utc::now())
    .execute(pool)
    .await
    .expect("compte de test");

    (id, email)
}

fn requete(email: &str, mot_de_passe: &str) -> test::TestRequest {
    test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .set_json(serde_json::json!({ "email": email, "mot_de_passe": mot_de_passe }))
}

#[actix_web::test]
async fn happy_un_compte_actif_recoit_un_acces_et_un_cookie_de_refresh() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (id, email) = compte(&pool, "happy", true).await;

    let reponse = test::call_service(&app, requete(&email, MDP).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::OK);

    let cookie = reponse
        .response()
        .cookies()
        .find(|c| c.name() == COOKIE_REFRESH)
        .expect("le refresh doit être posé en cookie");
    assert!(!cookie.value().is_empty());

    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["expire_dans"], 3600, "FR-004 : accès valable 1 h");
    let jeton = corps["jeton_acces"].as_str().expect("un jeton d'accès");
    assert_eq!(jeton.split('.').count(), 3, "un JWT a trois segments");

    let sessions: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM session_refresh WHERE utilisateur_id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(sessions, 1);
}

#[actix_web::test]
async fn happy_la_connexion_est_auditee() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (id, email) = compte(&pool, "audit", true).await;

    test::call_service(&app, requete(&email, MDP).to_request()).await;

    let entrees: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM journal_audit WHERE code = 'USER_LOGIN' AND sujet_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(entrees, 1);
}

#[actix_web::test]
async fn negative_un_mot_de_passe_faux_donne_401_sans_ouvrir_de_session() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (id, email) = compte(&pool, "faux", true).await;

    let reponse = test::call_service(&app, requete(&email, "Marie@2026Secur3").to_request()).await;
    assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "INVALID_CREDENTIALS");

    let sessions: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM session_refresh WHERE utilisateur_id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(sessions, 0);
}

#[actix_web::test]
async fn negative_un_compte_non_verifie_donne_403_et_le_dit() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (_, email) = compte(&pool, "attente", false).await;

    let reponse = test::call_service(&app, requete(&email, MDP).to_request()).await;
    // 403 et non 401 : les identifiants sont bons, c'est l'état du compte qui
    // bloque. Un 401 inviterait à ressaisir un mot de passe déjà correct.
    assert_eq!(reponse.status(), StatusCode::FORBIDDEN);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "ACCOUNT_NOT_VERIFIED");
}

#[actix_web::test]
async fn negative_une_saisie_invalide_donne_400() {
    let app = test::init_service(app_de_test(etat_de_test(pool().await, None))).await;
    for (email, mdp, code) in [
        ("pas-un-email", MDP, "EMAIL_MALFORMED"),
        ("marie@example.eu", "court", "PASSWORD_TOO_SHORT"),
    ] {
        let reponse = test::call_service(&app, requete(email, mdp).to_request()).await;
        assert_eq!(reponse.status(), StatusCode::BAD_REQUEST, "cas {email}");
        let corps: Value = test::read_body_json(reponse).await;
        assert_eq!(corps["code"], code);
    }
}

#[actix_web::test]
async fn edge_deux_connexions_ouvrent_deux_sessions_independantes() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (id, email) = compte(&pool, "deux", true).await;

    for _ in 0..2 {
        let reponse = test::call_service(&app, requete(&email, MDP).to_request()).await;
        assert_eq!(reponse.status(), StatusCode::OK);
    }

    let familles: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT famille_id) FROM session_refresh WHERE utilisateur_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    // Deux appareils, deux familles : couper l'un ne coupera pas l'autre.
    assert_eq!(familles, 2);
}

#[actix_web::test]
async fn edge_la_casse_de_l_adresse_n_empeche_pas_la_connexion() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (_, email) = compte(&pool, "casse", true).await;

    let reponse = test::call_service(&app, requete(&email.to_uppercase(), MDP).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::OK);
}

#[actix_web::test]
async fn edge_la_sixieme_tentative_depuis_la_meme_source_est_limitee() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (_, email) = compte(&pool, "limite", true).await;

    for _ in 0..5 {
        test::call_service(&app, requete(&email, "Marie@2026Secur3").to_request()).await;
    }
    let reponse = test::call_service(&app, requete(&email, MDP).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::TOO_MANY_REQUESTS);
    // La limite protège même la tentative correcte : un attaquant qui devine
    // au sixième essai ne doit pas être servi.
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "RATE_LIMIT_EXCEEDED");
}

#[actix_web::test]
async fn security_une_adresse_inconnue_et_un_mot_de_passe_faux_sont_indistinguables() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (_, email) = compte(&pool, "enum", true).await;

    let inconnue = test::call_service(
        &app,
        requete(&format!("jamais-{}@example.eu", Uuid::new_v4()), MDP).to_request(),
    )
    .await;
    let statut_inconnue = inconnue.status();
    let corps_inconnue = test::read_body(inconnue).await;

    let faux = test::call_service(&app, requete(&email, "Marie@2026Secur3").to_request()).await;
    assert_eq!(faux.status(), statut_inconnue);
    assert_eq!(test::read_body(faux).await, corps_inconnue);
}

#[actix_web::test]
async fn security_le_cookie_de_refresh_est_httponly_et_restreint() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (_, email) = compte(&pool, "cookie", true).await;

    let reponse = test::call_service(&app, requete(&email, MDP).to_request()).await;
    let cookie = reponse
        .response()
        .cookies()
        .find(|c| c.name() == COOKIE_REFRESH)
        .expect("cookie attendu");

    // FR-004 `@security` : jamais lisible par JavaScript.
    assert_eq!(cookie.http_only(), Some(true));
    assert_eq!(cookie.same_site(), Some(actix_web::cookie::SameSite::Lax));
    // Restreint aux routes d'authentification : seul `/auth/refresh` en a
    // besoin, l'envoyer partout l'exposerait à toute faille d'une autre route.
    assert_eq!(cookie.path(), Some("/api/v1/auth"));
    assert_eq!(
        cookie.max_age().map(|d| d.whole_days()),
        Some(30),
        "FR-004 : refresh 30 j"
    );
}

#[actix_web::test]
async fn security_le_refresh_n_est_conserve_que_hache() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (id, email) = compte(&pool, "hache", true).await;

    let reponse = test::call_service(&app, requete(&email, MDP).to_request()).await;
    let cookie = reponse
        .response()
        .cookies()
        .find(|c| c.name() == COOKIE_REFRESH)
        .expect("cookie attendu");
    let en_clair = cookie.value().to_string();

    let empreinte: String =
        sqlx::query_scalar("SELECT empreinte FROM session_refresh WHERE utilisateur_id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_ne!(empreinte, en_clair);
    assert_eq!(empreinte.len(), 64);
    assert!(empreinte.chars().all(|c| c.is_ascii_hexdigit()));
}

#[actix_web::test]
async fn security_un_echec_n_est_pas_relie_au_compte_dans_l_audit() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (id, email) = compte(&pool, "echec", true).await;

    test::call_service(&app, requete(&email, "Marie@2026Secur3").to_request()).await;

    let relies: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM journal_audit WHERE code = 'USER_LOGIN_FAILED' AND sujet_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(relies, 0, "un échec ne doit pas désigner le compte visé");
}

#[actix_web::test]
async fn security_la_reponse_ne_contient_ni_refresh_ni_identifiant() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (id, email) = compte(&pool, "fuite", true).await;

    let reponse = test::call_service(&app, requete(&email, MDP).to_request()).await;
    let refresh = reponse
        .response()
        .cookies()
        .find(|c| c.name() == COOKIE_REFRESH)
        .map(|c| c.value().to_string())
        .unwrap();

    let corps: Value = test::read_body_json(reponse).await;
    let objet = corps.as_object().expect("objet JSON");
    assert_eq!(objet.len(), 2, "réponse inattendue : {corps}");
    let brut = corps.to_string();
    assert!(
        !brut.contains(&refresh),
        "le refresh ne doit pas être dans le corps"
    );
    assert!(!brut.contains(&id.to_string()));
    assert!(!brut.contains(&email));
}

#[actix_web::test]
async fn security_un_champ_inconnu_dans_la_charge_est_refuse() {
    let app = test::init_service(app_de_test(etat_de_test(pool().await, None))).await;
    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/login")
            .set_json(serde_json::json!({
                "email": "marie@example.eu",
                "mot_de_passe": MDP,
                "statut": "ACTIVE"
            }))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::BAD_REQUEST);
}
