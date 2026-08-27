//! Story 1.9 — effacement RGPD (FR-005), contre un vrai PostgreSQL.
//!
//! Premier parcours authentifié : les cas passent par une vraie connexion pour
//! obtenir leur jeton, plutôt que d'en forger un. Ce qui doit tenir ici est
//! l'accord entre ce que `login` émet et ce que l'extracteur accepte.

use actix_web::{http::StatusCode, test};
use chrono::Utc;
use klaar_api::{app_de_test, etat_de_test};
use klaar_application::ports::horloge::HorlogeSysteme;
use klaar_application::usecases::effacer::executer_les_echus;
use klaar_identity::{EmpreinteMotDePasse, MotDePasse, ParametresArgon2};
use klaar_sqlx_repos::{creer_pool, PgJournalAudit, PgUtilisateurRepository, PoolPg};
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
    let email = format!("erase-{marqueur}-{id}@example.eu");
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

fn demande(jeton: &str, confirmation: &str) -> test::TestRequest {
    test::TestRequest::post()
        .uri("/api/v1/me/erase")
        .insert_header(("Authorization", format!("Bearer {jeton}")))
        .set_json(serde_json::json!({ "confirmation": confirmation }))
}

async fn etat_compte(pool: &PoolPg, id: Uuid) -> (String, Option<String>, Option<String>) {
    sqlx::query_as("SELECT statut, email, empreinte_mot_de_passe FROM utilisateur WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .map(|(s, e, p): (String, String, Option<String>)| (s, Some(e), p))
        .expect("le compte doit exister")
}

/// Fait passer l'échéance dans le passé, puis lance le job.
async fn executer_le_job(pool: &PoolPg, id: Uuid) -> usize {
    sqlx::query("UPDATE utilisateur SET efface_le = now() - interval '1 minute' WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    let depot = PgUtilisateurRepository::new(pool.clone());
    let journal = PgJournalAudit::new(pool.clone());
    executer_les_echus(&depot, &journal, &HorlogeSysteme)
        .await
        .expect("le job doit aboutir")
}

#[actix_web::test]
async fn happy_une_demande_confirmee_programme_l_effacement() {
    let pool = pool().await;
    let app = bac!(pool);
    let (id, email) = compte_actif(&pool, "happy").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(&app, demande(&jeton, "DELETE").to_request()).await;
    assert_eq!(reponse.status(), StatusCode::ACCEPTED);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "ERASURE_SCHEDULED");
    assert_eq!(corps["dans_jours"], 30);

    let (statut, _, _) = etat_compte(&pool, id).await;
    assert_eq!(statut, "ERASED_PENDING");
}

#[actix_web::test]
async fn happy_l_annulation_rend_le_compte_actif() {
    let pool = pool().await;
    let app = bac!(pool);
    let (id, email) = compte_actif(&pool, "annule").await;
    let jeton = jeton(&app, &email).await;

    test::call_service(&app, demande(&jeton, "DELETE").to_request()).await;
    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/me/erase/cancel")
            .insert_header(("Authorization", format!("Bearer {jeton}")))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::NO_CONTENT);

    let (statut, _, _) = etat_compte(&pool, id).await;
    assert_eq!(statut, "ACTIVE");
}

#[actix_web::test]
async fn happy_le_job_vide_le_compte_a_l_echeance() {
    let pool = pool().await;
    let app = bac!(pool);
    let (id, email) = compte_actif(&pool, "job").await;
    let jeton = jeton(&app, &email).await;
    test::call_service(&app, demande(&jeton, "DELETE").to_request()).await;

    assert!(executer_le_job(&pool, id).await >= 1);

    let (statut, adresse, empreinte) = etat_compte(&pool, id).await;
    assert_eq!(statut, "ERASED");
    assert_eq!(empreinte, None, "FR-005 : l'empreinte doit disparaître");
    let adresse = adresse.unwrap();
    assert!(adresse.starts_with("erased_"));
    // `.invalid` est réservé par la RFC 2606 : aucun envoi accidentel
    // n'atteindra de boîte réelle.
    assert!(adresse.ends_with("@klaar.invalid"));
    assert!(
        !adresse.contains("job"),
        "l'adresse d'origine ne doit pas transparaître"
    );
}

#[actix_web::test]
async fn negative_sans_jeton_la_demande_est_refusee() {
    let pool = pool().await;
    let app = bac!(pool);
    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/me/erase")
            .set_json(serde_json::json!({ "confirmation": "DELETE" }))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED);
    assert!(
        reponse.headers().contains_key("WWW-Authenticate"),
        "un 401 doit dire quoi présenter"
    );
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "AUTH_MISSING");
}

