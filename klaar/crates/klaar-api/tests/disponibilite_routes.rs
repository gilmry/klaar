//! Story 3.7 — disponibilité du prestataire, contre un vrai PostgreSQL.
//!
//! Le filtre « déjà en mission » du matching se vérifie ici aussi : c'est du
//! SQL, et aucun double en mémoire ne dirait si le `NOT EXISTS` porte sur la
//! bonne colonne.

use actix_web::{http::StatusCode, test};
use chrono::Utc;
use klaar_api::{app_de_test, etat_de_test};
use klaar_catalog::CodeCatalogue;
use klaar_identity::{
    EmpreinteMotDePasse, MotDePasse, NumeroBce, ParametresArgon2, PreuveKyc, Provider,
    RAYON_INTERVENTION_MAX, RAYON_INTERVENTION_MIN,
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

/// Plafond de classement assez haut pour qu'aucun accumulé ne fasse écran.
///
/// **Ces cas portent sur le filtrage, pas sur le rang.** La base de
/// développement est partagée avec toute la suite et accumule des prestataires
/// posés au même endroit ; interroger un « dix plus proches » y ferait dépendre
/// l'assertion du nombre de tests exécutés avant, ce qui la rend intermittente.
/// Le service, lui, borne à dix — c'est `chercher_candidats` qui le fait, et
/// ses propres cas le vérifient.
const CLASSEMENT_COMPLET: i64 = 1_000_000;

async fn pool() -> PoolPg {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI");
    creer_pool(&url).await.expect("connexion PostgreSQL")
}

async fn compte_actif(pool: &PoolPg, marqueur: &str) -> (Uuid, String) {
    let id = Uuid::new_v4();
    let email = format!("dispo-{marqueur}-{id}@example.eu");
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
    let corps = 1_000_000 + (Uuid::new_v4().as_u128() as u64) % 8_999_999;
    NumeroBce::parse(&format!("{corps:08}{:02}", 97 - (corps % 97))).expect("numéro construit")
}

/// Crée un prestataire actif, en service, à la position voulue.
async fn prestataire(pool: &PoolPg, marqueur: &str, lat: f64, lon: f64) -> (Provider, String) {
    let (utilisateur_id, email) = compte_actif(pool, marqueur).await;
    let mut p = Provider::inscrire(
        utilisateur_id,
        numero(),
        &format!("Prestataire {marqueur}"),
        Geo::new(lat, lon).unwrap(),
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
        .expect("mise en service");
    (p, email)
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

fn lire(jeton: &str) -> test::TestRequest {
    test::TestRequest::get()
        .uri("/api/v1/providers/me/availability")
        .insert_header(("Authorization", format!("Bearer {jeton}")))
}

fn regler(jeton: &str, corps: Value) -> test::TestRequest {
    test::TestRequest::patch()
        .uri("/api/v1/providers/me/availability")
        .insert_header(("Authorization", format!("Bearer {jeton}")))
        .set_json(corps)
}

/// Pose une Mission en cours sur ce prestataire.
async fn occuper(pool: &PoolPg, provider_id: Uuid, demandeur_id: Uuid) {
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
    sqlx::query(
        "INSERT INTO mission (id, demande_id, provider_id, statut, cree_le)
         VALUES ($1, $2, $3, 'ACCEPTED', now())",
    )
    .bind(Uuid::new_v4())
    .bind(demande_id)
    .bind(provider_id)
    .execute(pool)
    .await
    .expect("Mission");
}

#[actix_web::test]
async fn happy_un_prestataire_lit_son_etat() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "lecture", LAT, LON).await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(&app, lire(&jeton).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["provider_id"], p.id.to_string());
    assert_eq!(corps["statut"], "ACTIVE");
    assert_eq!(corps["disponible"], true);
    assert_eq!(corps["occupe"], false);
    assert_eq!(corps["sollicitable"], true);
    // Par défaut, aucune limite propre : c'est le rayon du tour qui décide.
    assert_eq!(corps["rayon_intervention_metres"], RAYON_INTERVENTION_MAX);
}

#[actix_web::test]
async fn happy_la_pause_et_la_reprise_sont_symetriques() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = prestataire(&pool, "pause", LAT, LON).await;
    let jeton = jeton(&app, &email).await;

    for (demande, attendu) in [(false, false), (true, true)] {
        let reponse = test::call_service(
            &app,
            regler(&jeton, serde_json::json!({ "disponible": demande })).to_request(),
        )
        .await;
        assert_eq!(reponse.status(), StatusCode::OK);
        let corps: Value = test::read_body_json(reponse).await;
        assert_eq!(corps["disponible"], attendu);
        assert_eq!(corps["sollicitable"], attendu);
    }
}

