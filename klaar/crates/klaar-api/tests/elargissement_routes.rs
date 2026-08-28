//! Story 3.6 — fin de tour et élargissement (FR-015), contre un vrai PostgreSQL.
//!
//! Le balayage lui-même se vérifie au niveau du dépôt
//! (`klaar-sqlx-repos/tests/diffusion.rs`). Ces cas-ci vérifient ce que l'API
//! rend au demandeur : qui peut élargir, jusqu'où, et ce qui se passe au
//! quatrième essai.

use actix_web::{http::StatusCode, test};
use chrono::{Duration, Utc};
use klaar_api::{app_de_test, etat_de_test};
use klaar_identity::{EmpreinteMotDePasse, MotDePasse, ParametresArgon2};
use klaar_matching::{ELARGISSEMENTS_MAX, RAYONS_METRES};
use klaar_sqlx_repos::{creer_pool, PoolPg};
use serde_json::Value;
use uuid::Uuid;

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
    let email = format!("elarg-{marqueur}-{id}@example.eu");
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

/// Insère une Demande dans l'état voulu, avec un tour d'un âge choisi.
///
/// Attendre trente secondes réelles dans une suite de tests serait absurde ;
/// poser l'état de départ en SQL dit la même chose sans faire perdre une
/// minute à chaque exécution.
async fn demande(
    pool: &PoolPg,
    demandeur_id: Uuid,
    statut: &str,
    elargissements: i16,
    tour_age_secondes: i64,
) -> Uuid {
    let id = Uuid::new_v4();
    let debut = Utc::now() - Duration::seconds(tour_age_secondes);
    sqlx::query(
        "INSERT INTO demande
             (id, demandeur_id, secteur_code, description, position, urgence, statut,
              rayon_metres, elargissements, diffuse_depuis, cree_le)
         VALUES ($1, $2, 'plomberie', 'Fuite sous l''évier',
                 ST_SetSRID(ST_MakePoint($3, $4), 4326)::geography, 'HIGH', $5,
                 $6, $7, $8, $8)",
    )
    .bind(id)
    .bind(demandeur_id)
    .bind(LON)
    .bind(LAT)
    .bind(statut)
    .bind(RAYONS_METRES[elargissements as usize])
    .bind(elargissements)
    .bind(debut)
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

fn elargir(jeton: &str, id: &str) -> test::TestRequest {
    test::TestRequest::post()
        .uri(&format!("/api/v1/requests/{id}/expand-radius"))
        .insert_header(("Authorization", format!("Bearer {jeton}")))
}

async fn statut_en_base(pool: &PoolPg, id: Uuid) -> String {
    sqlx::query_scalar("SELECT statut FROM demande WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("statut")
}

#[actix_web::test]
async fn happy_une_demande_sans_reponse_repart_sur_dix_kilometres() {
    let pool = pool().await;
    let app = bac!(pool);
    let (auteur, email) = compte_actif(&pool, "relance").await;
    let jeton = jeton(&app, &email).await;
    let id = demande(&pool, auteur, "NO_MATCH", 0, 60).await;

    let reponse = test::call_service(&app, elargir(&jeton, &id.to_string()).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "RADIUS_EXPANDED");
    assert_eq!(corps["statut"], "BROADCASTING");
    assert_eq!(corps["rayon_metres"], RAYONS_METRES[1]);
    assert_eq!(corps["elargissements"], 1);
    assert_eq!(statut_en_base(&pool, id).await, "BROADCASTING");
}

#[actix_web::test]
async fn happy_un_tour_echu_non_encore_balaye_s_elargit_aussi() {
    // Le demandeur n'a pas à connaître la cadence de nos tâches de fond : pour
    // lui, trente secondes ont passé et personne n'est venu.
    let pool = pool().await;
    let app = bac!(pool);
    let (auteur, email) = compte_actif(&pool, "pas-balaye").await;
    let jeton = jeton(&app, &email).await;
    let id = demande(&pool, auteur, "BROADCASTING", 0, 60).await;

    let reponse = test::call_service(&app, elargir(&jeton, &id.to_string()).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::OK);
}

#[actix_web::test]
async fn negative_une_demande_encore_dans_sa_fenetre_recoit_409() {
    // Élargir couperait le tour en cours, alors qu'un prestataire est
    // peut-être en train de répondre.
    let pool = pool().await;
    let app = bac!(pool);
    let (auteur, email) = compte_actif(&pool, "trop-tot").await;
    let jeton = jeton(&app, &email).await;
    let id = demande(&pool, auteur, "BROADCASTING", 0, 0).await;

    let reponse = test::call_service(&app, elargir(&jeton, &id.to_string()).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::CONFLICT);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "REQUEST_NOT_EXPIRED");
}

#[actix_web::test]
async fn negative_une_demande_annulee_recoit_409() {
    let pool = pool().await;
    let app = bac!(pool);
    let (auteur, email) = compte_actif(&pool, "annulee").await;
    let jeton = jeton(&app, &email).await;
    let id = demande(&pool, auteur, "CANCELLED", 0, 60).await;

    let reponse = test::call_service(&app, elargir(&jeton, &id.to_string()).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::CONFLICT);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "REQUEST_CLOSED");
}

