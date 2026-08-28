//! Story 8.4 — console d'exploitation (FR-041, FR-042), contre PostgreSQL.
//!
//! **Ce qui ne se teste qu'ici :** que la seconde authentification soit
//! réellement exigée, que le rejeu d'un code soit fermé par la base, et que le
//! journal d'exploitation refuse toute modification.

use actix_web::{http::StatusCode, test};
use chrono::Utc;
use klaar_api::{app_de_test, etat_de_test};
use klaar_identity::{
    calculer_totp, CompteOps, EmpreinteMotDePasse, MotDePasse, ParametresArgon2, TOTP_PAS_SECONDES,
};
use klaar_shared_kernel::Email;
use klaar_sqlx_repos::{creer_pool, PgOpsRepository, PoolPg};
use serde_json::Value;
use uuid::Uuid;

use klaar_application::ports::ops_repository::OpsRepository;

const MDP: &str = "Ops@2026Securise";

async fn pool() -> PoolPg {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI");
    creer_pool(&url).await.expect("connexion PostgreSQL")
}

/// Crée un compte d'exploitation avec sa seconde authentification prête.
async fn ops(pool: &PoolPg, role: &str, marqueur: &str) -> (CompteOps, Vec<u8>) {
    let email = Email::parse(&format!("ops-{marqueur}-{}@klaar.test", Uuid::new_v4())).unwrap();
    let empreinte =
        EmpreinteMotDePasse::calculer(&MotDePasse::parse(MDP).unwrap(), ParametresArgon2::tests())
            .unwrap();
    let mut compte = CompteOps::creer(email, empreinte, role, Utc::now()).expect("rôle connu");
    let secret = vec![7u8; 20];
    compte.secret_totp = Some(secret.clone());
    assert!(PgOpsRepository::new(pool.clone())
        .creer(&compte)
        .await
        .expect("création"));
    (compte, secret)
}

/// Le code courant pour ce secret.
fn code(secret: &[u8]) -> String {
    calculer_totp(secret, Utc::now().timestamp().div_euclid(TOTP_PAS_SECONDES))
}

macro_rules! bac {
    ($pool:expr) => {
        test::init_service(app_de_test(etat_de_test($pool.clone(), None))).await
    };
}

fn connexion(email: &str, mot_de_passe: &str, code: &str) -> test::TestRequest {
    test::TestRequest::post()
        .uri("/api/v1/ops/login")
        .set_json(serde_json::json!({
            "email": email, "mot_de_passe": mot_de_passe, "code": code
        }))
}

/// Les identifiants voyagent en paramètres pour les routes authentifiées.
fn parametres(compte: &CompteOps, secret: &[u8]) -> String {
    format!(
        "email={}&mot_de_passe={}&code={}",
        urlencoding(compte.email.as_str()),
        urlencoding(MDP),
        code(secret)
    )
}

/// Encodage minimal, suffisant pour une adresse et un mot de passe de test.
fn urlencoding(brut: &str) -> String {
    brut.replace('@', "%40").replace('+', "%2B")
}

// === @happy ===

#[actix_web::test]
async fn happy_un_compte_avec_son_code_se_connecte() {
    let pool = pool().await;
    let app = bac!(pool);
    let (compte, secret) = ops(&pool, "SUPER_ADMIN", "connexion").await;

    let reponse = test::call_service(
        &app,
        connexion(compte.email.as_str(), MDP, &code(&secret)).to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "OPS_AUTHENTICATED");
    assert_eq!(corps["role"], "SUPER_ADMIN");
}

#[actix_web::test]
async fn happy_un_super_admin_cree_un_compte_et_recoit_son_secret() {
    let pool = pool().await;
    let app = bac!(pool);
    let (patron, secret) = ops(&pool, "SUPER_ADMIN", "createur").await;

    let nouvelle = format!("nouvel-ops-{}@klaar.test", Uuid::new_v4());
    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!(
                "/api/v1/ops/accounts?{}",
                parametres(&patron, &secret)
            ))
            .set_json(serde_json::json!({
                "email": nouvelle, "mot_de_passe": MDP, "role": "KYC_REVIEWER"
            }))
            .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::CREATED);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "OPS_ACCOUNT_CREATED");
    assert_eq!(corps["role"], "KYC_REVIEWER");
    // Le secret est rendu une fois, en base32 lisible par une application
    // d'authentification.
    let secret_lisible = corps["secret_totp"].as_str().expect("secret");
    assert!(secret_lisible.len() >= 32);
    assert!(secret_lisible
        .chars()
        .all(|c| c.is_ascii_uppercase() || ('2'..='7').contains(&c)));
}

