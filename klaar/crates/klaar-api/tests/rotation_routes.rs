//! Story 1.4 — rotation du refresh, détection de rejeu, déconnexion.
//!
//! Le parcours part d'une vraie connexion : fabriquer une session à la main en
//! base ne testerait pas ce que `login` écrit réellement, et c'est justement
//! l'accord entre les deux qui doit tenir.

use actix_web::{http::StatusCode, test};
use chrono::Utc;
use klaar_api::routes::session::COOKIE_REFRESH;
use klaar_api::{app_de_test, etat_de_test};
use klaar_identity::{EmpreinteMotDePasse, JetonVerification, MotDePasse, ParametresArgon2};
use klaar_sqlx_repos::{creer_pool, PoolPg};
use serde_json::Value;
use uuid::Uuid;

const MDP: &str = "Marie@2026Secure";
const UA: &str = "Firefox/120";

async fn pool() -> PoolPg {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI");
    creer_pool(&url).await.expect("connexion PostgreSQL")
}

async fn compte_actif(pool: &PoolPg, marqueur: &str) -> (Uuid, String) {
    let id = Uuid::new_v4();
    let email = format!("rot-{marqueur}-{id}@example.eu");
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

/// Se connecte et rend le refresh posé en cookie.
async fn ouvrir_session<S>(app: &S, email: &str, agent: &str) -> String
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
            .insert_header(("User-Agent", agent))
            .set_json(serde_json::json!({ "email": email, "mot_de_passe": MDP }))
            .to_request(),
    )
    .await;
    assert_eq!(
        reponse.status(),
        StatusCode::OK,
        "la connexion doit réussir"
    );
    reponse
        .response()
        .cookies()
        .find(|c| c.name() == COOKIE_REFRESH)
        .expect("cookie de refresh")
        .value()
        .to_string()
}

fn requete_refresh(refresh: &str, agent: &str) -> test::TestRequest {
    test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("User-Agent", agent))
        .cookie(
            actix_web::cookie::Cookie::build(COOKIE_REFRESH, refresh.to_string())
                .path("/api/v1/auth")
                .finish(),
        )
}

async fn empreinte_est_revoquee(pool: &PoolPg, refresh: &str) -> bool {
    let empreinte = JetonVerification::depuis_chaine(refresh).empreinte();
    sqlx::query_scalar::<_, Option<chrono::DateTime<Utc>>>(
        "SELECT revoque_le FROM session_refresh WHERE empreinte = $1",
    )
    .bind(empreinte.as_str())
    .fetch_one(pool)
    .await
    .expect("la ligne doit exister")
    .is_some()
}

#[actix_web::test]
async fn happy_la_rotation_rend_un_nouvel_acces_et_un_nouveau_refresh() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "happy").await;
    let r1 = ouvrir_session(&app, &email, UA).await;

    let reponse = test::call_service(&app, requete_refresh(&r1, UA).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::OK);

    let r2 = reponse
        .response()
        .cookies()
        .find(|c| c.name() == COOKIE_REFRESH)
        .expect("un nouveau refresh")
        .value()
        .to_string();
    assert_ne!(r2, r1, "la rotation doit changer le refresh");

    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["expire_dans"], 3600);
}

#[actix_web::test]
async fn happy_le_nouveau_refresh_sert_a_la_rotation_suivante() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "chaine").await;
    let mut courant = ouvrir_session(&app, &email, UA).await;

    for tour in 0..3 {
        let reponse = test::call_service(&app, requete_refresh(&courant, UA).to_request()).await;
        assert_eq!(reponse.status(), StatusCode::OK, "tour {tour}");
        courant = reponse
            .response()
            .cookies()
            .find(|c| c.name() == COOKIE_REFRESH)
            .expect("refresh")
            .value()
            .to_string();
    }
}

#[actix_web::test]
async fn happy_la_rotation_est_auditee() {
    let pool = pool().await;
    let app = bac!(pool);
    let (id, email) = compte_actif(&pool, "audit").await;
    let r1 = ouvrir_session(&app, &email, UA).await;

    test::call_service(&app, requete_refresh(&r1, UA).to_request()).await;

    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM journal_audit WHERE code = 'SESSION_REFRESHED' AND sujet_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 1);
}

#[actix_web::test]
async fn negative_sans_cookie_la_demande_est_refusee() {
    let pool = pool().await;
    let app = bac!(pool);
    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/refresh")
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::BAD_REQUEST);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "REFRESH_MISSING");
}

