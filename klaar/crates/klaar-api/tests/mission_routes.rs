//! Story 4.3 — cycle de vie d'une Mission (FR-018), contre un vrai PostgreSQL.
//!
//! L'historique est append-only et écrit dans la même transaction que la
//! bascule : ni le domaine ni un double en mémoire ne diraient si le
//! déclencheur tient ni si les deux écritures vont bien ensemble.

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
    let email = format!("mis-{marqueur}-{id}@example.eu");
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

fn numero() -> NumeroBce {
    let corps = (Uuid::new_v4().as_u128() as u64) % 20_000_000;
    NumeroBce::parse(&format!("{corps:08}{:02}", 97 - (corps % 97))).expect("numéro construit")
}

async fn prestataire(pool: &PoolPg, marqueur: &str) -> (Provider, String) {
    let (utilisateur_id, email) = compte_actif(pool, marqueur).await;
    let mut p = Provider::inscrire(
        utilisateur_id,
        numero(),
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

/// Pose une Demande attribuée et sa Mission, dans l'état voulu.
async fn mission(pool: &PoolPg, provider_id: Uuid, statut: &str) -> Uuid {
    let (demandeur_id, _) = compte_actif(pool, "demandeur").await;
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

fn avancer(jeton: &str, id: Uuid, corps: Value) -> test::TestRequest {
    test::TestRequest::patch()
        .uri(&format!("/api/v1/missions/{id}/status"))
        .insert_header(("Authorization", format!("Bearer {jeton}")))
        .set_json(corps)
}

async fn transitions(pool: &PoolPg, mission_id: Uuid) -> Vec<String> {
    sqlx::query_scalar("SELECT statut FROM mission_transition WHERE mission_id = $1 ORDER BY id")
        .bind(mission_id)
        .fetch_all(pool)
        .await
        .expect("historique")
}

#[actix_web::test]
async fn happy_le_parcours_nominal_va_jusqu_au_bout() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "parcours").await;
    let jeton = jeton(&app, &email).await;
    let id = mission(&pool, p.id, "ACCEPTED").await;

    for cible in ["PROVIDER_EN_ROUTE", "ON_SITE", "COMPLETED"] {
        let reponse = test::call_service(
            &app,
            avancer(
                &jeton,
                id,
                serde_json::json!({ "statut": cible, "latitude": LAT, "longitude": LON }),
            )
            .to_request(),
        )
        .await;
        assert_eq!(reponse.status(), StatusCode::OK, "étape {cible}");
        let corps: Value = test::read_body_json(reponse).await;
        assert_eq!(corps["statut"], cible);
        assert_eq!(corps["code"], "MISSION_STATUS_CHANGED");
        assert_eq!(corps["hors_zone"], false);
    }

    // FR-018 `@security` : une entrée d'historique par transition.
    assert_eq!(
        transitions(&pool, id).await,
        vec!["PROVIDER_EN_ROUTE", "ON_SITE", "COMPLETED"]
    );
}

#[actix_web::test]
async fn negative_les_transitions_interdites_du_fr_recoivent_409() {
    // FR-018 `@negative`, repris tel quel. Chaque cas a son propre prestataire
    // attribué : réutiliser un jeton étranger ferait répondre 404 avant même
    // que la transition soit examinée, et le cas ne prouverait plus rien.
    let pool = pool().await;
    let app = bac!(pool);

    // Le marqueur est un rang et non le nom des statuts : ceux-ci portent des
    // tirets bas, que l'adresse construite ne repasse pas à la connexion.
    for (rang, (depuis, vers)) in [
        ("COMPLETED", "PROVIDER_EN_ROUTE"),
        ("ON_SITE", "ACCEPTED"),
        ("CANCELLED", "ON_SITE"),
    ]
    .into_iter()
    .enumerate()
    {
        let (p, email) = prestataire(&pool, &format!("interdite-{rang}")).await;
        let jeton = jeton(&app, &email).await;
        let id = mission(&pool, p.id, depuis).await;

        let reponse = test::call_service(
            &app,
            avancer(&jeton, id, serde_json::json!({ "statut": vers })).to_request(),
        )
        .await;
        assert_eq!(reponse.status(), StatusCode::CONFLICT, "{depuis} → {vers}");
        let corps: Value = test::read_body_json(reponse).await;
        assert_eq!(corps["code"], "INVALID_TRANSITION", "{depuis} → {vers}");
        // Et rien n'a été consigné : l'historique raconte ce qui s'est passé,
        // pas ce qui a été tenté.
        assert!(transitions(&pool, id).await.is_empty(), "{depuis} → {vers}");
    }
}

#[actix_web::test]
async fn negative_une_transition_interdite_sur_sa_propre_mission_recoit_409() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "sienne").await;
    let jeton = jeton(&app, &email).await;
    let id = mission(&pool, p.id, "COMPLETED").await;

    let reponse = test::call_service(
        &app,
        avancer(
            &jeton,
            id,
            serde_json::json!({ "statut": "PROVIDER_EN_ROUTE" }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::CONFLICT);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "INVALID_TRANSITION");
    assert!(transitions(&pool, id).await.is_empty());
}

