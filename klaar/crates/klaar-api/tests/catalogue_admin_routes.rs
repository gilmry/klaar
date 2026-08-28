//! Story 2.4 — administration du catalogue (FR-010), contre PostgreSQL.
//!
//! **Ce qui ne se teste qu'ici :** que la règle des quatre yeux tienne dans la
//! base et pas seulement dans le code, et que le refus de retirer un secteur
//! porteur d'interventions se décide dans la même requête que le retrait — un
//! comptage lu séparément serait déjà faux au moment du clic.

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
    let email = Email::parse(&format!("cat-{marqueur}-{}@klaar.test", Uuid::new_v4())).unwrap();
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

fn code_totp(secret: &[u8]) -> String {
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
                "email": compte.email.as_str(), "mot_de_passe": MDP, "code": code_totp(secret)
            }))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::OK, "connexion d'exploitation");
    let corps: Value = test::read_body_json(reponse).await;
    format!("Bearer {}", corps["jeton"].as_str().expect("jeton"))
}

/// Un code de secteur unique : la base garde les précédents.
fn code_secteur() -> String {
    format!("secteur-{}", Uuid::new_v4().simple())
}

/// Retire les secteurs créés par ce cas.
///
/// **Les tests d'administration publient réellement**, et un secteur publié
/// reste dans le catalogue **public** de la base de développement. Sans ce
/// nettoyage, chaque exécution en ajoute — et un test du catalogue public qui
/// compte ses secteurs finit par échouer sur des lignes qu'il n'a pas créées.
/// C'est arrivé, et c'est ce qui a fait découvrir la fragilité de l'autre
/// suite.
async fn ranger(pool: &PoolPg, code: &str) {
    let _ = sqlx::query("DELETE FROM secteur WHERE code = $1")
        .bind(code)
        .execute(pool)
        .await;
}

// **Pas de nettoyage global de la suite, et c'est une leçon.** Une première
// version supprimait tous les secteurs au motif reconnaissable en début de
// chaque cas, pour effacer ce que les exécutions d'avant avaient laissé. Les
// cas tournent en parallèle : elle effaçait donc les secteurs des cas voisins
// en cours, et deux d'entre eux se sont mis à échouer. Un nettoyage partagé qui
// court avec les tests qu'il sert vaut moins que le résidu qu'il enlève.
//
// Chaque cas range ce qu'il a créé, et les suites voisines ne présument plus du
// contenu global du catalogue — ce qui est de toute façon la bonne façon de les
// écrire depuis que l'exploitation peut le faire grandir.

fn creer(entete: &str, code: &str) -> test::TestRequest {
    test::TestRequest::post()
        .uri("/api/v1/ops/catalog/sectors")
        .insert_header(("Authorization", entete.to_string()))
        .set_json(serde_json::json!({
            "code": code,
            "libelle_fr": "Chauffage",
            "libelle_nl": "Verwarming",
            "libelle_en": "Heating",
            "ordre": 99
        }))
}

fn publier(entete: &str, code: &str) -> test::TestRequest {
    test::TestRequest::post()
        .uri(&format!("/api/v1/ops/catalog/sectors/{code}/publish"))
        .insert_header(("Authorization", entete.to_string()))
}

