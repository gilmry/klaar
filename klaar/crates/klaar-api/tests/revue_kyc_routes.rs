//! Story 8.1 — revue KYC (FR-038), contre un vrai PostgreSQL.
//!
//! **Ce qui ne se teste qu'ici :** que la règle des quatre yeux tienne dans la
//! base et pas seulement dans le code, qu'un refus proposé ne change rien à
//! l'entreprise, et qu'un retrait pendant l'examen ferme la porte à la
//! décision.

use actix_web::{http::StatusCode, test};
use chrono::{Duration, Utc};
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
const MDP_UTILISATEUR: &str = "Marie@2026Secure";
const MOTIF: &str = "Le numéro d'entreprise ne correspond à aucune inscription active à la BCE.";

async fn pool() -> PoolPg {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI");
    creer_pool(&url).await.expect("connexion PostgreSQL")
}

async fn ops(pool: &PoolPg, role: &str, marqueur: &str) -> (CompteOps, Vec<u8>) {
    let email = Email::parse(&format!("kyc-{marqueur}-{}@klaar.test", Uuid::new_v4())).unwrap();
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

fn code(secret: &[u8]) -> String {
    calculer_totp(secret, Utc::now().timestamp().div_euclid(TOTP_PAS_SECONDES))
}

macro_rules! bac {
    ($pool:expr) => {
        test::init_service(app_de_test(etat_de_test($pool.clone(), None))).await
    };
}

async fn porteur<S>(app: &S, compte: &CompteOps, secret: &[u8]) -> String
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
            .uri("/api/v1/ops/login")
            .set_json(serde_json::json!({
                "email": compte.email.as_str(), "mot_de_passe": MDP, "code": code(secret)
            }))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::OK, "connexion d'exploitation");
    let corps: Value = test::read_body_json(reponse).await;
    format!("Bearer {}", corps["jeton"].as_str().expect("jeton"))
}

/// Une entreprise en attente de contrôle. Rend (provider_id, email du compte).
async fn en_attente(pool: &PoolPg, marqueur: &str, jours: i64) -> (Uuid, String) {
    let utilisateur = Uuid::new_v4();
    let email = format!("kyc-p-{marqueur}-{utilisateur}@example.eu");
    let empreinte = EmpreinteMotDePasse::calculer(
        &MotDePasse::parse(MDP_UTILISATEUR).unwrap(),
        ParametresArgon2::tests(),
    )
    .unwrap();
    sqlx::query(
        "INSERT INTO utilisateur (id, email, empreinte_mot_de_passe, statut, locale, cree_le)
         VALUES ($1, $2, $3, 'ACTIVE', 'fr', now())",
    )
    .bind(utilisateur)
    .bind(&email)
    .bind(empreinte.as_str())
    .execute(pool)
    .await
    .expect("compte");

    let provider = Uuid::new_v4();
    let corps = (Uuid::new_v4().as_u128() as u64) % 20_000_000;
    let bce = format!("{corps:08}{:02}", 97 - (corps % 97));
    sqlx::query(
        "INSERT INTO provider
             (id, utilisateur_id, numero_bce, raison_sociale, base, statut, disponible, cree_le)
         VALUES ($1, $2, $3, 'Candidate SPRL',
                 ST_SetSRID(ST_MakePoint(4.3525, 50.8467), 4326)::geography,
                 'PENDING_KYC', FALSE, $4)",
    )
    .bind(provider)
    .bind(utilisateur)
    .bind(&bce)
    .bind(Utc::now() - Duration::days(jours))
    .execute(pool)
    .await
    .expect("entreprise en attente");
    sqlx::query(
        "INSERT INTO provider_competence (provider_id, secteur_code) VALUES ($1, 'plomberie')",
    )
    .bind(provider)
    .execute(pool)
    .await
    .expect("secteur");

    (provider, email)
}

fn reviser(entete: &str, provider: Uuid, corps: Value) -> test::TestRequest {
    test::TestRequest::post()
        .uri(&format!("/api/v1/ops/kyc/{provider}/review"))
        .insert_header(("Authorization", entete.to_string()))
        .set_json(corps)
}

