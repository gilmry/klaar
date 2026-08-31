//! Story 4.4 — suivi géolocalisé du trajet (FR-019), contre un vrai PostgreSQL.
//!
//! **Ce qui ne se teste qu'ici :** que la position écrite en base soit celle
//! dégradée et non celle envoyée, que le demandeur d'à côté ne voie rien, et
//! que la purge remplace les positions par une distance sans jamais laisser les
//! deux coexister.

use actix_web::{http::StatusCode, test};
use chrono::{Duration, Utc};
use klaar_api::{app_de_test, etat_de_test};
use klaar_catalog::CodeCatalogue;
use klaar_identity::{
    EmpreinteMotDePasse, MotDePasse, NumeroBce, ParametresArgon2, PreuveKyc, Provider,
};
use klaar_intervention::{PAS_LATITUDE, PAS_LONGITUDE};
use klaar_shared_kernel::Geo;
use klaar_sqlx_repos::{creer_pool, PgProviderRepository, PoolPg};
use serde_json::Value;
use uuid::Uuid;

use klaar_application::ports::provider_repository::ProviderRepository;

const MDP: &str = "Marie@2026Secure";
const LAT: f64 = 50.8467;
const LON: f64 = 4.3525;

async fn pool() -> PoolPg {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI");
    creer_pool(&url).await.expect("connexion PostgreSQL")
}

async fn compte_actif(pool: &PoolPg, marqueur: &str) -> (Uuid, String) {
    let id = Uuid::new_v4();
    let email = format!("sui-{marqueur}-{id}@example.eu");
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

/// Un numéro d'entreprise encore libre en base.
///
/// **Pourquoi interroger la base plutôt que tirer et espérer.** Le format n'offre
/// que vingt millions de corps possibles, et la base de développement n'est
/// jamais purgée : les prestataires des exécutions précédentes s'y accumulent.
/// Passé quelques milliers de lignes, un tirage finissait par retomber sur un
/// numéro déjà pris et le test échouait sur `provider_numero_bce_key` — un échec
/// sans rapport avec ce qu'il vérifie, et **d'autant plus fréquent que la base
/// grossit**. Observé une fois sur deux exécutions complètes à onze mille
/// prestataires en base ; ce n'était donc pas de la malchance, mais une dette
/// qui se paie de plus en plus cher.
///
/// Il reste une fenêtre entre la vérification et l'insertion, deux binaires de
/// test tournant en parallèle. Elle est de l'ordre du vingt-millionième, contre
/// un millième pour le tirage aveugle : c'est le rapport qui compte, pas la
/// perfection.
async fn numero(pool: &PoolPg) -> NumeroBce {
    for _ in 0..64 {
        let corps = (Uuid::new_v4().as_u128() as u64) % 20_000_000;
        let candidat = format!("{corps:08}{:02}", 97 - (corps % 97));
        let pris: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM provider WHERE numero_bce = $1)")
                .bind(&candidat)
                .fetch_one(pool)
                .await
                .expect("recherche d'un numéro libre");
        if !pris {
            return NumeroBce::parse(&candidat).expect("numéro construit");
        }
    }
    panic!("aucun numéro d'entreprise libre en soixante-quatre tirages : purger la base de test");
}

async fn prestataire(pool: &PoolPg, marqueur: &str) -> (Provider, String) {
    let (utilisateur_id, email) = compte_actif(pool, marqueur).await;
    let mut p = Provider::inscrire(
        utilisateur_id,
        numero(pool).await,
        &format!("Prestataire {marqueur}"),
        Geo::new(LAT, LON).unwrap(),
        vec![CodeCatalogue::parse("plomberie").unwrap()],
        Utc::now(),
    )
    .expect("prestataire valide");
    p.valider_kyc(PreuveKyc::demonstration(Utc::now()));
    PgProviderRepository::new(pool.clone())
        .creer(&p)
        .await
        .expect("création");
    (p, email)
}

/// Une Mission dans l'état voulu, avec son demandeur. Rend (mission, email).
async fn mission(pool: &PoolPg, provider_id: Uuid, statut: &str) -> (Uuid, String) {
    let (demandeur_id, email) = compte_actif(pool, "demandeur").await;
    let demande_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO demande
             (id, demandeur_id, secteur_code, description, position, urgence, statut,
              rayon_metres, elargissements, diffuse_depuis, cree_le)
         VALUES ($1, $2, 'plomberie', 'Fuite', ST_SetSRID(ST_MakePoint($3, $4), 4326)::geography,
                 'HIGH', 'MATCHED', 5000, 0, now(), now())",
    )
    .bind(demande_id)
    .bind(demandeur_id)
    .bind(LON)
    .bind(LAT)
    .execute(pool)
    .await
    .expect("Demande attribuée");

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO mission (id, demande_id, provider_id, statut, cree_le)
         VALUES ($1, $2, $3, $4, now())",
    )
    .bind(id)
    .bind(demande_id)
    .bind(provider_id)
    .bind(statut)
    .execute(pool)
    .await
    .expect("Mission");
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

