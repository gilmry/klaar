//! Story 3.5 — annulation d'une Demande (FR-014), contre un vrai PostgreSQL.

use actix_web::{http::StatusCode, test};
use chrono::{Duration, Utc};
use klaar_api::{app_de_test, etat_de_test};
use klaar_identity::{EmpreinteMotDePasse, MotDePasse, ParametresArgon2};
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
    let email = format!("annul-{marqueur}-{id}@example.eu");
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

async fn demande(pool: &PoolPg, demandeur_id: Uuid, statut: &str) -> Uuid {
    let id = Uuid::new_v4();
    let debut = Utc::now() - Duration::seconds(1);
    sqlx::query(
        "INSERT INTO demande
             (id, demandeur_id, secteur_code, description, position, urgence, statut,
              rayon_metres, elargissements, diffuse_depuis, cree_le)
         VALUES ($1, $2, 'plomberie', 'Fuite sous l''évier',
                 ST_SetSRID(ST_MakePoint($3, $4), 4326)::geography, 'HIGH', $5,
                 5000, 0, $6, $6)",
    )
    .bind(id)
    .bind(demandeur_id)
    .bind(LON)
    .bind(LAT)
    .bind(statut)
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

fn supprimer(jeton: &str, id: &str, motif: Option<&str>) -> test::TestRequest {
    let uri = match motif {
        Some(m) => format!("/api/v1/requests/{id}?motif={m}"),
        None => format!("/api/v1/requests/{id}"),
    };
    test::TestRequest::delete()
        .uri(&uri)
        .insert_header(("Authorization", format!("Bearer {jeton}")))
}

async fn ligne(pool: &PoolPg, id: Uuid) -> (String, Option<String>) {
    sqlx::query_as("SELECT statut, motif_annulation FROM demande WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("la Demande")
}

#[actix_web::test]
async fn happy_le_demandeur_retire_sa_demande() {
    let pool = pool().await;
    let app = bac!(pool);
    let (auteur, email) = compte_actif(&pool, "retrait").await;
    let jeton = jeton(&app, &email).await;
    let id = demande(&pool, auteur, "BROADCASTING").await;

    let reponse =
        test::call_service(&app, supprimer(&jeton, &id.to_string(), None).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "REQUEST_CANCELLED");
    assert_eq!(corps["statut"], "CANCELLED");
    assert_eq!(ligne(&pool, id).await, ("CANCELLED".into(), None));
}

#[actix_web::test]
async fn happy_le_motif_donne_est_conserve() {
    let pool = pool().await;
    let app = bac!(pool);
    let (auteur, email) = compte_actif(&pool, "motif").await;
    let jeton = jeton(&app, &email).await;
    let id = demande(&pool, auteur, "BROADCASTING").await;

    let reponse = test::call_service(
        &app,
        supprimer(&jeton, &id.to_string(), Some("FOUND_ELSEWHERE")).to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::OK);
    assert_eq!(
        ligne(&pool, id).await,
        ("CANCELLED".into(), Some("FOUND_ELSEWHERE".into()))
    );
}

#[actix_web::test]
async fn happy_une_demande_sans_reponse_se_retire_aussi() {
    let pool = pool().await;
    let app = bac!(pool);
    let (auteur, email) = compte_actif(&pool, "sans-reponse").await;
    let jeton = jeton(&app, &email).await;
    let id = demande(&pool, auteur, "NO_MATCH").await;

    let reponse =
        test::call_service(&app, supprimer(&jeton, &id.to_string(), None).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::OK);
}

#[actix_web::test]
async fn negative_une_demande_attribuee_renvoie_vers_l_annulation_de_mission() {
    // Le prestataire est peut-être déjà en route.
    let pool = pool().await;
    let app = bac!(pool);
    let (auteur, email) = compte_actif(&pool, "attribuee").await;
    let jeton = jeton(&app, &email).await;
    let id = demande(&pool, auteur, "MATCHED").await;

    let reponse =
        test::call_service(&app, supprimer(&jeton, &id.to_string(), None).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::CONFLICT);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "ALREADY_MATCHED");
    assert_eq!(ligne(&pool, id).await.0, "MATCHED");
}

#[actix_web::test]
async fn negative_une_seconde_annulation_est_refusee() {
    let pool = pool().await;
    let app = bac!(pool);
    let (auteur, email) = compte_actif(&pool, "deux-fois").await;
    let jeton = jeton(&app, &email).await;
    let id = demande(&pool, auteur, "BROADCASTING").await;

    test::call_service(&app, supprimer(&jeton, &id.to_string(), None).to_request()).await;
    let reponse =
        test::call_service(&app, supprimer(&jeton, &id.to_string(), None).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::CONFLICT);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "ALREADY_CANCELLED");
}