#[actix_web::test]
async fn happy_le_rayon_se_regle_sans_toucher_a_la_disponibilite() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = prestataire(&pool, "rayon", LAT, LON).await;
    let jeton = jeton(&app, &email).await;

    test::call_service(
        &app,
        regler(&jeton, serde_json::json!({ "disponible": false })).to_request(),
    )
    .await;
    let reponse = test::call_service(
        &app,
        regler(
            &jeton,
            serde_json::json!({ "rayon_intervention_metres": 3000.0 }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["rayon_intervention_metres"], 3000.0);
    // Changer de rayon ne doit pas sortir quelqu'un de sa pause.
    assert_eq!(corps["disponible"], false);
}

#[actix_web::test]
async fn negative_un_rayon_hors_bornes_est_refuse() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = prestataire(&pool, "hors-bornes", LAT, LON).await;
    let jeton = jeton(&app, &email).await;

    for metres in [
        0.0,
        RAYON_INTERVENTION_MIN - 1.0,
        RAYON_INTERVENTION_MAX + 1.0,
    ] {
        let reponse = test::call_service(
            &app,
            regler(
                &jeton,
                serde_json::json!({ "rayon_intervention_metres": metres }),
            )
            .to_request(),
        )
        .await;
        assert_eq!(reponse.status(), StatusCode::BAD_REQUEST, "rayon {metres}");
        let corps: Value = test::read_body_json(reponse).await;
        assert_eq!(corps["code"], "SERVICE_RADIUS_OUT_OF_RANGE");
    }
}

#[actix_web::test]
async fn negative_un_compte_sans_fiche_prestataire_recoit_403() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "simple-usager").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(&app, lire(&jeton).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::FORBIDDEN);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "NOT_A_PROVIDER");
}

#[actix_web::test]
async fn edge_un_prestataire_occupe_le_voit_sans_etre_en_pause() {
    // Sans ce champ, un prestataire en service et pourtant jamais sollicité
    // conclurait que le service est cassé.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "occupe", LAT, LON).await;
    let (demandeur, _) = compte_actif(&pool, "demandeur").await;
    occuper(&pool, p.id, demandeur).await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(&app, lire(&jeton).to_request()).await;
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["occupe"], true);
    assert_eq!(
        corps["disponible"], true,
        "il n'est pas en pause pour autant"
    );
    assert_eq!(corps["sollicitable"], false);
}

#[actix_web::test]
async fn edge_un_prestataire_occupe_n_est_plus_propose_par_le_matching() {
    // Le notifier lui ferait ouvrir l'application pour se voir refuser, et
    // volerait sa place à quelqu'un de libre.
    let pool = pool().await;
    let depot = PgProviderRepository::new(pool.clone());
    let (p, _) = prestataire(&pool, "hors-matching", LAT, LON).await;
    let (demandeur, _) = compte_actif(&pool, "dem-matching").await;

    let position = Geo::new(LAT, LON).unwrap();
    let secteur = CodeCatalogue::parse("plomberie").unwrap();
    let avant = depot
        .proches(&secteur, position, 5_000.0, CLASSEMENT_COMPLET)
        .await
        .unwrap();
    assert!(avant.iter().any(|c| c.provider.id == p.id));

    occuper(&pool, p.id, demandeur).await;

    let apres = depot
        .proches(&secteur, position, 5_000.0, CLASSEMENT_COMPLET)
        .await
        .unwrap();
    assert!(!apres.iter().any(|c| c.provider.id == p.id));
}

