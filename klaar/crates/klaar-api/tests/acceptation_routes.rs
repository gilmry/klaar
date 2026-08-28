//! Story 3.4 — acceptation d'une Demande (FR-013), contre un vrai PostgreSQL.
//!
//! La course elle-même se vérifie au niveau du dépôt
//! (`klaar-sqlx-repos/tests/mission.rs`), là où elle se joue. Ces cas-ci
//! vérifient ce que l'API en fait : qui a le droit d'essayer, et quel code
//! chacun reçoit.

use actix_web::{http::StatusCode, test};
use chrono::{Duration, Utc};
use klaar_api::{app_de_test, etat_de_test};
use klaar_catalog::CodeCatalogue;
use klaar_identity::{
    EmpreinteMotDePasse, MotDePasse, NumeroBce, ParametresArgon2, PreuveKyc, Provider,
};
use klaar_shared_kernel::Geo;
use klaar_sqlx_repos::{creer_pool, PgProviderRepository, PoolPg};
use serde_json::Value;
use uuid::Uuid;

use klaar_application::ports::provider_repository::ProviderRepository;

const MDP: &str = "Marie@2026Secure";
/// Grand-Place.
const LAT: f64 = 50.8467;
const LON: f64 = 4.3525;

async fn pool() -> PoolPg {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI");
    creer_pool(&url).await.expect("connexion PostgreSQL")
}

async fn compte_actif(pool: &PoolPg, marqueur: &str) -> (Uuid, String) {
    let id = Uuid::new_v4();
    let email = format!("acc-{marqueur}-{id}@example.eu");
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

/// Numéro BCE construit et tiré au sort, jamais copié d'une entreprise réelle.
fn numero() -> NumeroBce {
    let corps = 1_000_000 + (Uuid::new_v4().as_u128() as u64) % 8_999_999;
    NumeroBce::parse(&format!("{corps:08}{:02}", 97 - (corps % 97))).expect("numéro construit")
}

/// Crée un compte prestataire actif et rend son courriel de connexion.
async fn prestataire(pool: &PoolPg, marqueur: &str, actif: bool) -> (Provider, String) {
    prestataire_du_secteur(pool, marqueur, actif, "plomberie").await
}

async fn prestataire_du_secteur(
    pool: &PoolPg,
    marqueur: &str,
    actif: bool,
    secteur: &str,
) -> (Provider, String) {
    let (utilisateur_id, email) = compte_actif(pool, marqueur).await;
    let mut p = Provider::inscrire(
        utilisateur_id,
        numero(),
        &format!("Prestataire {marqueur}"),
        Geo::new(LAT, LON).unwrap(),
        vec![CodeCatalogue::parse(secteur).unwrap()],
        Utc::now(),
    )
    .expect("prestataire valide");
    if actif {
        p.valider_kyc(PreuveKyc::demonstration(Utc::now()));
    }
    let depot = PgProviderRepository::new(pool.clone());
    depot.creer(&p).await.expect("création");
    if actif {
        depot
            .definir_disponibilite(p.id, true)
            .await
            .expect("disponibilité");
    }
    (p, email)
}

/// Insère une Demande diffusée directement, avec l'âge voulu.
///
/// Passer par l'API imposerait de dater la Demande à `now`, alors que le cas
/// de l'expiration a besoin d'un tour déjà écoulé. Le SQL est ici le chemin
/// honnête : il pose l'état de départ sans mimer une attente de trente
/// secondes que personne ne veut voir dans une suite de tests.
async fn demande_diffusee(pool: &PoolPg, secondes: i64) -> Uuid {
    let (demandeur_id, _) = compte_actif(pool, "demandeur").await;
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO demande
             (id, demandeur_id, secteur_code, description, position, urgence, statut,
              diffuse_depuis, cree_le)
         VALUES ($1, $2, 'plomberie', 'Fuite sous l''évier',
                 ST_SetSRID(ST_MakePoint($3, $4), 4326)::geography, 'HIGH', 'BROADCASTING',
                 $5, $5)",
    )
    .bind(id)
    .bind(demandeur_id)
    .bind(LON)
    .bind(LAT)
    .bind(Utc::now() - Duration::seconds(secondes))
    .execute(pool)
    .await
    .expect("Demande de test");
    id
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

fn accept(jeton: &str, demande_id: &str) -> test::TestRequest {
    test::TestRequest::post()
        .uri(&format!("/api/v1/requests/{demande_id}/accept"))
        .insert_header(("Authorization", format!("Bearer {jeton}")))
}

#[actix_web::test]
async fn happy_le_premier_prestataire_obtient_la_mission() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = prestataire(&pool, "gagnant", true).await;
    let jeton = jeton(&app, &email).await;
    let demande_id = demande_diffusee(&pool, 0).await;

    let reponse =
        test::call_service(&app, accept(&jeton, &demande_id.to_string()).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::CREATED);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "MATCH_ACCEPTED");
    assert_eq!(corps["statut"], "ACCEPTED");
    assert_eq!(corps["demande_id"], demande_id.to_string());
    // Sans clé VAPID dans ce bac, personne n'est prévenu : c'est un mode de
    // fonctionnement légitime et le champ le dit plutôt que de le taire.
    assert_eq!(corps["autres_prevenus"], 0);
}