#[actix_web::test]
async fn negative_un_motif_hors_vocabulaire_est_refuse() {
    // Refusé et non ramené sur `OTHER` : le ramener silencieusement ferait
    // passer pour un choix délibéré ce qui n'est qu'une faute de frappe, et
    // fausserait l'analyse.
    let pool = pool().await;
    let app = bac!(pool);
    let (auteur, email) = compte_actif(&pool, "motif-libre").await;
    let jeton = jeton(&app, &email).await;
    let id = demande(&pool, auteur, "BROADCASTING").await;

    for hostile in [
        "le%20plombier%20etait%20desagreable",
        "found_elsewhere",
        "DROP",
    ] {
        let reponse = test::call_service(
            &app,
            supprimer(&jeton, &id.to_string(), Some(hostile)).to_request(),
        )
        .await;
        assert_eq!(reponse.status(), StatusCode::BAD_REQUEST, "motif {hostile}");
    }
    // Et la Demande n'a pas bougé.
    assert_eq!(ligne(&pool, id).await.0, "BROADCASTING");
}

#[actix_web::test]
async fn negative_un_identifiant_illisible_recoit_400() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "saisie").await;
    let jeton = jeton(&app, &email).await;

    for illisible in ["pas-un-uuid", "1", "0000"] {
        let reponse =
            test::call_service(&app, supprimer(&jeton, illisible, None).to_request()).await;
        assert_eq!(
            reponse.status(),
            StatusCode::BAD_REQUEST,
            "identifiant {illisible}"
        );
    }
}

#[actix_web::test]
async fn security_la_demande_d_un_autre_est_introuvable_et_intacte() {
    // FR-014 demande un 403 ; c'est un 404 qui est rendu, parce que distinguer
    // « elle n'existe pas » de « elle n'est pas à vous » laisserait apprendre
    // quelles Demandes existent. La précédence de l'anti-énumération est une
    // décision déjà prise sur ce projet, et l'élargissement répond pareil.
    let pool = pool().await;
    let app = bac!(pool);
    let (auteur, _) = compte_actif(&pool, "proprietaire").await;
    let (_, curieux) = compte_actif(&pool, "curieux").await;
    let jeton = jeton(&app, &curieux).await;
    let id = demande(&pool, auteur, "BROADCASTING").await;

    let reponse =
        test::call_service(&app, supprimer(&jeton, &id.to_string(), None).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "REQUEST_NOT_FOUND");
    assert_eq!(ligne(&pool, id).await.0, "BROADCASTING");
}

#[actix_web::test]
async fn security_sans_jeton_l_annulation_est_refusee() {
    let pool = pool().await;
    let app = bac!(pool);
    let (auteur, _) = compte_actif(&pool, "sans-jeton").await;
    let id = demande(&pool, auteur, "BROADCASTING").await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::delete()
            .uri(&format!("/api/v1/requests/{id}"))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(ligne(&pool, id).await.0, "BROADCASTING");
}

#[actix_web::test]
async fn security_l_annulation_est_journalisee() {
    // FR-014 `@security`. Le journal porte le code et le compte, jamais le
    // motif : celui-ci vit sur la Demande, où il s'efface avec elle.
    let pool = pool().await;
    let app = bac!(pool);
    let (auteur, email) = compte_actif(&pool, "audit").await;
    let jeton = jeton(&app, &email).await;
    let id = demande(&pool, auteur, "BROADCASTING").await;

    test::call_service(
        &app,
        supprimer(&jeton, &id.to_string(), Some("TOO_SLOW")).to_request(),
    )
    .await;

    let entrees: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM journal_audit WHERE sujet_id = $1 AND code = 'REQUEST_CANCELLED'",
    )
    .bind(auteur)
    .fetch_one(&pool)
    .await
    .expect("journal");
    assert_eq!(entrees, 1);
}

#[actix_web::test]
async fn edge_annulation_et_acceptation_simultanees_n_en_laissent_passer_qu_une() {
    // FR-014 `@edge` : soit l'annulation gagne et le prestataire reçoit un
    // refus, soit l'acceptation gagne et le demandeur est renvoyé vers FR-023.
    // Les deux écritures portent sur la même ligne, PostgreSQL les sérialise.
    let pool = pool().await;
    let app = bac!(pool);
    let (auteur, email) = compte_actif(&pool, "course").await;
    let jeton = jeton(&app, &email).await;
    let id = demande(&pool, auteur, "BROADCASTING").await.to_string();

    let annulation = test::call_service(&app, supprimer(&jeton, &id, None).to_request()).await;
    let seconde = test::call_service(&app, supprimer(&jeton, &id, None).to_request()).await;

    // Une seule des deux annulations aboutit, quel que soit l'ordre.
    let succes = [annulation.status(), seconde.status()]
        .iter()
        .filter(|s| **s == StatusCode::OK)
        .count();
    assert_eq!(succes, 1);
}
