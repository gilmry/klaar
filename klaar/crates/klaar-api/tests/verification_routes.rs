//! Story 1.2 — vérification d'adresse, montée en mémoire sur une base réelle.
//!
//! Le jeton n'est jamais lisible en base : les cas partent donc d'un jeton tiré
//! ici, dont seule l'empreinte est insérée — exactement ce que fait
//! l'inscription. C'est aussi ce qui permet de fabriquer un jeton expiré sans
//! attendre une heure.

use actix_web::{http::StatusCode, test};
use chrono::{Duration, Utc};
use klaar_api::{app_de_test, etat_de_test};
use klaar_identity::JetonVerification;
use klaar_sqlx_repos::{creer_pool, PoolPg};
use serde_json::Value;
use uuid::Uuid;

const PHC_FACTICE: &str =
    "$argon2id$v=19$m=32,t=1,p=1$c2Vsc2Vsc2Vsc2VsMQ$0000000000000000000000000000000000000000000";

async fn pool() -> PoolPg {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI");
    creer_pool(&url).await.expect("connexion PostgreSQL")
}

/// Crée un compte en attente et son jeton, et rend le jeton **en clair**.
///
/// `expire_dans` négatif fabrique un jeton déjà périmé.
async fn compte_en_attente(
    pool: &PoolPg,
    marqueur: &str,
    expire_dans: Duration,
) -> (Uuid, JetonVerification) {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO utilisateur (id, email, empreinte_mot_de_passe, statut, locale, cree_le)
         VALUES ($1, $2, $3, 'PENDING_EMAIL_VERIFY', 'fr', now())",
    )
    .bind(id)
    .bind(format!("verif-{marqueur}-{id}@example.eu"))
    .bind(PHC_FACTICE)
    .execute(pool)
    .await
    .expect("compte de test");

    let jeton = JetonVerification::tirer();
    sqlx::query(
        "INSERT INTO jeton_verification_email (empreinte, utilisateur_id, expire_le)
         VALUES ($1, $2, $3)",
    )
    .bind(jeton.empreinte().as_str())
    .bind(id)
    .bind(Utc::now() + expire_dans)
    .execute(pool)
    .await
    .expect("jeton de test");

    (id, jeton)
}

async fn statut(pool: &PoolPg, id: Uuid) -> String {
    sqlx::query_scalar("SELECT statut FROM utilisateur WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("le compte doit exister")
}

fn requete(jeton: &str) -> test::TestRequest {
    test::TestRequest::post()
        .uri("/api/v1/auth/verify-email")
        .set_json(serde_json::json!({ "jeton": jeton }))
}

#[actix_web::test]
async fn happy_un_jeton_valide_active_le_compte() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (id, jeton) = compte_en_attente(&pool, "happy", Duration::hours(1)).await;

    let reponse = test::call_service(&app, requete(jeton.expose()).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "EMAIL_VERIFIED");
    assert_eq!(statut(&pool, id).await, "ACTIVE");
}

#[actix_web::test]
async fn happy_la_verification_est_auditee() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (id, jeton) = compte_en_attente(&pool, "audit", Duration::hours(1)).await;

    test::call_service(&app, requete(jeton.expose()).to_request()).await;

    let entrees: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM journal_audit WHERE code = 'USER_EMAIL_VERIFIED' AND sujet_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(entrees, 1);
}

#[actix_web::test]
async fn negative_un_jeton_inconnu_donne_404_sans_rien_activer() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (id, _) = compte_en_attente(&pool, "inconnu", Duration::hours(1)).await;

    let reponse = test::call_service(
        &app,
        requete(JetonVerification::tirer().expose()).to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "TOKEN_INVALID");
    assert_eq!(statut(&pool, id).await, "PENDING_EMAIL_VERIFY");
}

#[actix_web::test]
async fn negative_un_jeton_expire_donne_410_et_laisse_le_compte_en_attente() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (id, jeton) = compte_en_attente(&pool, "expire", -Duration::minutes(1)).await;

    let reponse = test::call_service(&app, requete(jeton.expose()).to_request()).await;
    // 410 Gone : la ressource a existé et n'existe plus. FR-001 le nomme.
    assert_eq!(reponse.status(), StatusCode::GONE);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "TOKEN_EXPIRED");
    assert_eq!(statut(&pool, id).await, "PENDING_EMAIL_VERIFY");
}

#[actix_web::test]
async fn negative_un_jeton_vide_est_refuse_avant_toute_lecture() {
    let app = test::init_service(app_de_test(etat_de_test(pool().await, None))).await;
    for vide in ["", "   "] {
        let reponse = test::call_service(&app, requete(vide).to_request()).await;
        assert_eq!(reponse.status(), StatusCode::BAD_REQUEST);
        let corps: Value = test::read_body_json(reponse).await;
        assert_eq!(corps["code"], "TOKEN_MISSING");
    }
}