#[actix_web::test]
async fn edge_un_prestataire_hors_de_son_propre_rayon_n_est_pas_propose() {
    // Le rayon du tour dit jusqu'où la Demande cherche ; celui du prestataire
    // dit jusqu'où il accepte d'aller. Les deux s'appliquent.
    let pool = pool().await;
    let depot = PgProviderRepository::new(pool.clone());
    // Environ 3,3 km au nord de la Grand-Place.
    let (p, _) = prestataire(&pool, "rayon-serre", LAT + 0.03, LON).await;
    let position = Geo::new(LAT, LON).unwrap();
    let secteur = CodeCatalogue::parse("plomberie").unwrap();

    let avant = depot
        .proches(&secteur, position, 5_000.0, CLASSEMENT_COMPLET)
        .await
        .unwrap();
    assert!(avant.iter().any(|c| c.provider.id == p.id));

    depot
        .definir_rayon_intervention(p.id, RAYON_INTERVENTION_MIN)
        .await
        .unwrap();

    let apres = depot
        .proches(&secteur, position, 5_000.0, CLASSEMENT_COMPLET)
        .await
        .unwrap();
    assert!(
        !apres.iter().any(|c| c.provider.id == p.id),
        "un prestataire qui ne se déplace qu'à un kilomètre ne doit pas être proposé à trois"
    );
}

#[actix_web::test]
async fn edge_un_prestataire_en_pause_n_est_plus_propose() {
    let pool = pool().await;
    let depot = PgProviderRepository::new(pool.clone());
    let (p, _) = prestataire(&pool, "en-pause", LAT, LON).await;
    let position = Geo::new(LAT, LON).unwrap();
    let secteur = CodeCatalogue::parse("plomberie").unwrap();

    depot.definir_disponibilite(p.id, false).await.unwrap();
    let apres = depot
        .proches(&secteur, position, 5_000.0, CLASSEMENT_COMPLET)
        .await
        .unwrap();
    assert!(!apres.iter().any(|c| c.provider.id == p.id));
}

#[actix_web::test]
async fn security_sans_jeton_la_disponibilite_est_inaccessible() {
    let pool = pool().await;
    let app = bac!(pool);

    for requete in [
        test::TestRequest::get().uri("/api/v1/providers/me/availability"),
        test::TestRequest::patch()
            .uri("/api/v1/providers/me/availability")
            .set_json(serde_json::json!({ "disponible": true })),
    ] {
        let reponse = test::call_service(&app, requete.to_request()).await;
        assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED);
    }
}

#[actix_web::test]
async fn security_le_reglage_ne_porte_que_sur_sa_propre_fiche() {
    // La fiche est retrouvée par le compte du jeton : il n'existe aucun champ
    // pour désigner celle d'un autre, et un identifiant glissé dans le corps
    // est refusé par `deny_unknown_fields`.
    let pool = pool().await;
    let app = bac!(pool);
    let (mien, email) = prestataire(&pool, "moi", LAT, LON).await;
    let (autre, _) = prestataire(&pool, "autre", LAT, LON).await;
    let jeton = jeton(&app, &email).await;

    let refus = test::call_service(
        &app,
        regler(
            &jeton,
            serde_json::json!({ "disponible": false, "provider_id": autre.id.to_string() }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(refus.status(), StatusCode::BAD_REQUEST);

    let reponse = test::call_service(
        &app,
        regler(&jeton, serde_json::json!({ "disponible": false })).to_request(),
    )
    .await;
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["provider_id"], mien.id.to_string());

    // Et l'autre n'a pas bougé.
    let depot = PgProviderRepository::new(pool.clone());
    assert!(depot.par_id(autre.id).await.unwrap().unwrap().disponible);
}