#[actix_web::test]
async fn negative_un_statut_inconnu_recoit_400() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "vocabulaire").await;
    let jeton = jeton(&app, &email).await;
    let id = mission(&pool, p.id, "ACCEPTED").await;

    for inconnu in ["EN_ROUTE", "assigned", "DONE", ""] {
        let reponse = test::call_service(
            &app,
            avancer(&jeton, id, serde_json::json!({ "statut": inconnu })).to_request(),
        )
        .await;
        assert_eq!(
            reponse.status(),
            StatusCode::BAD_REQUEST,
            "statut {inconnu}"
        );
    }
}

#[actix_web::test]
async fn negative_un_horodatage_trop_ancien_recoit_400() {
    // FR-018 `@edge` : au-delà de cinq minutes, une intervention pourrait se
    // prétendre commencée une heure plus tôt.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "horodatage").await;
    let jeton = jeton(&app, &email).await;
    let id = mission(&pool, p.id, "ACCEPTED").await;

    let reponse = test::call_service(
        &app,
        avancer(
            &jeton,
            id,
            serde_json::json!({
                "statut": "PROVIDER_EN_ROUTE",
                "horodate_le": (Utc::now() - Duration::hours(2)).to_rfc3339(),
            }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::BAD_REQUEST);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "TIMESTAMP_IMPLAUSIBLE");
    assert!(transitions(&pool, id).await.is_empty());
}

#[actix_web::test]
async fn edge_un_horodatage_dans_la_tolerance_est_conserve() {
    // C'est tout l'intérêt : une transition faite hors connexion garde sa date.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "hors-ligne").await;
    let jeton = jeton(&app, &email).await;
    let id = mission(&pool, p.id, "ACCEPTED").await;
    let annonce = Utc::now() - Duration::minutes(3);

    let reponse = test::call_service(
        &app,
        avancer(
            &jeton,
            id,
            serde_json::json!({
                "statut": "PROVIDER_EN_ROUTE",
                "horodate_le": annonce.to_rfc3339(),
            }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::OK);

    let (horodate, enregistre): (chrono::DateTime<Utc>, chrono::DateTime<Utc>) = sqlx::query_as(
        "SELECT horodate_le, enregistre_le FROM mission_transition WHERE mission_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(horodate.timestamp(), annonce.timestamp());
    assert!(enregistre > horodate, "le serveur date sa réception à part");
}

#[actix_web::test]
async fn edge_une_position_hors_region_passe_et_est_marquee() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "hors-zone").await;
    let jeton = jeton(&app, &email).await;
    let id = mission(&pool, p.id, "ACCEPTED").await;

    let reponse = test::call_service(
        &app,
        avancer(
            &jeton,
            id,
            // Anvers.
            serde_json::json!({ "statut": "PROVIDER_EN_ROUTE", "latitude": 51.2194, "longitude": 4.4025 }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["hors_zone"], true);
    assert_eq!(corps["statut"], "PROVIDER_EN_ROUTE");
}

#[actix_web::test]
async fn edge_sans_position_la_transition_passe_quand_meme() {
    // Exiger la position rendrait l'autorisation de géolocalisation de fait
    // obligatoire.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "sans-gps").await;
    let jeton = jeton(&app, &email).await;
    let id = mission(&pool, p.id, "ACCEPTED").await;

    let reponse = test::call_service(
        &app,
        avancer(
            &jeton,
            id,
            serde_json::json!({ "statut": "PROVIDER_EN_ROUTE" }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["hors_zone"], false);
}

#[actix_web::test]
async fn negative_une_position_incomplete_est_refusee() {
    // N'en donner qu'une est une erreur de client ; la traiter comme « pas de
    // position » masquerait un bogue dont personne ne verrait la trace.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "moitie").await;
    let jeton = jeton(&app, &email).await;
    let id = mission(&pool, p.id, "ACCEPTED").await;

    let reponse = test::call_service(
        &app,
        avancer(
            &jeton,
            id,
            serde_json::json!({ "statut": "PROVIDER_EN_ROUTE", "latitude": LAT }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::BAD_REQUEST);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "POSITION_INCOMPLETE");
}

#[actix_web::test]
async fn security_la_mission_d_un_autre_est_introuvable_et_intacte() {
    let pool = pool().await;
    let app = bac!(pool);
    let (attribue, _) = prestataire(&pool, "attribue").await;
    let (_, curieux) = prestataire(&pool, "curieux").await;
    let jeton = jeton(&app, &curieux).await;
    let id = mission(&pool, attribue.id, "ACCEPTED").await;

    let reponse = test::call_service(
        &app,
        avancer(
            &jeton,
            id,
            serde_json::json!({ "statut": "PROVIDER_EN_ROUTE" }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
    assert!(transitions(&pool, id).await.is_empty());

    let statut: String = sqlx::query_scalar("SELECT statut FROM mission WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(statut, "ACCEPTED");
}

#[actix_web::test]
async fn security_un_compte_sans_fiche_prestataire_recoit_403() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "vrai").await;
    let (_, usager) = compte_actif(&pool, "usager").await;
    let jeton = jeton(&app, &usager).await;
    let id = mission(&pool, p.id, "ACCEPTED").await;

    let reponse = test::call_service(
        &app,
        avancer(
            &jeton,
            id,
            serde_json::json!({ "statut": "PROVIDER_EN_ROUTE" }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn security_sans_jeton_la_transition_est_refusee() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "sans-jeton").await;
    let id = mission(&pool, p.id, "ACCEPTED").await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri(&format!("/api/v1/missions/{id}/status"))
            .set_json(serde_json::json!({ "statut": "PROVIDER_EN_ROUTE" }))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn security_l_historique_ne_se_modifie_pas() {
    // Une preuve qu'on peut réécrire n'en est pas une.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "immuable").await;
    let jeton = jeton(&app, &email).await;
    let id = mission(&pool, p.id, "ACCEPTED").await;
    test::call_service(
        &app,
        avancer(
            &jeton,
            id,
            serde_json::json!({ "statut": "PROVIDER_EN_ROUTE" }),
        )
        .to_request(),
    )
    .await;

    let refus =
        sqlx::query("UPDATE mission_transition SET statut = 'ON_SITE' WHERE mission_id = $1")
            .bind(id)
            .execute(&pool)
            .await;
    assert!(refus.is_err(), "un UPDATE doit être refusé");

    let refus = sqlx::query("DELETE FROM mission_transition WHERE mission_id = $1")
        .bind(id)
        .execute(&pool)
        .await;
    assert!(refus.is_err(), "un DELETE doit être refusé");
}

#[actix_web::test]
async fn edge_une_mission_terminee_libere_le_prestataire() {
    // La Story 3.4 avait laissé la note : sans cela, un prestataire ayant
    // terminé une intervention resterait bloqué à vie.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "libere").await;
    let jeton = jeton(&app, &email).await;
    let id = mission(&pool, p.id, "ON_SITE").await;

    test::call_service(
        &app,
        avancer(&jeton, id, serde_json::json!({ "statut": "COMPLETED" })).to_request(),
    )
    .await;

    // Une seconde Mission devient possible : l'index partiel ne compte plus la
    // première.
    let seconde = mission(&pool, p.id, "ACCEPTED").await;
    let existe: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mission WHERE id = $1")
        .bind(seconde)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(existe, 1);
}

#[actix_web::test]
async fn security_le_prestataire_ne_valide_pas_par_la_route_de_statut() {
    // Le domaine autorise `COMPLETED` → `VALIDATED` ; cette route est celle du
    // prestataire, et lui laisser poser ce statut reviendrait à le laisser
    // signer la réception de son propre travail, donc déclencher son paiement.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "auto-validation").await;
    let jeton = jeton(&app, &email).await;
    let id = mission(&pool, p.id, "COMPLETED").await;

    let reponse = test::call_service(
        &app,
        avancer(&jeton, id, serde_json::json!({ "statut": "VALIDATED" })).to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::FORBIDDEN);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "RESERVED_TO_USER");

    let statut: String = sqlx::query_scalar("SELECT statut FROM mission WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("Mission relue");
    assert_eq!(statut, "COMPLETED", "rien ne doit avoir bougé");
}

#[actix_web::test]
async fn security_la_vue_du_prestataire_n_offre_pas_la_validation() {
    // Le bouton ne doit pas exister : le proposer ferait cliquer pour recevoir
    // un refus, et c'est déjà une erreur de conception.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "suites-validation").await;
    let jeton = jeton(&app, &email).await;
    let id = mission(&pool, p.id, "COMPLETED").await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/v1/missions/{id}"))
            .insert_header(("Authorization", format!("Bearer {jeton}")))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    let suites = corps["suites"].as_array().expect("suites");
    assert!(
        !suites.iter().any(|s| s == "VALIDATED"),
        "suites offertes : {suites:?}"
    );
}