fn consentir(jeton: &str, mission_id: Uuid, accepte: bool) -> test::TestRequest {
    test::TestRequest::post()
        .uri(&format!("/api/v1/missions/{mission_id}/tracking/consent"))
        .insert_header(("Authorization", format!("Bearer {jeton}")))
        .set_json(serde_json::json!({ "accepte": accepte }))
}

fn relever(jeton: &str, mission_id: Uuid, lat: f64, lon: f64) -> test::TestRequest {
    test::TestRequest::post()
        .uri(&format!("/api/v1/missions/{mission_id}/tracking"))
        .insert_header(("Authorization", format!("Bearer {jeton}")))
        .set_json(serde_json::json!({ "lat": lat, "lon": lon }))
}

fn consulter(jeton: &str, mission_id: Uuid) -> test::TestRequest {
    test::TestRequest::get()
        .uri(&format!("/api/v1/missions/{mission_id}/tracking"))
        .insert_header(("Authorization", format!("Bearer {jeton}")))
}

/// Pose une transition terminale datée, pour amener la purge à échéance.
async fn terminee_il_y_a(pool: &PoolPg, mission_id: Uuid, provider_id: Uuid, heures: i64) {
    let quand = Utc::now() - Duration::hours(heures);
    sqlx::query(
        "INSERT INTO mission_transition
             (mission_id, provider_id, statut, horodate_le, enregistre_le)
         VALUES ($1, $2, 'COMPLETED', $3, $3)",
    )
    .bind(mission_id)
    .bind(provider_id)
    .bind(quand)
    .execute(pool)
    .await
    .expect("transition terminale");
}

#[actix_web::test]
async fn happy_le_prestataire_consent_puis_partage_et_le_demandeur_voit() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "happy").await;
    let (mission_id, email_d) = mission(&pool, p.id, "PROVIDER_EN_ROUTE").await;
    let jp = jeton(&app, &email_p).await;
    let jd = jeton(&app, &email_d).await;

    // Avant tout partage, le demandeur n'a pas une carte vide : il a un état.
    let vue: Value =
        test::call_and_read_body_json(&app, consulter(&jd, mission_id).to_request()).await;
    assert_eq!(
        vue["etat"], "POSITION_LOST",
        "sans relevé, l'écran doit le dire plutôt que rester muet"
    );
    assert!(vue["position"].is_null(), "rien n'a encore été partagé");

    let reponse = test::call_service(&app, consentir(&jp, mission_id, true).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::OK);

    let reponse = test::call_service(&app, relever(&jp, mission_id, LAT, LON).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::CREATED);

    let vue: Value =
        test::call_and_read_body_json(&app, consulter(&jd, mission_id).to_request()).await;
    assert_eq!(vue["etat"], "EN_ROUTE");
    assert!(
        !vue["position"].is_null(),
        "la position partagée doit être visible du demandeur"
    );
}

#[actix_web::test]
async fn security_la_position_stockee_est_la_position_degradee() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "degrade").await;
    let (mission_id, _) = mission(&pool, p.id, "PROVIDER_EN_ROUTE").await;
    let jp = jeton(&app, &email_p).await;
    test::call_service(&app, consentir(&jp, mission_id, true).to_request()).await;

    // Une position volontairement précise, au dix-millionième de degré.
    let lat = 50.846_712_3;
    let lon = 4.352_589_7;
    let corps: Value =
        test::call_and_read_body_json(&app, relever(&jp, mission_id, lat, lon).to_request()).await;

    let rendue_lat = corps["lat"].as_f64().unwrap();
    assert_ne!(
        rendue_lat, lat,
        "la position rendue doit être la dégradée, sinon la grille n'est qu'une promesse"
    );

    // **La base ne doit pas contenir la position d'origine.** C'est le seul
    // endroit où la vérifier : dégrader à l'affichage laisserait la donnée fine
    // ici, là où une fuite la prendrait.
    let (stock_lat, stock_lon): (f64, f64) = sqlx::query_as(
        "SELECT ST_Y(position::geometry), ST_X(position::geometry)
         FROM position_suivi WHERE mission_id = $1",
    )
    .bind(mission_id)
    .fetch_one(&pool)
    .await
    .expect("position stockée");

    assert_eq!(stock_lat, rendue_lat, "la base porte la position dégradée");
    assert_eq!(stock_lon, corps["lon"].as_f64().unwrap());

    // Et l'écart reste dans la maille annoncée : dégrader ne veut pas dire
    // déplacer ailleurs. Un arrondi qui déporterait de plusieurs centaines de
    // mètres rendrait le suivi inutile tout en donnant l'illusion de marcher.
    assert!(
        (stock_lat - lat).abs() <= PAS_LATITUDE,
        "la latitude dégradée doit rester dans sa maille"
    );
    assert!(
        (stock_lon - lon).abs() <= PAS_LONGITUDE,
        "la longitude dégradée doit rester dans sa maille"
    );
}