#[actix_web::test]
async fn edge_le_quatrieme_elargissement_annule_la_demande() {
    // FR-015 `@security`. Laisser un `NO_MATCH` entretiendrait l'idée que
    // quelque chose peut encore arriver.
    let pool = pool().await;
    let app = bac!(pool);
    let (auteur, email) = compte_actif(&pool, "epuise").await;
    let jeton = jeton(&app, &email).await;
    let id = demande(&pool, auteur, "NO_MATCH", ELARGISSEMENTS_MAX as i16, 60).await;

    let reponse = test::call_service(&app, elargir(&jeton, &id.to_string()).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "MAX_RADIUS_REACHED");
    assert_eq!(statut_en_base(&pool, id).await, "CANCELLED");
}

#[actix_web::test]
async fn edge_trois_elargissements_menent_au_dernier_rayon() {
    let pool = pool().await;
    let app = bac!(pool);
    let (auteur, email) = compte_actif(&pool, "escalier").await;
    let jeton = jeton(&app, &email).await;
    let id = demande(&pool, auteur, "NO_MATCH", 0, 60).await;

    for (tour, attendu) in RAYONS_METRES.iter().enumerate().skip(1) {
        let reponse = test::call_service(&app, elargir(&jeton, &id.to_string()).to_request()).await;
        assert_eq!(reponse.status(), StatusCode::OK, "tour {tour}");
        let corps: Value = test::read_body_json(reponse).await;
        assert_eq!(corps["rayon_metres"], *attendu, "tour {tour}");
        // Le tour repart : on le vieillit pour pouvoir élargir à nouveau.
        sqlx::query(
            "UPDATE demande SET statut = 'NO_MATCH', diffuse_depuis = now() - interval '60 seconds'
             WHERE id = $1",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    }
    let reponse = test::call_service(&app, elargir(&jeton, &id.to_string()).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[actix_web::test]
async fn security_la_demande_d_un_autre_est_introuvable() {
    // Et non « interdite » : distinguer les deux laisserait apprendre quelles
    // Demandes existent en essayant des identifiants.
    let pool = pool().await;
    let app = bac!(pool);
    let (auteur, _) = compte_actif(&pool, "proprietaire").await;
    let (_, curieux) = compte_actif(&pool, "curieux").await;
    let jeton = jeton(&app, &curieux).await;
    let id = demande(&pool, auteur, "NO_MATCH", 0, 60).await;

    let reponse = test::call_service(&app, elargir(&jeton, &id.to_string()).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "REQUEST_NOT_FOUND");
    // Et elle n'a pas bougé.
    assert_eq!(statut_en_base(&pool, id).await, "NO_MATCH");
}

#[actix_web::test]
async fn security_sans_jeton_l_elargissement_est_refuse() {
    let pool = pool().await;
    let app = bac!(pool);
    let (auteur, _) = compte_actif(&pool, "sans-jeton").await;
    let id = demande(&pool, auteur, "NO_MATCH", 0, 60).await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/requests/{id}/expand-radius"))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn security_deux_clics_ne_consomment_qu_un_elargissement() {
    // Le dépôt garde `NO_MATCH` en condition : un double clic ne doit pas
    // brûler deux des trois chances du demandeur.
    let pool = pool().await;
    let app = bac!(pool);
    let (auteur, email) = compte_actif(&pool, "double-clic").await;
    let jeton = jeton(&app, &email).await;
    let id = demande(&pool, auteur, "NO_MATCH", 0, 60).await;

    let premier = test::call_service(&app, elargir(&jeton, &id.to_string()).to_request()).await;
    assert_eq!(premier.status(), StatusCode::OK);
    let second = test::call_service(&app, elargir(&jeton, &id.to_string()).to_request()).await;
    assert_eq!(second.status(), StatusCode::CONFLICT);

    let consommes: i16 = sqlx::query_scalar("SELECT elargissements FROM demande WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(consommes, 1);
}

#[actix_web::test]
async fn negative_un_identifiant_illisible_recoit_400() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "saisie").await;
    let jeton = jeton(&app, &email).await;

    for illisible in ["pas-un-uuid", "1", "0000"] {
        let reponse = test::call_service(&app, elargir(&jeton, illisible).to_request()).await;
        assert_eq!(
            reponse.status(),
            StatusCode::BAD_REQUEST,
            "identifiant {illisible}"
        );
    }
}