#[actix_web::test]
async fn edge_le_second_clic_repond_200_et_non_une_erreur() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (_, jeton) = compte_en_attente(&pool, "seconde", Duration::hours(1)).await;

    let premier = test::call_service(&app, requete(jeton.expose()).to_request()).await;
    assert_eq!(premier.status(), StatusCode::OK);

    let second = test::call_service(&app, requete(jeton.expose()).to_request()).await;
    assert_eq!(second.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(second).await;
    assert_eq!(corps["code"], "EMAIL_ALREADY_VERIFIED");
}

#[actix_web::test]
async fn edge_un_jeton_consomme_puis_perime_reste_deja_verifie() {
    // L'ordre des contrôles compte : consommé d'abord, expiré ensuite. Sinon,
    // quelqu'un dont le compte est actif depuis des semaines verrait « lien
    // expiré » en rouvrant l'ancien courriel.
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (_, jeton) = compte_en_attente(&pool, "perime", Duration::hours(1)).await;

    test::call_service(&app, requete(jeton.expose()).to_request()).await;
    sqlx::query("UPDATE jeton_verification_email SET expire_le = now() - interval '2 hours' WHERE empreinte = $1")
        .bind(jeton.empreinte().as_str())
        .execute(&pool)
        .await
        .unwrap();

    let reponse = test::call_service(&app, requete(jeton.expose()).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "EMAIL_ALREADY_VERIFIED");
}

#[actix_web::test]
async fn edge_un_second_clic_n_ajoute_pas_d_entree_d_audit() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (id, jeton) = compte_en_attente(&pool, "audit2", Duration::hours(1)).await;

    for _ in 0..3 {
        test::call_service(&app, requete(jeton.expose()).to_request()).await;
    }

    let entrees: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM journal_audit WHERE code = 'USER_EMAIL_VERIFIED' AND sujet_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(entrees, 1, "une adresse n'est vérifiée qu'une fois");
}

#[actix_web::test]
async fn security_le_jeton_est_conserve_hache_et_consomme_une_seule_fois() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (_, jeton) = compte_en_attente(&pool, "hache", Duration::hours(1)).await;

    // La valeur en clair n'est nulle part dans la table.
    let en_clair: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM jeton_verification_email WHERE empreinte = $1")
            .bind(jeton.expose())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(en_clair, 0);

    test::call_service(&app, requete(jeton.expose()).to_request()).await;

    let consomme: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT consomme_le FROM jeton_verification_email WHERE empreinte = $1")
            .bind(jeton.empreinte().as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(consomme.is_some(), "le jeton doit être marqué utilisé");
}

#[actix_web::test]
async fn security_le_jeton_d_un_compte_n_active_pas_celui_d_un_autre() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (id_a, jeton_a) = compte_en_attente(&pool, "croise-a", Duration::hours(1)).await;
    let (id_b, _) = compte_en_attente(&pool, "croise-b", Duration::hours(1)).await;

    test::call_service(&app, requete(jeton_a.expose()).to_request()).await;

    assert_eq!(statut(&pool, id_a).await, "ACTIVE");
    assert_eq!(statut(&pool, id_b).await, "PENDING_EMAIL_VERIFY");
}

#[actix_web::test]
async fn security_deux_presentations_concurrentes_n_activent_qu_une_fois() {
    // `FOR UPDATE` sur la ligne du jeton. Sans lui, les deux requêtes lisent
    // `consomme_le IS NULL` et consomment chacune de leur côté, produisant deux
    // entrées d'audit pour une seule vérification.
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (id, jeton) = compte_en_attente(&pool, "course", Duration::hours(1)).await;

    let (a, b) = futures_util::future::join(
        test::call_service(&app, requete(jeton.expose()).to_request()),
        test::call_service(&app, requete(jeton.expose()).to_request()),
    )
    .await;
    assert_eq!(a.status(), StatusCode::OK);
    assert_eq!(b.status(), StatusCode::OK);

    let entrees: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM journal_audit WHERE code = 'USER_EMAIL_VERIFIED' AND sujet_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        entrees, 1,
        "une seule consommation malgré deux présentations"
    );
}

#[actix_web::test]
async fn security_un_champ_inconnu_dans_la_charge_est_refuse() {
    let app = test::init_service(app_de_test(etat_de_test(pool().await, None))).await;
    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/verify-email")
            .set_json(serde_json::json!({
                "jeton": JetonVerification::tirer().expose(),
                "utilisateur_id": Uuid::new_v4().to_string()
            }))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::BAD_REQUEST);
}