#[actix_web::test]
async fn security_sans_consentement_la_position_n_est_pas_ecrite() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "sans-accord").await;
    let (mission_id, _) = mission(&pool, p.id, "PROVIDER_EN_ROUTE").await;
    let jp = jeton(&app, &email_p).await;

    let reponse = test::call_service(&app, relever(&jp, mission_id, LAT, LON).to_request()).await;
    assert_eq!(
        reponse.status(),
        StatusCode::FORBIDDEN,
        "un refus de partage n'est pas une donnée invalide, c'est un droit"
    );

    let compte: i64 =
        sqlx::query_scalar("SELECT count(*) FROM position_suivi WHERE mission_id = $1")
            .bind(mission_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(compte, 0, "aucune trace ne doit rester d'un envoi refusé");
}

#[actix_web::test]
async fn security_le_retrait_du_consentement_arrete_les_relevés_suivants() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "retrait").await;
    let (mission_id, _) = mission(&pool, p.id, "PROVIDER_EN_ROUTE").await;
    let jp = jeton(&app, &email_p).await;

    test::call_service(&app, consentir(&jp, mission_id, true).to_request()).await;
    test::call_service(&app, relever(&jp, mission_id, LAT, LON).to_request()).await;

    let corps: Value =
        test::call_and_read_body_json(&app, consentir(&jp, mission_id, false).to_request()).await;
    assert_eq!(corps["code"], "TRACKING_WITHDRAWN");

    let reponse = test::call_service(&app, relever(&jp, mission_id, LAT, LON).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::FORBIDDEN);

    // Le relevé d'avant reste : il a été partagé de plein gré, le demandeur l'a
    // vu, et l'effacer rétroactivement lui retirerait ce sur quoi il s'est
    // organisé. Il partira avec la purge, comme les autres.
    let compte: i64 =
        sqlx::query_scalar("SELECT count(*) FROM position_suivi WHERE mission_id = $1")
            .bind(mission_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        compte, 1,
        "le retrait vaut pour la suite, pas pour le passé"
    );

    // Et la preuve que le consentement avait été donné n'est pas effacée.
    let (consenti, retire): (Option<chrono::DateTime<Utc>>, Option<chrono::DateTime<Utc>>) =
        sqlx::query_as(
            "SELECT consenti_le, retire_le FROM consentement_suivi WHERE mission_id = $1",
        )
        .bind(mission_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(consenti.is_some() && retire.is_some());
}

#[actix_web::test]
async fn negative_hors_trajet_la_position_est_refusee() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "hors-trajet").await;
    // `ON_SITE` : arrivé. Le trajet est fini, il n'y a plus rien à suivre.
    let (mission_id, _) = mission(&pool, p.id, "ON_SITE").await;
    let jp = jeton(&app, &email_p).await;
    test::call_service(&app, consentir(&jp, mission_id, true).to_request()).await;

    let reponse = test::call_service(&app, relever(&jp, mission_id, LAT, LON).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let compte: i64 =
        sqlx::query_scalar("SELECT count(*) FROM position_suivi WHERE mission_id = $1")
            .bind(mission_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        compte, 0,
        "le suivi hors trajet est impossible, pas seulement interdit"
    );
}