#[actix_web::test]
async fn negative_un_compte_sans_fiche_prestataire_recoit_403() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "simple-usager").await;
    let jeton = jeton(&app, &email).await;
    let demande_id = demande_diffusee(&pool, 0).await;

    let reponse =
        test::call_service(&app, accept(&jeton, &demande_id.to_string()).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::FORBIDDEN);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "PROVIDER_NOT_ELIGIBLE");
}

#[actix_web::test]
async fn negative_un_prestataire_non_valide_recoit_403() {
    // Contrôle au moment d'accepter, pas au matching (FR-013 `@security`).
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = prestataire(&pool, "en-attente", false).await;
    let jeton = jeton(&app, &email).await;
    let demande_id = demande_diffusee(&pool, 0).await;

    let reponse =
        test::call_service(&app, accept(&jeton, &demande_id.to_string()).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn negative_une_demande_deja_prise_recoit_409() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, premier) = prestataire(&pool, "premier", true).await;
    let (_, tardif) = prestataire(&pool, "tardif", true).await;
    let demande_id = demande_diffusee(&pool, 0).await.to_string();

    let j1 = jeton(&app, &premier).await;
    let j2 = jeton(&app, &tardif).await;
    test::call_service(&app, accept(&j1, &demande_id).to_request()).await;

    let reponse = test::call_service(&app, accept(&j2, &demande_id).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::CONFLICT);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "REQUEST_ALREADY_MATCHED");
}

#[actix_web::test]
async fn negative_un_prestataire_deja_en_mission_recoit_409() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = prestataire(&pool, "occupe", true).await;
    let jeton = jeton(&app, &email).await;
    let premiere = demande_diffusee(&pool, 0).await.to_string();
    let seconde = demande_diffusee(&pool, 0).await.to_string();

    test::call_service(&app, accept(&jeton, &premiere).to_request()).await;
    let reponse = test::call_service(&app, accept(&jeton, &seconde).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::CONFLICT);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "PROVIDER_BUSY");
}

#[actix_web::test]
async fn negative_un_prestataire_d_un_autre_secteur_recoit_403() {
    // La route est ouverte à tout prestataire actif : sans ce contrôle, un
    // serrurier qui connaît l'identifiant d'une Demande de plomberie pourrait
    // la rafler, et le demandeur verrait arriver quelqu'un qui ne sait pas la
    // réparer.
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = prestataire_du_secteur(&pool, "serrurier", true, "serrurerie").await;
    let jeton = jeton(&app, &email).await;
    let demande_id = demande_diffusee(&pool, 0).await;

    let reponse =
        test::call_service(&app, accept(&jeton, &demande_id.to_string()).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::FORBIDDEN);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "PROVIDER_NOT_ELIGIBLE");
}

#[actix_web::test]
async fn security_un_refus_hors_secteur_laisse_la_demande_prenable() {
    // Le contrôle vient avant l'attribution : une tentative hors secteur ne
    // doit pas éteindre une Demande que le bon prestataire pourra prendre.
    let pool = pool().await;
    let app = bac!(pool);
    let (_, serrurier) = prestataire_du_secteur(&pool, "serrurier-2", true, "serrurerie").await;
    let (_, plombier) = prestataire(&pool, "plombier-apres", true).await;
    let demande_id = demande_diffusee(&pool, 0).await.to_string();

    let js = jeton(&app, &serrurier).await;
    let jp = jeton(&app, &plombier).await;
    test::call_service(&app, accept(&js, &demande_id).to_request()).await;

    let reponse = test::call_service(&app, accept(&jp, &demande_id).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::CREATED);
}

#[actix_web::test]
async fn negative_une_demande_retiree_par_son_auteur_recoit_410() {
    // FR-014 `@edge` : quand l'annulation gagne la course, le prestataire
    // reçoit un 410. La Demande a existé et n'existe plus ; ce n'est pas
    // « quelqu'un d'autre l'a », qui serait un 409.
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = prestataire(&pool, "sur-annulee", true).await;
    let jeton = jeton(&app, &email).await;
    let demande_id = demande_diffusee(&pool, 0).await;
    sqlx::query("UPDATE demande SET statut = 'CANCELLED' WHERE id = $1")
        .bind(demande_id)
        .execute(&pool)
        .await
        .unwrap();

    let reponse =
        test::call_service(&app, accept(&jeton, &demande_id.to_string()).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::GONE);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "REQUEST_CANCELLED");
}