// === @negative ===

#[actix_web::test]
async fn negative_sans_code_la_connexion_est_refusee() {
    // FR-041 `@security` : sans seconde authentification, accès bloqué.
    let pool = pool().await;
    let app = bac!(pool);
    let (compte, _) = ops(&pool, "SUPER_ADMIN", "sans-code").await;

    for mauvais in ["", "000000", "12345", "abcdef"] {
        let reponse = test::call_service(
            &app,
            connexion(compte.email.as_str(), MDP, mauvais).to_request(),
        )
        .await;
        assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED, "code {mauvais}");
    }
}

#[actix_web::test]
async fn negative_un_mauvais_mot_de_passe_est_refuse() {
    let pool = pool().await;
    let app = bac!(pool);
    let (compte, secret) = ops(&pool, "SUPER_ADMIN", "mdp-faux").await;

    let reponse = test::call_service(
        &app,
        connexion(compte.email.as_str(), "Autre@2026Secure", &code(&secret)).to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn negative_un_role_inconnu_est_refuse_a_la_creation() {
    // FR-041 `@negative` : 422.
    let pool = pool().await;
    let app = bac!(pool);
    let (patron, secret) = ops(&pool, "SUPER_ADMIN", "role-faux").await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!(
                "/api/v1/ops/accounts?{}",
                parametres(&patron, &secret)
            ))
            .set_json(serde_json::json!({
                "email": format!("x-{}@klaar.test", Uuid::new_v4()),
                "mot_de_passe": MDP,
                "role": "super_root"
            }))
            .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "ROLE_UNKNOWN");
}

// === @edge ===

#[actix_web::test]
async fn edge_un_compte_desactive_ne_se_connecte_plus() {
    let pool = pool().await;
    let app = bac!(pool);
    let (compte, secret) = ops(&pool, "SUPER_ADMIN", "revoque").await;
    sqlx::query("UPDATE compte_ops SET actif = FALSE WHERE id = $1")
        .bind(compte.id)
        .execute(&pool)
        .await
        .expect("désactivation");

    let reponse = test::call_service(
        &app,
        connexion(compte.email.as_str(), MDP, &code(&secret)).to_request(),
    )
    .await;

    // 403 et non 401 : les identifiants sont bons, c'est l'état du compte qui
    // refuse. Le distinguer n'apprend rien à qui n'a pas le mot de passe,
    // puisqu'il faut l'avoir pour arriver jusqu'ici.
    assert_eq!(reponse.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn edge_une_adresse_deja_prise_est_refusee() {
    let pool = pool().await;
    let app = bac!(pool);
    let (patron, secret) = ops(&pool, "SUPER_ADMIN", "doublon").await;
    let (existant, _) = ops(&pool, "READER", "existant").await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!(
                "/api/v1/ops/accounts?{}",
                parametres(&patron, &secret)
            ))
            .set_json(serde_json::json!({
                "email": existant.email.as_str(), "mot_de_passe": MDP, "role": "READER"
            }))
            .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::CONFLICT);
}

// === @security ===

#[actix_web::test]
async fn security_un_code_deja_utilise_ne_repasse_pas() {
    // **Sans cela, un code lu par-dessus une épaule reste utilisable une minute
    // et demie.** C'est la fenêtre de tolérance qui l'exige, et le
    // compare-and-swap en base qui la referme.
    let pool = pool().await;
    let app = bac!(pool);
    let (compte, secret) = ops(&pool, "SUPER_ADMIN", "rejeu").await;
    let unique = code(&secret);

    let premiere = test::call_service(
        &app,
        connexion(compte.email.as_str(), MDP, &unique).to_request(),
    )
    .await;
    assert_eq!(premiere.status(), StatusCode::OK);

    let seconde = test::call_service(
        &app,
        connexion(compte.email.as_str(), MDP, &unique).to_request(),
    )
    .await;
    assert_eq!(
        seconde.status(),
        StatusCode::UNAUTHORIZED,
        "un code consommé ne doit pas repasser"
    );
}

#[actix_web::test]
async fn security_un_lecteur_ne_cree_pas_de_compte() {
    // Qui peut créer un compte peut se créer un super-administrateur : ce droit
    // n'appartient qu'à un seul rôle.
    let pool = pool().await;
    let app = bac!(pool);
    let (lecteur, secret) = ops(&pool, "READER", "lecteur-createur").await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!(
                "/api/v1/ops/accounts?{}",
                parametres(&lecteur, &secret)
            ))
            .set_json(serde_json::json!({
                "email": format!("x-{}@klaar.test", Uuid::new_v4()),
                "mot_de_passe": MDP,
                "role": "SUPER_ADMIN"
            }))
            .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::FORBIDDEN);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "FORBIDDEN");
}