#[actix_web::test]
async fn security_un_tiers_ne_voit_pas_le_trajet() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "tiers").await;
    let (mission_id, _) = mission(&pool, p.id, "PROVIDER_EN_ROUTE").await;
    let jp = jeton(&app, &email_p).await;
    test::call_service(&app, consentir(&jp, mission_id, true).to_request()).await;
    test::call_service(&app, relever(&jp, mission_id, LAT, LON).to_request()).await;

    let (_, email_curieux) = compte_actif(&pool, "curieux").await;
    let jc = jeton(&app, &email_curieux).await;
    let reponse = test::call_service(&app, consulter(&jc, mission_id).to_request()).await;
    assert_eq!(
        reponse.status(),
        StatusCode::NOT_FOUND,
        "un compte étranger à l'intervention ne doit rien apprendre, pas même qu'elle existe"
    );

    // Et le prestataire lui-même n'a pas de vue demandeur : ces deux droits ne
    // se confondent pas.
    let reponse = test::call_service(&app, consulter(&jp, mission_id).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn edge_apres_l_arrivee_la_derniere_position_n_est_plus_montree() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "arrivee").await;
    let (mission_id, email_d) = mission(&pool, p.id, "PROVIDER_EN_ROUTE").await;
    let jp = jeton(&app, &email_p).await;
    let jd = jeton(&app, &email_d).await;
    test::call_service(&app, consentir(&jp, mission_id, true).to_request()).await;
    test::call_service(&app, relever(&jp, mission_id, LAT, LON).to_request()).await;

    sqlx::query("UPDATE mission SET statut = 'ON_SITE' WHERE id = $1")
        .bind(mission_id)
        .execute(&pool)
        .await
        .unwrap();

    let vue: Value =
        test::call_and_read_body_json(&app, consulter(&jd, mission_id).to_request()).await;
    assert_eq!(vue["etat"], "STOPPED");
    assert!(
        vue["position"].is_null(),
        "une fois arrivé, il n'y a plus de raison de sortir la position de la base"
    );
}

#[actix_web::test]
async fn security_la_purge_remplace_les_positions_par_une_distance() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "purge").await;
    let (mission_id, _) = mission(&pool, p.id, "PROVIDER_EN_ROUTE").await;
    let jp = jeton(&app, &email_p).await;
    test::call_service(&app, consentir(&jp, mission_id, true).to_request()).await;

    // Trois points espacés d'environ deux cents mètres, pour que la distance
    // agrégée soit mesurable et non nulle.
    for pas in 0..3 {
        let lat = LAT + f64::from(pas) * 0.002;
        test::call_service(&app, relever(&jp, mission_id, lat, LON).to_request()).await;
    }
    terminee_il_y_a(&pool, mission_id, p.id, 25).await;

    let suivis = klaar_sqlx_repos::PgSuiviRepository::new(pool.clone());
    let purgees = klaar_application::usecases::suivre_position::purger_les_traces(
        &suivis,
        &klaar_application::ports::horloge::HorlogeSysteme,
    )
    .await
    .expect("purge");
    assert!(purgees >= 1, "l'intervention échue devait être purgée");

    let restantes: i64 =
        sqlx::query_scalar("SELECT count(*) FROM position_suivi WHERE mission_id = $1")
            .bind(mission_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        restantes, 0,
        "aucune position ne doit survivre à l'échéance"
    );

    let (distance, releves): (f64, i32) =
        sqlx::query_as("SELECT distance_metres, releves FROM trajet_agrege WHERE mission_id = $1")
            .bind(mission_id)
            .fetch_one(&pool)
            .await
            .expect("trajet agrégé");
    assert_eq!(releves, 3);
    assert!(
        distance > 100.0,
        "la mesure du déplacement doit survivre à l'effacement du chemin ({distance:.0} m)"
    );
}

#[actix_web::test]
async fn edge_la_purge_epargne_une_intervention_finie_depuis_moins_de_vingt_quatre_heures() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "recente").await;
    let (mission_id, _) = mission(&pool, p.id, "PROVIDER_EN_ROUTE").await;
    let jp = jeton(&app, &email_p).await;
    test::call_service(&app, consentir(&jp, mission_id, true).to_request()).await;
    test::call_service(&app, relever(&jp, mission_id, LAT, LON).to_request()).await;
    terminee_il_y_a(&pool, mission_id, p.id, 23).await;

    let suivis = klaar_sqlx_repos::PgSuiviRepository::new(pool.clone());
    klaar_application::usecases::suivre_position::purger_les_traces(
        &suivis,
        &klaar_application::ports::horloge::HorlogeSysteme,
    )
    .await
    .expect("purge");

    // Assertion locale à cette intervention : d'autres purges peuvent tourner
    // en parallèle sur la même base, et compter globalement rendrait ce test
    // dépendant de ce que font les autres.
    let restantes: i64 =
        sqlx::query_scalar("SELECT count(*) FROM position_suivi WHERE mission_id = $1")
            .bind(mission_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        restantes, 1,
        "la fenêtre de vingt-quatre heures doit être tenue"
    );
}