#[actix_web::test]
async fn negative_un_refresh_inconnu_est_refuse() {
    let pool = pool().await;
    let app = bac!(pool);
    let reponse = test::call_service(
        &app,
        requete_refresh(JetonVerification::tirer().expose(), UA).to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "REFRESH_INVALID");
}

#[actix_web::test]
async fn negative_un_refresh_expire_est_refuse() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "expire").await;
    let r1 = ouvrir_session(&app, &email, UA).await;

    sqlx::query(
        "UPDATE session_refresh SET expire_le = now() - interval '1 day' WHERE empreinte = $1",
    )
    .bind(JetonVerification::depuis_chaine(&r1).empreinte().as_str())
    .execute(&pool)
    .await
    .unwrap();

    let reponse = test::call_service(&app, requete_refresh(&r1, UA).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "REFRESH_EXPIRED");
}

#[actix_web::test]
async fn edge_un_changement_d_agent_est_signale_sans_couper_la_session() {
    // Les navigateurs changent d'agent utilisateur à chaque mise à jour :
    // bloquer là-dessus déconnecterait tout le monde toutes les quelques
    // semaines, sans qu'aucun vol n'ait eu lieu.
    let pool = pool().await;
    let app = bac!(pool);
    let (id, email) = compte_actif(&pool, "contexte").await;
    let r1 = ouvrir_session(&app, &email, UA).await;

    let reponse = test::call_service(&app, requete_refresh(&r1, "curl/8").to_request()).await;
    assert_eq!(reponse.status(), StatusCode::OK, "la session doit survivre");

    let anomalies: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM journal_audit
         WHERE code = 'SESSION_CONTEXT_CHANGED' AND sujet_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(anomalies, 1, "l'anomalie doit être consignée");
}

#[actix_web::test]
async fn edge_le_meme_agent_ne_leve_aucune_anomalie() {
    let pool = pool().await;
    let app = bac!(pool);
    let (id, email) = compte_actif(&pool, "stable").await;
    let r1 = ouvrir_session(&app, &email, UA).await;

    test::call_service(&app, requete_refresh(&r1, UA).to_request()).await;

    let anomalies: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM journal_audit
         WHERE code = 'SESSION_CONTEXT_CHANGED' AND sujet_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(anomalies, 0);
}

#[actix_web::test]
async fn edge_un_refus_efface_le_cookie() {
    let pool = pool().await;
    let app = bac!(pool);
    let reponse = test::call_service(
        &app,
        requete_refresh(JetonVerification::tirer().expose(), UA).to_request(),
    )
    .await;
    let cookie = reponse
        .response()
        .cookies()
        .find(|c| c.name() == COOKIE_REFRESH)
        .expect("un cookie d'effacement");
    // Durée nulle : le navigateur le jette. Le garder ferait rejouer le même
    // jeton mort à chaque tentative, et un rejeu suffit à couper la famille.
    assert_eq!(cookie.max_age().map(|d| d.whole_seconds()), Some(0));
    assert_eq!(cookie.value(), "");
}

#[actix_web::test]
async fn security_rejouer_un_refresh_consomme_coupe_toute_la_famille() {
    // Le coeur de la story. Le porteur légitime détient R2 ; présenter R1
    // signifie qu'une copie circule, sans qu'on puisse dire laquelle des deux
    // mains est la bonne — les deux sont donc coupées.
    let pool = pool().await;
    let app = bac!(pool);
    let (id, email) = compte_actif(&pool, "rejeu").await;
    let r1 = ouvrir_session(&app, &email, UA).await;

    let reponse = test::call_service(&app, requete_refresh(&r1, UA).to_request()).await;
    let r2 = reponse
        .response()
        .cookies()
        .find(|c| c.name() == COOKIE_REFRESH)
        .unwrap()
        .value()
        .to_string();

    let rejeu = test::call_service(&app, requete_refresh(&r1, UA).to_request()).await;
    assert_eq!(rejeu.status(), StatusCode::UNAUTHORIZED);
    let corps: Value = test::read_body_json(rejeu).await;
    assert_eq!(corps["code"], "REFRESH_REUSED");

    // Le refresh courant du voleur comme du porteur légitime est mort.
    assert!(empreinte_est_revoquee(&pool, &r1).await);
    assert!(empreinte_est_revoquee(&pool, &r2).await);

    let apres = test::call_service(&app, requete_refresh(&r2, UA).to_request()).await;
    assert_eq!(apres.status(), StatusCode::UNAUTHORIZED);

    let detections: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM journal_audit
         WHERE code = 'SESSION_REUSE_DETECTED' AND sujet_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(detections, 1);
}