async fn statut(pool: &PoolPg, code: &str) -> String {
    sqlx::query_scalar("SELECT statut FROM secteur WHERE code = $1")
        .bind(code)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[actix_web::test]
async fn happy_un_secteur_nait_en_brouillon_et_un_autre_compte_le_publie() {
    let pool = pool().await;
    let app = bac!(pool);
    let (createur, s1) = ops(&pool, "SUPER_ADMIN", "createur").await;
    let (validateur, s2) = ops(&pool, "SUPER_ADMIN", "validateur").await;
    let e1 = porteur(&app, &createur, &s1).await;
    let e2 = porteur(&app, &validateur, &s2).await;
    let code = code_secteur();

    let reponse = test::call_service(&app, creer(&e1, &code).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::CREATED);
    // Créer directement publié contournerait la seconde paire d'yeux.
    assert_eq!(statut(&pool, &code).await, "DRAFT");

    let reponse = test::call_service(&app, publier(&e2, &code).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::NO_CONTENT);
    assert_eq!(statut(&pool, &code).await, "PUBLISHED");

    ranger(&pool, &code).await;
}

#[actix_web::test]
async fn security_on_ne_publie_pas_son_propre_brouillon() {
    let pool = pool().await;
    let app = bac!(pool);
    let (moi, secret) = ops(&pool, "SUPER_ADMIN", "solo").await;
    let entete = porteur(&app, &moi, &secret).await;
    let code = code_secteur();

    test::call_service(&app, creer(&entete, &code).to_request()).await;
    let reponse = test::call_service(&app, publier(&entete, &code).to_request()).await;
    // FR-010 `@security` : quatre yeux. Publier son propre brouillon ne serait
    // pas une validation, ce serait un second clic.
    assert_eq!(reponse.status(), StatusCode::FORBIDDEN);
    assert_eq!(statut(&pool, &code).await, "DRAFT");

    ranger(&pool, &code).await;
}

#[actix_web::test]
async fn security_la_base_refuse_qu_un_createur_publie_son_brouillon() {
    let pool = pool().await;
    let app = bac!(pool);
    let (moi, secret) = ops(&pool, "SUPER_ADMIN", "sql-direct").await;
    let entete = porteur(&app, &moi, &secret).await;
    let code = code_secteur();
    test::call_service(&app, creer(&entete, &code).to_request()).await;

    // **Même en écrivant directement.** Une garantie qui ne tient que dans le
    // code s'évapore au premier script de maintenance.
    let ecrasement = sqlx::query(
        "UPDATE secteur SET statut = 'PUBLISHED', publie_par = cree_par, publie_le = now()
          WHERE code = $1",
    )
    .bind(&code)
    .execute(&pool)
    .await;
    assert!(
        ecrasement.is_err(),
        "la contrainte des quatre yeux doit tenir dans la base"
    );

    ranger(&pool, &code).await;
}

#[actix_web::test]
async fn negative_un_code_deja_pris_donne_409() {
    let pool = pool().await;
    let app = bac!(pool);
    let (compte, secret) = ops(&pool, "SUPER_ADMIN", "doublon").await;
    let entete = porteur(&app, &compte, &secret).await;

    // « plomberie » vient du peuplement initial.
    let reponse = test::call_service(&app, creer(&entete, "plomberie").to_request()).await;
    assert_eq!(reponse.status(), StatusCode::CONFLICT);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "SECTOR_CODE_TAKEN");
}

#[actix_web::test]
async fn negative_un_libelle_manquant_est_refuse() {
    let pool = pool().await;
    let app = bac!(pool);
    let (compte, secret) = ops(&pool, "SUPER_ADMIN", "libelle").await;
    let entete = porteur(&app, &compte, &secret).await;

    // Un secteur publié sans néerlandais s'afficherait en français à un
    // néerlandophone, dans une région où c'est précisément ce qu'il ne faut pas
    // faire.
    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/ops/catalog/sectors")
            .insert_header(("Authorization", entete))
            .set_json(serde_json::json!({
                "code": code_secteur(),
                "libelle_fr": "Chauffage",
                "libelle_nl": "   ",
                "libelle_en": "Heating",
                "ordre": 99
            }))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "LABEL_REQUIRED");
}

#[actix_web::test]
async fn negative_un_code_hors_format_est_refuse() {
    let pool = pool().await;
    let app = bac!(pool);
    let (compte, secret) = ops(&pool, "SUPER_ADMIN", "code-invalide").await;
    let entete = porteur(&app, &compte, &secret).await;

    for mauvais in [
        "Chauffage",
        "chauffage gaz",
        "chauffage_gaz",
        "",
        "-chauffage",
    ] {
        let reponse = test::call_service(&app, creer(&entete, mauvais).to_request()).await;
        assert_eq!(
            reponse.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "code accepté à tort : {mauvais:?}"
        );
    }
}