#[actix_web::test]
async fn negative_un_jeton_forge_est_refuse() {
    let pool = pool().await;
    let app = bac!(pool);
    for faux in [
        "abc",
        "a.b.c",
        // `alg: none` avec un sujet arbitraire : la faille classique du JWT.
        "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.eyJzdWIiOiIwMDAwMDAwMC0wMDAwLTAwMDAtMDAwMC0wMDAwMDAwMDAwMDEiLCJpYXQiOjE3ODAwMDAwMDAsImV4cCI6NDEwMjQ0NDgwMH0.",
    ] {
        let reponse = test::call_service(&app, demande(faux, "DELETE").to_request()).await;
        assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED, "jeton {faux}");
        let corps: Value = test::read_body_json(reponse).await;
        assert_eq!(corps["code"], "AUTH_INVALID");
    }
}

#[actix_web::test]
async fn negative_une_confirmation_fautive_ne_programme_rien() {
    let pool = pool().await;
    let app = bac!(pool);
    let (id, email) = compte_actif(&pool, "confirm").await;
    let jeton = jeton(&app, &email).await;

    for saisie in ["", "delete", "Delete", "SUPPRIMER"] {
        let reponse = test::call_service(&app, demande(&jeton, saisie).to_request()).await;
        assert_eq!(
            reponse.status(),
            StatusCode::BAD_REQUEST,
            "saisie {saisie:?}"
        );
        let corps: Value = test::read_body_json(reponse).await;
        assert_eq!(corps["code"], "CONFIRMATION_REQUIRED");
    }

    let (statut, _, _) = etat_compte(&pool, id).await;
    assert_eq!(statut, "ACTIVE");
}

#[actix_web::test]
async fn negative_annuler_sans_demande_donne_409() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "sans").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/me/erase/cancel")
            .insert_header(("Authorization", format!("Bearer {jeton}")))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::CONFLICT);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "NO_ERASURE_PENDING");
}

#[actix_web::test]
async fn edge_redemander_repond_202_sans_repousser_l_echeance() {
    let pool = pool().await;
    let app = bac!(pool);
    let (id, email) = compte_actif(&pool, "redemande").await;
    let jeton = jeton(&app, &email).await;

    test::call_service(&app, demande(&jeton, "DELETE").to_request()).await;
    let echeance: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT efface_le FROM utilisateur WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();

    let seconde = test::call_service(&app, demande(&jeton, "DELETE").to_request()).await;
    // 202 et non 409 : redemander n'est pas un conflit, c'est un second clic.
    assert_eq!(seconde.status(), StatusCode::ACCEPTED);
    let corps: Value = test::read_body_json(seconde).await;
    assert_eq!(corps["code"], "ERASURE_ALREADY_SCHEDULED");

    let apres: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT efface_le FROM utilisateur WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(apres, echeance);
}

#[actix_web::test]
async fn edge_le_compte_reste_utilisable_pendant_le_delai_de_grace() {
    // Le verrouiller ferait du délai de grâce une impasse : son titulaire ne
    // pourrait plus se connecter pour annuler sa propre demande.
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "grace").await;
    let jeton = jeton(&app, &email).await;
    test::call_service(&app, demande(&jeton, "DELETE").to_request()).await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/login")
            .set_json(serde_json::json!({ "email": email, "mot_de_passe": MDP }))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::OK);
}

#[actix_web::test]
async fn edge_une_reinscription_sur_l_adresse_effacee_cree_un_compte_neuf() {
    // FR-005 `@edge` : aucun chaînage avec l'ancien compte.
    let pool = pool().await;
    let app = bac!(pool);
    let (ancien, email) = compte_actif(&pool, "reinscription").await;
    let jeton = jeton(&app, &email).await;
    test::call_service(&app, demande(&jeton, "DELETE").to_request()).await;
    executer_le_job(&pool, ancien).await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/signup")
            .set_json(serde_json::json!({ "email": email, "mot_de_passe": MDP }))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::ACCEPTED);

    let nouveau: Uuid = sqlx::query_scalar("SELECT id FROM utilisateur WHERE email = $1")
        .bind(&email)
        .fetch_one(&pool)
        .await
        .expect("un compte neuf");
    assert_ne!(nouveau, ancien, "aucun lien avec l'ancien compte");
}

#[actix_web::test]
async fn security_l_effacement_emporte_sessions_jetons_et_abonnements() {
    let pool = pool().await;
    let app = bac!(pool);
    let (id, email) = compte_actif(&pool, "purge").await;
    let jeton = jeton(&app, &email).await;

    sqlx::query(
        "INSERT INTO push_subscription (id, endpoint, p256dh, auth, sujet_id)
         VALUES ($1, $2, 'p', 'a', $3)",
    )
    .bind(Uuid::new_v4())
    .bind(format!("https://push.example.net/e/{id}"))
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();

    test::call_service(&app, demande(&jeton, "DELETE").to_request()).await;
    executer_le_job(&pool, id).await;

    for (table, colonne) in [
        ("session_refresh", "utilisateur_id"),
        ("jeton_verification_email", "utilisateur_id"),
        ("push_subscription", "sujet_id"),
    ] {
        let restants: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM {table} WHERE {colonne} = $1"
        ))
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(restants, 0, "{table} doit être vidée");
    }
}