#[actix_web::test]
async fn security_couper_une_famille_ne_touche_pas_l_autre_appareil() {
    // Deux connexions, deux familles : un vol sur l'une ne doit pas
    // déconnecter l'autre appareil, qui n'a rien à voir avec l'incident.
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "deux-appareils").await;

    let appareil_a = ouvrir_session(&app, &email, UA).await;
    let appareil_b = ouvrir_session(&app, &email, "Safari/17").await;

    test::call_service(&app, requete_refresh(&appareil_a, UA).to_request()).await;
    let rejeu = test::call_service(&app, requete_refresh(&appareil_a, UA).to_request()).await;
    assert_eq!(rejeu.status(), StatusCode::UNAUTHORIZED);

    let survivant =
        test::call_service(&app, requete_refresh(&appareil_b, "Safari/17").to_request()).await;
    assert_eq!(
        survivant.status(),
        StatusCode::OK,
        "l'autre appareil doit rester connecté"
    );
}

#[actix_web::test]
async fn security_la_deconnexion_coupe_toute_la_famille() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "logout").await;
    let r1 = ouvrir_session(&app, &email, UA).await;

    let reponse = test::call_service(&app, requete_refresh(&r1, UA).to_request()).await;
    let r2 = reponse
        .response()
        .cookies()
        .find(|c| c.name() == COOKIE_REFRESH)
        .unwrap()
        .value()
        .to_string();

    let deconnexion = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/logout")
            .cookie(
                actix_web::cookie::Cookie::build(COOKIE_REFRESH, r2.clone())
                    .path("/api/v1/auth")
                    .finish(),
            )
            .to_request(),
    )
    .await;
    assert_eq!(deconnexion.status(), StatusCode::NO_CONTENT);

    // Y compris le maillon déjà consommé : le laisser vivant rendrait la
    // détection de rejeu inopérante après une déconnexion.
    assert!(empreinte_est_revoquee(&pool, &r1).await);
    assert!(empreinte_est_revoquee(&pool, &r2).await);

    let apres = test::call_service(&app, requete_refresh(&r2, UA).to_request()).await;
    assert_eq!(apres.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn security_la_deconnexion_ne_dit_pas_si_la_session_existait() {
    // Un 404 sur un refresh inconnu ferait de la déconnexion un moyen de tester
    // la validité d'un jeton volé.
    let pool = pool().await;
    let app = bac!(pool);
    for refresh in ["", JetonVerification::tirer().expose()] {
        let reponse = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/auth/logout")
                .cookie(
                    actix_web::cookie::Cookie::build(COOKIE_REFRESH, refresh.to_string())
                        .path("/api/v1/auth")
                        .finish(),
                )
                .to_request(),
        )
        .await;
        assert_eq!(reponse.status(), StatusCode::NO_CONTENT);
    }
}

#[actix_web::test]
async fn security_le_nouveau_refresh_n_est_conserve_que_hache() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "hache").await;
    let r1 = ouvrir_session(&app, &email, UA).await;

    let reponse = test::call_service(&app, requete_refresh(&r1, UA).to_request()).await;
    let r2 = reponse
        .response()
        .cookies()
        .find(|c| c.name() == COOKIE_REFRESH)
        .unwrap()
        .value()
        .to_string();

    let en_clair: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM session_refresh WHERE empreinte = $1")
            .bind(&r2)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(en_clair, 0);
}

#[actix_web::test]
async fn security_l_agent_utilisateur_n_est_pas_conserve_en_clair() {
    // Il sert à lier le refresh à son contexte, pas à profiler : seule son
    // empreinte est écrite.
    let pool = pool().await;
    let app = bac!(pool);
    let (id, email) = compte_actif(&pool, "ua").await;
    ouvrir_session(&app, &email, "Mozilla/5.0 (TresReconnaissable)").await;

    let en_clair: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM session_refresh
         WHERE utilisateur_id = $1 AND empreinte_contexte LIKE '%Reconnaissable%'",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(en_clair, 0);

    let empreinte: Option<String> = sqlx::query_scalar(
        "SELECT empreinte_contexte FROM session_refresh WHERE utilisateur_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let empreinte = empreinte.expect("le contexte doit être enregistré");
    assert_eq!(empreinte.len(), 64);
    assert!(empreinte.chars().all(|c| c.is_ascii_hexdigit()));
}
