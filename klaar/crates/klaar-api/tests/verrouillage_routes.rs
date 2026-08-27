//! Story 1.8 — verrouillage anti-brute-force (FR-007), contre un vrai PostgreSQL.
//!
//! La limitation par adresse IP (5 tentatives par heure) plafonne avant le
//! verrou (5 échecs). Les cas qui doivent atteindre le verrou changent donc
//! d'adresse source à chaque tentative : c'est exactement ce que ferait une
//! attaque distribuée, et c'est le scénario que FR-007 vise.

use actix_web::{http::StatusCode, test};
use chrono::Utc;
use klaar_api::{app_de_test, etat_de_test};
use klaar_identity::{EmpreinteMotDePasse, MotDePasse, ParametresArgon2};
use klaar_sqlx_repos::{creer_pool, PoolPg};
use serde_json::Value;
use uuid::Uuid;

const MDP: &str = "Marie@2026Secure";
const FAUX: &str = "Marie@2026Secur3";

async fn pool() -> PoolPg {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI");
    creer_pool(&url).await.expect("connexion PostgreSQL")
}

async fn compte_actif(pool: &PoolPg, marqueur: &str) -> (Uuid, String) {
    let id = Uuid::new_v4();
    let email = format!("lock-{marqueur}-{id}@example.eu");
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

/// Une requête de connexion depuis une adresse source distincte.
///
/// `X-Forwarded-For` n'est cru que si le déploiement le déclare, et
/// `etat_de_test` ne le déclare pas. C'est donc `peer_addr` que la route lit,
/// que `TestRequest` laisse choisir.
fn requete(email: &str, mot_de_passe: &str, source: u8) -> test::TestRequest {
    test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr(format!("10.0.0.{source}:40000").parse().unwrap())
        .set_json(serde_json::json!({ "email": email, "mot_de_passe": mot_de_passe }))
}

async fn verrouillage(pool: &PoolPg, id: Uuid) -> (i32, Option<chrono::DateTime<Utc>>) {
    sqlx::query_as("SELECT echecs_consecutifs, verrouille_jusqu_a FROM utilisateur WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("le compte doit exister")
}

#[actix_web::test]
async fn happy_cinq_echecs_verrouillent_le_compte() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (id, email) = compte_actif(&pool, "cinq").await;

    for source in 1..=5 {
        let reponse = test::call_service(&app, requete(&email, FAUX, source).to_request()).await;
        assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED, "essai {source}");
    }

    let (echecs, jusqu_a) = verrouillage(&pool, id).await;
    assert_eq!(echecs, 5);
    assert!(jusqu_a.is_some(), "le compte doit être verrouillé");
}

#[actix_web::test]
async fn happy_le_verrouillage_est_audite_une_seule_fois() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (id, email) = compte_actif(&pool, "audit").await;

    for source in 1..=8 {
        test::call_service(&app, requete(&email, FAUX, source).to_request()).await;
    }

    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM journal_audit WHERE code = 'ACCOUNT_LOCKED' AND sujet_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 1, "une seule entrée, quel que soit le nombre d'essais");
}

#[actix_web::test]
async fn negative_le_bon_mot_de_passe_sur_compte_verrouille_donne_423() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (_, email) = compte_actif(&pool, "423").await;

    for source in 1..=5 {
        test::call_service(&app, requete(&email, FAUX, source).to_request()).await;
    }

    let reponse = test::call_service(&app, requete(&email, MDP, 6).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::LOCKED);
    let retry: i64 = reponse
        .headers()
        .get("Retry-After")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .expect("Retry-After numérique");
    assert!(
        (1..=900).contains(&retry),
        "Retry-After hors du verrou de 15 min : {retry}"
    );
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "ACCOUNT_LOCKED");
}

#[actix_web::test]
async fn negative_quatre_echecs_ne_verrouillent_pas() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (id, email) = compte_actif(&pool, "quatre").await;

    for source in 1..=4 {
        test::call_service(&app, requete(&email, FAUX, source).to_request()).await;
    }

    let (echecs, jusqu_a) = verrouillage(&pool, id).await;
    assert_eq!(echecs, 4);
    assert_eq!(jusqu_a, None);

    let reponse = test::call_service(&app, requete(&email, MDP, 5).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::OK);
}