#[actix_web::test]
async fn security_un_refus_de_droit_est_consigne() {
    // Une tentative d'accès hors droits est précisément ce qu'un journal
    // d'exploitation doit montrer.
    let pool = pool().await;
    let app = bac!(pool);
    let (lecteur, secret) = ops(&pool, "READER", "refus-consigne").await;

    test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!(
                "/api/v1/ops/accounts?{}",
                parametres(&lecteur, &secret)
            ))
            .set_json(serde_json::json!({
                "email": format!("x-{}@klaar.test", Uuid::new_v4()),
                "mot_de_passe": MDP,
                "role": "READER"
            }))
            .to_request(),
    )
    .await;

    let gestes: Vec<String> = sqlx::query_scalar("SELECT geste FROM journal_ops WHERE ops_id = $1")
        .bind(lecteur.id)
        .fetch_all(&pool)
        .await
        .expect("journal");
    assert!(
        gestes.iter().any(|g| g == "OPS_MANAGE_DENIED"),
        "le refus doit être consigné : {gestes:?}"
    );
}

#[actix_web::test]
async fn security_la_lecture_du_journal_est_elle_meme_journalisee() {
    // Qui a consulté quoi est ce qu'un audit de sécurité vient chercher : un
    // journal qui ne consigne pas ses propres lectures ne dit qu'une moitié de
    // l'histoire.
    let pool = pool().await;
    let app = bac!(pool);
    let (lecteur, secret) = ops(&pool, "READER", "lecture-tracee").await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/v1/ops/audit?{}",
                parametres(&lecteur, &secret)
            ))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["par_page"], 50);

    let gestes: Vec<String> = sqlx::query_scalar("SELECT geste FROM journal_ops WHERE ops_id = $1")
        .bind(lecteur.id)
        .fetch_all(&pool)
        .await
        .expect("journal");
    assert!(gestes.iter().any(|g| g == "AUDIT_READ"), "{gestes:?}");
}

#[actix_web::test]
async fn security_le_journal_d_exploitation_ne_se_modifie_pas() {
    // FR-042 `@security` : « même un super-admin ne peut modifier ».
    let pool = pool().await;
    let app = bac!(pool);
    let (lecteur, secret) = ops(&pool, "READER", "journal-fige").await;
    test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/v1/ops/audit?{}",
                parametres(&lecteur, &secret)
            ))
            .to_request(),
    )
    .await;

    for tentative in [
        "UPDATE journal_ops SET geste = 'RIEN' WHERE ops_id = $1",
        "DELETE FROM journal_ops WHERE ops_id = $1",
    ] {
        let refus = sqlx::query(tentative)
            .bind(lecteur.id)
            .execute(&pool)
            .await
            .expect_err("le journal doit être insert-only");
        assert!(
            refus.to_string().contains("insert-only"),
            "déclencheur attendu, obtenu : {refus}"
        );
    }
}

#[actix_web::test]
async fn security_un_secret_deja_configure_ne_se_remplace_pas() {
    // Le remplacer permettrait à quelqu'un qui a volé une session de
    // reconfigurer la seconde authentification sur son propre téléphone.
    let pool = pool().await;
    let (compte, _) = ops(&pool, "SUPER_ADMIN", "secret-fige").await;
    let depot = PgOpsRepository::new(pool.clone());

    let remplace = depot
        .configurer_totp(compte.id, &[9u8; 20])
        .await
        .expect("appel abouti");
    assert!(!remplace, "un secret existant ne doit pas être écrasé");

    let secret: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT secret_totp FROM compte_ops WHERE id = $1")
            .bind(compte.id)
            .fetch_one(&pool)
            .await
            .expect("compte relu");
    assert_eq!(secret, Some(vec![7u8; 20]), "le secret d'origine tient");
}

#[actix_web::test]
async fn security_une_adresse_inconnue_donne_le_meme_refus() {
    // Distinguer « cette adresse n'existe pas » de « le mot de passe est faux »
    // donnerait la liste des comptes d'exploitation à qui essaie.
    let pool = pool().await;
    let app = bac!(pool);

    let reponse = test::call_service(
        &app,
        connexion("personne@klaar.test", MDP, "123456").to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "OPS_CREDENTIALS_INVALID");
}