#[actix_web::test]
async fn security_le_journal_d_audit_survit_a_l_effacement() {
    // Scénario `@security` de FR-005. La ligne de compte est vidée et non
    // supprimée, précisément pour que ces entrées ne partent pas en cascade.
    let pool = pool().await;
    let app = bac!(pool);
    let (id, email) = compte_actif(&pool, "audit").await;
    let jeton = jeton(&app, &email).await;
    test::call_service(&app, demande(&jeton, "DELETE").to_request()).await;
    executer_le_job(&pool, id).await;

    let entrees: Vec<String> =
        sqlx::query_scalar("SELECT code FROM journal_audit WHERE sujet_id = $1 ORDER BY id")
            .bind(id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(entrees.contains(&"USER_ERASURE_REQUESTED".to_string()));
    assert!(entrees.contains(&"USER_ERASED".to_string()));
}

#[actix_web::test]
async fn security_un_compte_efface_ne_peut_plus_se_connecter() {
    let pool = pool().await;
    let app = bac!(pool);
    let (id, email) = compte_actif(&pool, "mort").await;
    let jeton = jeton(&app, &email).await;
    test::call_service(&app, demande(&jeton, "DELETE").to_request()).await;
    executer_le_job(&pool, id).await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/login")
            .set_json(serde_json::json!({ "email": email, "mot_de_passe": MDP }))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn security_un_jeton_encore_valide_ne_fait_rien_sur_un_compte_efface() {
    // Le jeton d'accès dure une heure et n'est pas révocable : il peut donc
    // arriver après l'effacement. Il ne doit rien pouvoir ressusciter.
    let pool = pool().await;
    let app = bac!(pool);
    let (id, email) = compte_actif(&pool, "zombie").await;
    let jeton = jeton(&app, &email).await;
    test::call_service(&app, demande(&jeton, "DELETE").to_request()).await;
    executer_le_job(&pool, id).await;

    let reponse = test::call_service(&app, demande(&jeton, "DELETE").to_request()).await;
    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);

    let (statut, _, _) = etat_compte(&pool, id).await;
    assert_eq!(
        statut, "ERASED",
        "le statut ne doit pas être revenu en arrière"
    );
}

#[actix_web::test]
async fn security_le_job_est_idempotent() {
    let pool = pool().await;
    let app = bac!(pool);
    let (id, email) = compte_actif(&pool, "idem").await;
    let jeton = jeton(&app, &email).await;
    test::call_service(&app, demande(&jeton, "DELETE").to_request()).await;

    executer_le_job(&pool, id).await;
    let depot = PgUtilisateurRepository::new(pool.clone());
    let journal = PgJournalAudit::new(pool.clone());
    executer_les_echus(&depot, &journal, &HorlogeSysteme)
        .await
        .expect("second passage");

    let effacements: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM journal_audit WHERE code = 'USER_ERASED' AND sujet_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        effacements, 1,
        "un seul effacement, quel que soit le nombre de passages"
    );
}

#[actix_web::test]
async fn security_on_ne_peut_pas_effacer_le_compte_d_un_autre() {
    // Le jeton porte l'identifiant, et la route ne lit rien d'autre : il n'y a
    // pas de paramètre par lequel viser quelqu'un d'autre. Ce test fixe cette
    // absence, pour qu'un `?utilisateur_id=` ajouté plus tard la casse
    // bruyamment.
    let pool = pool().await;
    let app = bac!(pool);
    let (_, moi) = compte_actif(&pool, "moi").await;
    let (autre_id, _) = compte_actif(&pool, "autre").await;
    let jeton = jeton(&app, &moi).await;

    test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/me/erase?utilisateur_id={autre_id}"))
            .insert_header(("Authorization", format!("Bearer {jeton}")))
            .set_json(serde_json::json!({ "confirmation": "DELETE" }))
            .to_request(),
    )
    .await;

    let (statut, _, _) = etat_compte(&pool, autre_id).await;
    assert_eq!(statut, "ACTIVE", "le compte visé ne doit pas être touché");
}

#[actix_web::test]
async fn security_un_delai_ecoule_est_requis_avant_effacement() {
    // Sans l'échéance, une demande effacerait immédiatement, et la
    // réversibilité que le délai promet n'existerait pas.
    let pool = pool().await;
    let app = bac!(pool);
    let (id, email) = compte_actif(&pool, "delai").await;
    let jeton = jeton(&app, &email).await;
    test::call_service(&app, demande(&jeton, "DELETE").to_request()).await;

    // Échéance laissée à trente jours : le job ne doit rien trouver.
    let depot = PgUtilisateurRepository::new(pool.clone());
    let journal = PgJournalAudit::new(pool.clone());
    executer_les_echus(&depot, &journal, &HorlogeSysteme)
        .await
        .unwrap();

    let (statut, _, _) = etat_compte(&pool, id).await;
    assert_eq!(statut, "ERASED_PENDING");
}