#[actix_web::test]
async fn edge_un_secteur_avec_des_interventions_en_cours_ne_se_retire_pas() {
    let pool = pool().await;
    let app = bac!(pool);
    let (compte, secret) = ops(&pool, "SUPER_ADMIN", "actif").await;
    let entete = porteur(&app, &compte, &secret).await;

    // « plomberie » porte des Missions des autres suites de test. Le retirer
    // doit échouer tant qu'il en reste en cours — et s'il n'en reste aucune, le
    // cas ne prouve rien : on le construit alors.
    let en_cours: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM mission m JOIN demande d ON d.id = m.demande_id
          WHERE d.secteur_code = 'plomberie'
            AND m.statut IN ('ACCEPTED', 'PROVIDER_EN_ROUTE', 'ON_SITE')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    if en_cours > 0 {
        let reponse = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/ops/catalog/sectors/plomberie/disable")
                .insert_header(("Authorization", entete))
                .to_request(),
        )
        .await;
        // FR-010 `@edge` : 409 `SECTOR_HAS_ACTIVE_MISSIONS`.
        assert_eq!(reponse.status(), StatusCode::CONFLICT);
        let corps: Value = test::read_body_json(reponse).await;
        assert_eq!(corps["code"], "SECTOR_HAS_ACTIVE_MISSIONS");
        assert_eq!(statut(&pool, "plomberie").await, "PUBLISHED");
    }
}

#[actix_web::test]
async fn happy_un_secteur_sans_intervention_se_retire() {
    let pool = pool().await;
    let app = bac!(pool);
    let (createur, s1) = ops(&pool, "SUPER_ADMIN", "retrait-a").await;
    let (validateur, s2) = ops(&pool, "SUPER_ADMIN", "retrait-b").await;
    let e1 = porteur(&app, &createur, &s1).await;
    let e2 = porteur(&app, &validateur, &s2).await;
    let code = code_secteur();

    test::call_service(&app, creer(&e1, &code).to_request()).await;
    test::call_service(&app, publier(&e2, &code).to_request()).await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/ops/catalog/sectors/{code}/disable"))
            .insert_header(("Authorization", e1))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::NO_CONTENT);
    // Retiré, pas effacé : les Demandes passées y renvoient.
    assert_eq!(statut(&pool, &code).await, "DISABLED");

    ranger(&pool, &code).await;
}

#[actix_web::test]
async fn security_un_role_sans_droit_ne_touche_pas_au_catalogue() {
    let pool = pool().await;
    let app = bac!(pool);
    // Le médiateur tranche les litiges ; le catalogue ne le regarde pas.
    let (mediateur, secret) = ops(&pool, "MEDIATOR", "sans-droit").await;
    let entete = porteur(&app, &mediateur, &secret).await;

    let reponse = test::call_service(&app, creer(&entete, &code_secteur()).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::FORBIDDEN);

    let refus: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM journal_ops WHERE ops_id = $1 AND geste = 'CATALOG_MANAGE_DENIED'",
    )
    .bind(mediateur.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(refus, 1, "le refus est journalisé");
}

#[actix_web::test]
async fn security_un_brouillon_n_apparait_pas_dans_le_catalogue_public() {
    let pool = pool().await;
    let app = bac!(pool);
    let (compte, secret) = ops(&pool, "SUPER_ADMIN", "invisible").await;
    let entete = porteur(&app, &compte, &secret).await;
    let code = code_secteur();
    test::call_service(&app, creer(&entete, &code).to_request()).await;

    // Un brouillon visible laisserait soumettre des Demandes dans un secteur où
    // aucun prestataire ne s'est encore déclaré.
    let public: Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/catalog/sectors")
            .to_request(),
    )
    .await;
    let brut = public.to_string();
    assert!(
        !brut.contains(&code),
        "le brouillon ne doit pas être proposé au public"
    );

    // Mais l'exploitation, elle, doit le voir : c'est ce qu'elle a à publier.
    let admin: Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/ops/catalog/sectors")
            .insert_header(("Authorization", entete))
            .to_request(),
    )
    .await;
    let mien = admin["secteurs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["code"] == code)
        .expect("le brouillon figure dans la vue d'exploitation");
    assert_eq!(mien["statut"], "DRAFT");
    assert_eq!(mien["cree_par_moi"], true);

    ranger(&pool, &code).await;
}