#[actix_web::test]
async fn edge_une_connexion_reussie_remet_le_compteur_a_zero() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (id, email) = compte_actif(&pool, "reset").await;

    for source in 1..=3 {
        test::call_service(&app, requete(&email, FAUX, source).to_request()).await;
    }
    test::call_service(&app, requete(&email, MDP, 4).to_request()).await;

    let (echecs, jusqu_a) = verrouillage(&pool, id).await;
    assert_eq!(echecs, 0, "le compteur doit repartir de zéro");
    assert_eq!(jusqu_a, None);
}

#[actix_web::test]
async fn edge_marteler_un_compte_verrouille_ne_prolonge_pas_le_verrou() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (id, email) = compte_actif(&pool, "martele").await;

    for source in 1..=5 {
        test::call_service(&app, requete(&email, FAUX, source).to_request()).await;
    }
    let (_, fin_initiale) = verrouillage(&pool, id).await;

    for source in 6..=20 {
        test::call_service(&app, requete(&email, FAUX, source).to_request()).await;
    }

    let (_, fin_apres) = verrouillage(&pool, id).await;
    // Sinon, un tiers maintient un compte fermé indéfiniment en réessayant,
    // soit exactement l'attaque que le verrou prétend arrêter.
    assert_eq!(fin_apres, fin_initiale);
}

#[actix_web::test]
async fn security_un_mauvais_mot_de_passe_sur_compte_verrouille_reste_un_401() {
    // Le coeur de l'arbitrage FR-007 : répondre 423 à qui ne connaît pas le mot
    // de passe apprendrait que l'adresse existe, ce que le scénario `@security`
    // du même FR interdit. La réponse doit être celle d'une adresse inconnue.
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (_, email) = compte_actif(&pool, "enum").await;

    for source in 1..=5 {
        test::call_service(&app, requete(&email, FAUX, source).to_request()).await;
    }

    let verrouille =
        test::call_service(&app, requete(&email, "EncoreUnAutre@2026", 6).to_request()).await;
    let statut_verrouille = verrouille.status();
    let corps_verrouille = test::read_body(verrouille).await;

    let inconnue = test::call_service(
        &app,
        requete(
            &format!("jamais-{}@example.eu", Uuid::new_v4()),
            "EncoreUnAutre@2026",
            7,
        )
        .to_request(),
    )
    .await;

    assert_eq!(statut_verrouille, StatusCode::UNAUTHORIZED);
    assert_eq!(inconnue.status(), statut_verrouille);
    assert_eq!(test::read_body(inconnue).await, corps_verrouille);
}

#[actix_web::test]
async fn security_un_compte_verrouille_n_obtient_aucune_session() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (id, email) = compte_actif(&pool, "session").await;

    for source in 1..=5 {
        test::call_service(&app, requete(&email, FAUX, source).to_request()).await;
    }
    test::call_service(&app, requete(&email, MDP, 6).to_request()).await;

    let sessions: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM session_refresh WHERE utilisateur_id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(sessions, 0);
}

#[actix_web::test]
async fn security_les_echecs_ne_sont_pas_relies_au_compte_dans_l_audit() {
    // `ACCOUNT_LOCKED` porte l'identifiant — l'événement concerne le compte
    // lui-même. `USER_LOGIN_FAILED` ne le porte pas : ce serait un oracle.
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (id, email) = compte_actif(&pool, "audit-echec").await;

    for source in 1..=5 {
        test::call_service(&app, requete(&email, FAUX, source).to_request()).await;
    }

    let relies: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM journal_audit WHERE code = 'USER_LOGIN_FAILED' AND sujet_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(relies, 0);
}

#[actix_web::test]
async fn security_un_compte_inexistant_ne_se_verrouille_pas_et_repond_pareil() {
    // Rien à verrouiller, et surtout : aucune ligne créée. Sinon, marteler des
    // adresses inventées ferait grossir la table indéfiniment.
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let inexistante = format!("fantome-{}@example.eu", Uuid::new_v4());

    for source in 1..=6 {
        let reponse =
            test::call_service(&app, requete(&inexistante, FAUX, source).to_request()).await;
        assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED, "essai {source}");
    }

    let lignes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM utilisateur WHERE email = $1")
        .bind(&inexistante)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(lignes, 0);
}

#[actix_web::test]
async fn security_le_verrou_d_un_compte_n_affecte_pas_un_autre() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let (_, cible) = compte_actif(&pool, "cible").await;
    let (_, voisin) = compte_actif(&pool, "voisin").await;

    for source in 1..=5 {
        test::call_service(&app, requete(&cible, FAUX, source).to_request()).await;
    }

    let reponse = test::call_service(&app, requete(&voisin, MDP, 6).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::OK);
}
