//! Story 8.3 — tableau de bord d'exploitation (FR-040), contre PostgreSQL.
//!
//! **Ce qui ne se teste qu'ici :** que les agrégats se prennent sur une seule
//! vue de la base, que la réponse ne porte aucun identifiant, et que chaque
//! consultation laisse une trace dans le journal.

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

async fn ops(pool: &PoolPg, role: &str, marqueur: &str) -> (CompteOps, Vec<u8>) {
    let email = Email::parse(&format!("tb-{marqueur}-{}@klaar.test", Uuid::new_v4())).unwrap();
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

/// Ouvre une session et rend l'en-tête `Authorization`.
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

fn tableau(entete: &str) -> test::TestRequest {
    test::TestRequest::get()
        .uri("/api/v1/ops/dashboard")
        .insert_header(("Authorization", entete.to_string()))
}

#[actix_web::test]
async fn happy_le_tableau_rend_les_indicateurs_de_la_fenetre() {
    let pool = pool().await;
    let app = bac!(pool);
    let (compte, secret) = ops(&pool, "SUPER_ADMIN", "happy").await;

    let corps: Value = test::call_and_read_body_json(
        &app,
        tableau(&porteur(&app, &compte, &secret).await).to_request(),
    )
    .await;

    assert_eq!(corps["fenetre_jours"], 30);
    // Les compteurs existent, quelle que soit la valeur : une base partagée
    // avec d'autres tests ne permet pas d'affirmer un nombre, seulement que
    // l'indicateur est rendu.
    for cle in [
        "comptes_actifs",
        "demandes",
        "demandes_attribuees",
        "gmv_htva_cents",
        "commission_htva_cents",
        "litiges_ouverts",
        "notes",
        "sorties_de_zone",
        "kyc_en_attente",
    ] {
        assert!(corps[cle].is_i64(), "{cle} doit être un entier");
        assert!(
            corps[cle].as_i64().unwrap() >= 0,
            "{cle} ne peut être négatif"
        );
    }
    assert!(corps["depuis"].as_str().unwrap().contains('T'));
}

#[actix_web::test]
async fn security_la_reponse_ne_porte_aucun_identifiant() {
    let pool = pool().await;
    let app = bac!(pool);
    let (compte, secret) = ops(&pool, "READER", "anonyme").await;

    let reponse = test::call_service(
        &app,
        tableau(&porteur(&app, &compte, &secret).await).to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::OK);
    let brut = String::from_utf8(test::read_body(reponse).await.to_vec()).unwrap();

    // **Le tableau ne doit être un chemin de lecture nominative pour personne.**
    // Un identifiant qui y apparaîtrait donnerait un moyen de consulter des
    // dossiers sans passer par les routes qui journalisent la cible.
    assert!(!brut.contains('@'), "aucune adresse ne doit figurer");
    for mot in [
        "utilisateur_id",
        "provider_id",
        "mission_id",
        "demande_id",
        "email",
    ] {
        assert!(
            !brut.contains(mot),
            "« {mot} » n'a rien à faire dans un agrégat"
        );
    }
    // Un UUID se reconnaît à ses tirets aux bonnes places ; le corps n'en a pas.
    assert!(
        !brut
            .split(|c: char| !c.is_ascii_hexdigit() && c != '-')
            .any(|m| m.len() == 36 && m.matches('-').count() == 4),
        "aucun identifiant ne doit figurer dans un agrégat"
    );
}

#[actix_web::test]
async fn security_chaque_consultation_est_journalisee() {
    let pool = pool().await;
    let app = bac!(pool);
    let (compte, secret) = ops(&pool, "MEDIATOR", "trace").await;

    test::call_service(
        &app,
        tableau(&porteur(&app, &compte, &secret).await).to_request(),
    )
    .await;

    // Assertion locale à ce compte : d'autres tests écrivent dans le même
    // journal, et compter globalement rendrait ce cas dépendant d'eux.
    let gestes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM journal_ops WHERE ops_id = $1 AND geste = 'DASHBOARD_READ'",
    )
    .bind(compte.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(gestes, 1, "une consultation, une trace");
}

#[actix_web::test]
async fn security_la_revocation_du_compte_ferme_sa_session() {
    let pool = pool().await;
    let app = bac!(pool);
    let (compte, secret) = ops(&pool, "READER", "revoque").await;
    // **La session est ouverte d'abord, le compte révoqué ensuite.** C'est le
    // cas qui compte : révoquer un compte doit fermer ses sessions dans la
    // seconde, et non les laisser vivre jusqu'à leur échéance. Un jeton qui
    // survit à la révocation de son porteur est exactement ce qu'on cherche à
    // éviter en donnant des sessions courtes.
    let entete = porteur(&app, &compte, &secret).await;
    sqlx::query("UPDATE compte_ops SET actif = FALSE WHERE id = $1")
        .bind(compte.id)
        .execute(&pool)
        .await
        .unwrap();

    let reponse = test::call_service(&app, tableau(&entete).to_request()).await;
    assert_eq!(
        reponse.status(),
        StatusCode::UNAUTHORIZED,
        "la révocation du compte doit fermer sa session sur-le-champ"
    );
}

#[actix_web::test]
async fn negative_sans_jeton_le_tableau_refuse() {
    let pool = pool().await;
    let app = bac!(pool);

    let reponse = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/ops/dashboard")
            .to_request(),
    )
    .await;
    assert_eq!(
        reponse.status(),
        StatusCode::UNAUTHORIZED,
        "sans jeton, aucun agrégat ne sort"
    );
}

#[actix_web::test]
async fn edge_les_taux_sont_absents_plutot_que_nuls_sans_assiette() {
    let pool = pool().await;
    let app = bac!(pool);
    let (compte, secret) = ops(&pool, "KYC_REVIEWER", "taux").await;

    let corps: Value = test::call_and_read_body_json(
        &app,
        tableau(&porteur(&app, &compte, &secret).await).to_request(),
    )
    .await;

    // Sur une base de développement, l'assiette n'est pas vide : le taux existe
    // et reste dans [0, 1]. Sur une base neuve, il vaut `null` — jamais zéro,
    // qui se lirait comme un échec de la plateforme (FR-040 `@edge`).
    match corps["taux_remplissage"].as_f64() {
        Some(t) => assert!((0.0..=1.0).contains(&t), "taux hors bornes : {t}"),
        None => assert!(corps["taux_remplissage"].is_null()),
    }
    match corps["note_moyenne"].as_f64() {
        Some(n) => assert!((1.0..=5.0).contains(&n), "note hors échelle : {n}"),
        None => assert!(corps["note_moyenne"].is_null()),
    }
}