#[actix_web::test]
async fn negative_une_demande_sans_reponse_recoit_410() {
    // FR-015 `@edge` : l'accept tardif est rejeté en 410, pas en 409 — le tour
    // s'est terminé sans personne, il n'y a pas de concurrent à aller chercher.
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = prestataire(&pool, "sur-nomatch", true).await;
    let jeton = jeton(&app, &email).await;
    let demande_id = demande_diffusee(&pool, 0).await;
    sqlx::query("UPDATE demande SET statut = 'NO_MATCH' WHERE id = $1")
        .bind(demande_id)
        .execute(&pool)
        .await
        .unwrap();

    let reponse =
        test::call_service(&app, accept(&jeton, &demande_id.to_string()).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::GONE);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "REQUEST_EXPIRED");
}

#[actix_web::test]
async fn negative_une_demande_inconnue_recoit_404() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = prestataire(&pool, "chercheur", true).await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        accept(&jeton, &Uuid::new_v4().to_string()).to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn negative_un_identifiant_illisible_recoit_400() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = prestataire(&pool, "saisie", true).await;
    let jeton = jeton(&app, &email).await;

    for illisible in ["pas-un-uuid", "1", "0000", "%00"] {
        let reponse = test::call_service(&app, accept(&jeton, illisible).to_request()).await;
        assert_eq!(
            reponse.status(),
            StatusCode::BAD_REQUEST,
            "identifiant {illisible}"
        );
    }
}

#[actix_web::test]
async fn security_une_traversee_de_chemin_ne_touche_jamais_le_handler() {
    // `../../etc/passwd` porte des barres obliques : il ne correspond à aucune
    // route, et c'est le routeur qui répond 404 — le handler n'est jamais
    // appelé. Ce test le fixe pour que le jour où quelqu'un remplace le
    // segment par un `Path<PathBuf>` ou une route à joker, l'échec se voie.
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = prestataire(&pool, "traversee", true).await;
    let jeton = jeton(&app, &email).await;

    for hostile in ["../../etc/passwd", "..%2F..%2Fetc%2Fpasswd", "a/b"] {
        let reponse = test::call_service(&app, accept(&jeton, hostile).to_request()).await;
        assert!(
            reponse.status() == StatusCode::NOT_FOUND
                || reponse.status() == StatusCode::BAD_REQUEST,
            "chemin {hostile} : statut {}",
            reponse.status()
        );
    }
}

#[actix_web::test]
async fn edge_un_tour_de_diffusion_ecoule_recoit_410() {
    // Passé trente secondes, le demandeur s'est vu proposer d'élargir ou
    // d'annuler (FR-015) : l'accept tardif arrive après cette bifurcation.
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = prestataire(&pool, "tardif-410", true).await;
    let jeton = jeton(&app, &email).await;
    let demande_id = demande_diffusee(&pool, 40).await;

    let reponse =
        test::call_service(&app, accept(&jeton, &demande_id.to_string()).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::GONE);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "REQUEST_EXPIRED");
}

#[actix_web::test]
async fn security_sans_jeton_l_acceptation_est_refusee() {
    let pool = pool().await;
    let app = bac!(pool);
    let demande_id = demande_diffusee(&pool, 0).await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/requests/{demande_id}/accept"))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn security_le_martelement_est_borne_a_cinq_par_seconde() {
    // FR-013 `@security`. Le quota est compté par compte et non par adresse :
    // une flotte derrière une seule sortie NAT ne doit pas s'épuiser
    // mutuellement.
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = prestataire(&pool, "spammeur", true).await;
    let jeton = jeton(&app, &email).await;

    let mut refuse = false;
    for _ in 0..8 {
        let reponse = test::call_service(
            &app,
            accept(&jeton, &Uuid::new_v4().to_string()).to_request(),
        )
        .await;
        if reponse.status() == StatusCode::TOO_MANY_REQUESTS {
            assert!(
                reponse.headers().contains_key("retry-after"),
                "un refus doit dire quand réessayer"
            );
            refuse = true;
            break;
        }
    }
    assert!(refuse, "huit tentatives d'affilée doivent finir refusées");
}

#[actix_web::test]
async fn security_le_quota_d_un_prestataire_n_epuise_pas_celui_d_un_autre() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, bruyant) = prestataire(&pool, "bruyant", true).await;
    let (_, discret) = prestataire(&pool, "discret", true).await;
    let jb = jeton(&app, &bruyant).await;
    let jd = jeton(&app, &discret).await;

    for _ in 0..8 {
        test::call_service(&app, accept(&jb, &Uuid::new_v4().to_string()).to_request()).await;
    }
    let demande_id = demande_diffusee(&pool, 0).await;
    let reponse = test::call_service(&app, accept(&jd, &demande_id.to_string()).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::CREATED);
}
