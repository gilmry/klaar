//! Story 4.10 — suivi d'une Demande et d'une Mission, contre un vrai PostgreSQL.
//!
//! Le cœur de ces cas est une **asymétrie** : le prestataire ne voit pas
//! l'adresse avant d'avoir pris l'intervention, le demandeur voit le nom de
//! l'entreprise dès qu'elle l'a prise. C'est la règle que ces tests fixent, et
//! elle ne se lit dans aucun type isolément.

use actix_web::{http::StatusCode, test};
use chrono::Utc;
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
    let email = format!("suivi-{marqueur}-{id}@example.eu");
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

async fn prestataire(pool: &PoolPg, marqueur: &str, raison: &str) -> (Provider, String) {
    let (utilisateur_id, email) = compte_actif(pool, marqueur).await;
    let mut p = Provider::inscrire(
        utilisateur_id,
        numero(),
        raison,
        Geo::new(LAT, LON).unwrap(),
        vec![CodeCatalogue::parse("plomberie").unwrap()],
        Utc::now(),
    )
    .expect("prestataire valide");
    p.valider_kyc(PreuveKyc::demonstration(Utc::now()));
    let depot = PgProviderRepository::new(pool.clone());
    depot.creer(&p).await.expect("création");
    depot
        .definir_disponibilite(p.id, true)
        .await
        .expect("service");
    (p, email)
}

/// Insère une Demande diffusée et sa trace pour ce prestataire.
async fn demande_proposee(pool: &PoolPg, provider_id: Uuid, retenu: bool) -> (Uuid, Uuid) {
    let (demandeur_id, _) = compte_actif(pool, "demandeur").await;
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO demande
             (id, demandeur_id, secteur_code, description, position, urgence, statut,
              rayon_metres, elargissements, diffuse_depuis, cree_le)
         VALUES ($1, $2, 'plomberie', 'Fuite sous l''évier',
                 ST_SetSRID(ST_MakePoint($3, $4), 4326)::geography, 'HIGH', 'BROADCASTING',
                 5000, 0, now(), now())",
    )
    .bind(id)
    .bind(demandeur_id)
    .bind(LON)
    .bind(LAT)
    .execute(pool)
    .await
    .expect("Demande");

    sqlx::query(
        "INSERT INTO trace_matching
             (demande_id, provider_id, score, distance_metres, ventilation, retenu, motif_ecart, tracee_le)
         VALUES ($1, $2, 0.9, 1200, '{}'::jsonb, $3, CASE WHEN $3 THEN NULL ELSE 'HORS_TOP' END, now())",
    )
    .bind(id)
    .bind(provider_id)
    .bind(retenu)
    .execute(pool)
    .await
    .expect("trace");
    (id, demandeur_id)
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

fn lire(jeton: &str, chemin: &str) -> test::TestRequest {
    test::TestRequest::get()
        .uri(chemin)
        .insert_header(("Authorization", format!("Bearer {jeton}")))
}

/// Connecte le compte d'un demandeur créé par `demande_proposee`.
async fn jeton_demandeur<S>(app: &S, pool: &PoolPg, demandeur_id: Uuid) -> String
where
    S: actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
{
    let email: String = sqlx::query_scalar("SELECT email FROM utilisateur WHERE id = $1")
        .bind(demandeur_id)
        .fetch_one(pool)
        .await
        .expect("courriel");
    jeton(app, &email).await
}

#[actix_web::test]
async fn happy_le_demandeur_suit_sa_demande() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "suivi", "Plomberie Test").await;
    let (id, demandeur) = demande_proposee(&pool, p.id, true).await;
    let jd = jeton_demandeur(&app, &pool, demandeur).await;

    let reponse = test::call_service(
        &app,
        lire(&jd, &format!("/api/v1/requests/{id}")).to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["statut"], "BROADCASTING");
    assert_eq!(corps["tour_ecoule"], false);
    assert!(corps["prestataire"].is_null(), "personne n'a encore pris");
}

#[actix_web::test]
async fn happy_le_prestataire_voit_les_demandes_qui_lui_sont_proposees() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "propose", "Plomberie Test").await;
    let jp = jeton(&app, &email).await;
    let (id, _) = demande_proposee(&pool, p.id, true).await;

    let reponse = test::call_service(
        &app,
        lire(&jp, "/api/v1/providers/me/requests").to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    let liste = corps.as_array().expect("un tableau");
    let mienne = liste
        .iter()
        .find(|d| d["id"] == id.to_string())
        .expect("la Demande proposée");
    assert_eq!(mienne["secteur"], "plomberie");
    assert!(mienne["secondes_restantes"].as_i64().unwrap() > 0);
}

#[actix_web::test]
async fn security_la_liste_du_prestataire_ne_porte_aucune_adresse() {
    // C'est la garantie centrale de cette story : dix entreprises n'ont pas à
    // connaître l'adresse d'un foyer pour un dépannage que neuf ne feront pas.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "sans-adresse", "Plomberie Test").await;
    let jp = jeton(&app, &email).await;
    demande_proposee(&pool, p.id, true).await;

    let reponse = test::call_service(
        &app,
        lire(&jp, "/api/v1/providers/me/requests").to_request(),
    )
    .await;
    let brut = String::from_utf8(test::read_body(reponse).await.to_vec()).unwrap();
    for interdit in ["latitude", "longitude", "position", "50.84", "4.35"] {
        assert!(
            !brut.contains(interdit),
            "la liste ne doit pas porter {interdit}"
        );
    }
}