async fn statut(pool: &PoolPg, provider: Uuid) -> String {
    sqlx::query_scalar("SELECT statut FROM provider WHERE id = $1")
        .bind(provider)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[actix_web::test]
async fn happy_une_validation_active_l_entreprise_immediatement() {
    let pool = pool().await;
    let app = bac!(pool);
    let (examinateur, secret) = ops(&pool, "KYC_REVIEWER", "valide").await;
    let entete = porteur(&app, &examinateur, &secret).await;
    let (provider, _) = en_attente(&pool, "valide", 1).await;

    let corps: Value = test::call_and_read_body_json(
        &app,
        reviser(
            &entete,
            provider,
            serde_json::json!({ "decision": "APPROVE" }),
        )
        .to_request(),
    )
    .await;

    assert_eq!(corps["statut"], "ACTIVE");
    assert_eq!(corps["attend_confirmation"], false);
    // **Aucun courriel n'est parti, et l'API le dit.** FR-038 `@happy` en
    // demande un ; le taire ferait croire que l'entreprise a été prévenue.
    assert_eq!(corps["notifie"], false);
    assert_eq!(statut(&pool, provider).await, "ACTIVE");

    // L'origine du contrôle dit qu'un humain a lu, pas que la BCE a répondu.
    let origine: Option<String> =
        sqlx::query_scalar("SELECT origine_kyc FROM provider WHERE id = $1")
            .bind(provider)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(origine.as_deref(), Some("OPS_REVIEW"));
}

#[actix_web::test]
async fn security_un_refus_ne_change_rien_avant_sa_confirmation() {
    let pool = pool().await;
    let app = bac!(pool);
    let (premier, s1) = ops(&pool, "KYC_REVIEWER", "refus-1").await;
    let (second, s2) = ops(&pool, "KYC_REVIEWER", "refus-2").await;
    let e1 = porteur(&app, &premier, &s1).await;
    let e2 = porteur(&app, &second, &s2).await;
    let (provider, _) = en_attente(&pool, "refus", 2).await;

    let corps: Value = test::call_and_read_body_json(
        &app,
        reviser(
            &e1,
            provider,
            serde_json::json!({ "decision": "REJECT", "motif": MOTIF }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(corps["attend_confirmation"], true);
    assert!(corps["statut"].is_null());
    // C'est le cœur des quatre yeux : proposé n'est pas décidé.
    assert_eq!(statut(&pool, provider).await, "PENDING_KYC");

    let corps: Value = test::call_and_read_body_json(
        &app,
        reviser(&e2, provider, serde_json::json!({ "decision": "REJECT" })).to_request(),
    )
    .await;
    assert_eq!(corps["statut"], "REJECTED");
    assert_eq!(statut(&pool, provider).await, "REJECTED");
}

#[actix_web::test]
async fn security_on_ne_confirme_pas_son_propre_refus() {
    let pool = pool().await;
    let app = bac!(pool);
    let (moi, secret) = ops(&pool, "KYC_REVIEWER", "solo").await;
    let entete = porteur(&app, &moi, &secret).await;
    let (provider, _) = en_attente(&pool, "solo", 1).await;

    test::call_service(
        &app,
        reviser(
            &entete,
            provider,
            serde_json::json!({ "decision": "REJECT", "motif": MOTIF }),
        )
        .to_request(),
    )
    .await;

    let reponse = test::call_service(
        &app,
        reviser(
            &entete,
            provider,
            serde_json::json!({ "decision": "REJECT" }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(
        reponse.status(),
        StatusCode::FORBIDDEN,
        "un refus se confirme par un autre compte que le sien"
    );
    assert_eq!(statut(&pool, provider).await, "PENDING_KYC");
}

#[actix_web::test]
async fn security_la_base_refuse_un_refus_confirme_par_son_auteur() {
    let pool = pool().await;
    let app = bac!(pool);
    let (moi, secret) = ops(&pool, "KYC_REVIEWER", "sql-direct").await;
    let entete = porteur(&app, &moi, &secret).await;
    let (provider, _) = en_attente(&pool, "sql-direct", 1).await;

    test::call_service(
        &app,
        reviser(
            &entete,
            provider,
            serde_json::json!({ "decision": "REJECT", "motif": MOTIF }),
        )
        .to_request(),
    )
    .await;

    // **Même en écrivant directement.** Une garantie qui ne tient que dans le
    // code s'évapore au premier script de maintenance.
    let ecrasement = sqlx::query(
        "UPDATE revue_kyc SET second_ops = premier_ops, confirme_le = now()
          WHERE provider_id = $1",
    )
    .bind(provider)
    .execute(&pool)
    .await;
    assert!(
        ecrasement.is_err(),
        "la contrainte des quatre yeux doit tenir dans la base"
    );
}

#[actix_web::test]
async fn negative_un_refus_sans_motif_donne_400() {
    let pool = pool().await;
    let app = bac!(pool);
    let (examinateur, secret) = ops(&pool, "KYC_REVIEWER", "sans-motif").await;
    let entete = porteur(&app, &examinateur, &secret).await;
    let (provider, _) = en_attente(&pool, "sans-motif", 1).await;

    for motif in [None, Some(""), Some("non")] {
        let mut corps = serde_json::json!({ "decision": "REJECT" });
        if let Some(m) = motif {
            corps["motif"] = serde_json::json!(m);
        }
        let reponse =
            test::call_service(&app, reviser(&entete, provider, corps).to_request()).await;
        // FR-038 `@negative` demande explicitement 400 « MOTIVE_REQUIRED ».
        assert_eq!(
            reponse.status(),
            StatusCode::BAD_REQUEST,
            "motif : {motif:?}"
        );
        let corps: Value = test::read_body_json(reponse).await;
        assert_eq!(corps["code"], "MOTIVE_REQUIRED");
    }
    assert_eq!(statut(&pool, provider).await, "PENDING_KYC");
}

#[actix_web::test]
async fn edge_une_entreprise_retiree_ne_se_valide_plus() {
    let pool = pool().await;
    let app = bac!(pool);
    let (examinateur, secret) = ops(&pool, "KYC_REVIEWER", "retrait").await;
    let entete = porteur(&app, &examinateur, &secret).await;
    let (provider, email) = en_attente(&pool, "retrait", 1).await;

    // L'entreprise retire sa candidature, par sa propre route.
    let jeton: Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/login")
            .set_json(serde_json::json!({ "email": email, "mot_de_passe": MDP_UTILISATEUR }))
            .to_request(),
    )
    .await;
    let retrait = test::call_service(
        &app,
        test::TestRequest::delete()
            .uri("/api/v1/providers/me/registration")
            .insert_header((
                "Authorization",
                format!("Bearer {}", jeton["jeton_acces"].as_str().unwrap()),
            ))
            .to_request(),
    )
    .await;
    assert_eq!(retrait.status(), StatusCode::NO_CONTENT);
    assert_eq!(statut(&pool, provider).await, "WITHDRAWN");

    // FR-038 `@edge` : « Provider annule pendant review » → 409
    // « PROVIDER_CANCELLED ».
    let reponse = test::call_service(
        &app,
        reviser(
            &entete,
            provider,
            serde_json::json!({ "decision": "APPROVE" }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::CONFLICT);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "PROVIDER_CANCELLED");
}

#[actix_web::test]
async fn happy_la_file_signale_l_attente_longue_et_le_refus_en_cours() {
    let pool = pool().await;
    let app = bac!(pool);
    let (premier, s1) = ops(&pool, "KYC_REVIEWER", "file-1").await;
    let (second, s2) = ops(&pool, "KYC_REVIEWER", "file-2").await;
    let e1 = porteur(&app, &premier, &s1).await;
    let e2 = porteur(&app, &second, &s2).await;
    let (vieux, _) = en_attente(&pool, "file-vieux", 9).await;

    test::call_service(
        &app,
        reviser(
            &e1,
            vieux,
            serde_json::json!({ "decision": "REJECT", "motif": MOTIF }),
        )
        .to_request(),
    )
    .await;

    let corps: Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/ops/kyc/pending")
            .insert_header(("Authorization", e2))
            .to_request(),
    )
    .await;

    let dossier = corps["dossiers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["provider_id"] == vieux.to_string())
        .expect("le dossier en attente doit figurer dans la file");
    assert_eq!(dossier["attente_longue"], true);
    assert!(dossier["attente_jours"].as_i64().unwrap() >= 9);
    // Le second examinateur doit voir le motif déjà écrit, sinon il en
    // rédigerait un qui ne servirait à rien.
    assert_eq!(dossier["refus_en_attente"]["motif"], MOTIF);
    assert_eq!(dossier["refus_en_attente"]["propose_par_moi"], false);
    assert_eq!(dossier["secteurs"][0], "plomberie");
}

#[actix_web::test]
async fn security_un_role_sans_droit_ne_revise_rien() {
    let pool = pool().await;
    let app = bac!(pool);
    // Le médiateur tranche les litiges ; il ne contrôle pas les entreprises.
    let (mediateur, secret) = ops(&pool, "MEDIATOR", "sans-droit").await;
    let entete = porteur(&app, &mediateur, &secret).await;
    let (provider, _) = en_attente(&pool, "sans-droit", 1).await;

    let reponse = test::call_service(
        &app,
        reviser(
            &entete,
            provider,
            serde_json::json!({ "decision": "APPROVE" }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::FORBIDDEN);
    assert_eq!(statut(&pool, provider).await, "PENDING_KYC");

    let refus: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM journal_ops WHERE ops_id = $1 AND geste = 'KYC_REVIEW_DENIED'",
    )
    .bind(mediateur.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(refus, 1, "le refus est journalisé");
}

#[actix_web::test]
async fn negative_un_motif_sur_une_validation_est_refuse() {
    let pool = pool().await;
    let app = bac!(pool);
    let (examinateur, secret) = ops(&pool, "KYC_REVIEWER", "motif-parasite").await;
    let entete = porteur(&app, &examinateur, &secret).await;
    let (provider, _) = en_attente(&pool, "motif-parasite", 1).await;

    // L'ignorer laisserait son auteur croire qu'il a été consigné.
    let reponse = test::call_service(
        &app,
        reviser(
            &entete,
            provider,
            serde_json::json!({ "decision": "APPROVE", "motif": MOTIF }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(statut(&pool, provider).await, "PENDING_KYC");
}