#[actix_web::test]
async fn happy_l_adresse_apparait_une_fois_la_mission_prise() {
    // Et là, elle doit : le prestataire attribué s'y rend.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "attribue", "Plomberie Test").await;
    let jp = jeton(&app, &email).await;
    let (id, _) = demande_proposee(&pool, p.id, true).await;

    let acceptation = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/requests/{id}/accept"))
            .insert_header(("Authorization", format!("Bearer {jp}")))
            .to_request(),
    )
    .await;
    assert_eq!(acceptation.status(), StatusCode::CREATED);
    let attribuee: Value = test::read_body_json(acceptation).await;
    let mission_id = attribuee["id"].as_str().unwrap();

    let reponse = test::call_service(
        &app,
        lire(&jp, &format!("/api/v1/missions/{mission_id}")).to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    assert!((corps["latitude"].as_f64().unwrap() - LAT).abs() < 1e-6);
    assert!((corps["longitude"].as_f64().unwrap() - LON).abs() < 1e-6);
    // Les suites viennent du serveur : l'interface n'a pas à recopier la
    // machine à états.
    assert_eq!(
        corps["suites"].as_array().unwrap(),
        &vec![Value::from("PROVIDER_EN_ROUTE"), Value::from("CANCELLED")]
    );
}

#[actix_web::test]
async fn happy_le_demandeur_apprend_qui_vient() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "qui-vient", "Plomberie Bien Nommée").await;
    let jp = jeton(&app, &email).await;
    let (id, demandeur) = demande_proposee(&pool, p.id, true).await;
    let jd = jeton_demandeur(&app, &pool, demandeur).await;

    test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/requests/{id}/accept"))
            .insert_header(("Authorization", format!("Bearer {jp}")))
            .to_request(),
    )
    .await;

    let reponse = test::call_service(
        &app,
        lire(&jd, &format!("/api/v1/requests/{id}")).to_request(),
    )
    .await;
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["statut"], "MATCHED");
    assert_eq!(corps["prestataire"], "Plomberie Bien Nommée");
    assert_eq!(corps["mission_statut"], "ACCEPTED");
}

#[actix_web::test]
async fn negative_un_candidat_ecarte_ne_voit_pas_la_demande() {
    // `retenu = false` : il a été examiné, pas retenu. La lui montrer
    // contournerait le classement.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "ecarte", "Plomberie Test").await;
    let jp = jeton(&app, &email).await;
    let (id, _) = demande_proposee(&pool, p.id, false).await;

    let reponse = test::call_service(
        &app,
        lire(&jp, "/api/v1/providers/me/requests").to_request(),
    )
    .await;
    let corps: Value = test::read_body_json(reponse).await;
    assert!(!corps
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["id"] == id.to_string()));
}

#[actix_web::test]
async fn security_la_demande_d_un_autre_est_introuvable() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "tiers", "Plomberie Test").await;
    let (id, _) = demande_proposee(&pool, p.id, true).await;
    let (_, curieux) = compte_actif(&pool, "curieux").await;
    let jc = jeton(&app, &curieux).await;

    let reponse = test::call_service(
        &app,
        lire(&jc, &format!("/api/v1/requests/{id}")).to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn security_la_mission_d_un_autre_prestataire_est_introuvable() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "sien", "Plomberie Test").await;
    let (_, curieux) = prestataire(&pool, "voisin", "Plomberie Voisine").await;
    let jp = jeton(&app, &email).await;
    let jc = jeton(&app, &curieux).await;
    let (id, _) = demande_proposee(&pool, p.id, true).await;

    let acceptation = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/requests/{id}/accept"))
            .insert_header(("Authorization", format!("Bearer {jp}")))
            .to_request(),
    )
    .await;
    let attribuee: Value = test::read_body_json(acceptation).await;
    let mission_id = attribuee["id"].as_str().unwrap();

    let reponse = test::call_service(
        &app,
        lire(&jc, &format!("/api/v1/missions/{mission_id}")).to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn security_un_compte_sans_fiche_prestataire_recoit_403() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, usager) = compte_actif(&pool, "usager").await;
    let ju = jeton(&app, &usager).await;

    let reponse = test::call_service(
        &app,
        lire(&ju, "/api/v1/providers/me/requests").to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn security_sans_jeton_rien_n_est_lisible() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "sans-jeton", "Plomberie Test").await;
    let (id, _) = demande_proposee(&pool, p.id, true).await;

    for chemin in [
        format!("/api/v1/requests/{id}"),
        "/api/v1/providers/me/requests".to_string(),
        format!("/api/v1/missions/{}", Uuid::new_v4()),
    ] {
        let reponse =
            test::call_service(&app, test::TestRequest::get().uri(&chemin).to_request()).await;
        assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED, "{chemin}");
    }
}
