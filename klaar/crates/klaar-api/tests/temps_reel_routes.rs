//! Story 4.9 — billet et ouverture de socket, contre un vrai PostgreSQL.
//!
//! Les refus sont vérifiés ici, avant la poignée de main : c'est là que se joue
//! l'essentiel de la garantie, puisqu'une socket ouverte à tort resterait
//! ouverte. Le trajet complet d'un événement jusqu'au navigateur est vérifié
//! par le parcours filmé, contre le service réel.

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
const LAT: f64 = 50.8467;
const LON: f64 = 4.3525;

async fn pool() -> PoolPg {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI");
    creer_pool(&url).await.expect("connexion PostgreSQL")
}

async fn compte_actif(pool: &PoolPg, marqueur: &str) -> (Uuid, String) {
    let id = Uuid::new_v4();
    let email = format!("tr-{marqueur}-{id}@example.eu");
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

/// Pose une Demande attribuée et sa Mission. Rend (mission, demandeur email).
async fn mission(pool: &PoolPg, provider_id: Uuid) -> (Uuid, String) {
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
         VALUES ($1, $2, $3, 'ACCEPTED', now())",
    )
    .bind(id)
    .bind(demande_id)
    .bind(provider_id)
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

async fn billet<S>(app: &S, jeton: &str) -> String
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
            .uri("/api/v1/realtime/ticket")
            .insert_header(("Authorization", format!("Bearer {jeton}")))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::CREATED);
    let corps: Value = test::read_body_json(reponse).await;
    corps["billet"].as_str().expect("billet").to_string()
}

/// Requête d'ouverture de socket, avec les en-têtes du protocole.
fn ouverture(mission_id: Uuid, billet: &str) -> test::TestRequest {
    test::TestRequest::get()
        .uri(&format!(
            "/api/v1/missions/{mission_id}/events?billet={billet}"
        ))
        .insert_header(("Upgrade", "websocket"))
        .insert_header(("Connection", "Upgrade"))
        .insert_header(("Sec-WebSocket-Version", "13"))
        .insert_header(("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ=="))
}

// === @happy ===

#[actix_web::test]
async fn happy_le_prestataire_attribue_ouvre_la_socket() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "titulaire").await;
    let jeton = jeton(&app, &email).await;
    let (mission_id, _) = mission(&pool, p.id).await;

    let billet = billet(&app, &jeton).await;
    let reponse = test::call_service(&app, ouverture(mission_id, &billet).to_request()).await;

    assert_eq!(reponse.status(), StatusCode::SWITCHING_PROTOCOLS);
}

#[actix_web::test]
async fn happy_le_demandeur_ouvre_la_socket_de_sa_mission() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "pour-demandeur").await;
    let (mission_id, email_demandeur) = mission(&pool, p.id).await;
    let jeton = jeton(&app, &email_demandeur).await;

    let billet = billet(&app, &jeton).await;
    let reponse = test::call_service(&app, ouverture(mission_id, &billet).to_request()).await;

    assert_eq!(reponse.status(), StatusCode::SWITCHING_PROTOCOLS);
}

#[actix_web::test]
async fn happy_le_billet_annonce_sa_duree_de_vie() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "duree").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/realtime/ticket")
            .insert_header(("Authorization", format!("Bearer {jeton}")))
            .to_request(),
    )
    .await;
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["expire_dans"], 30);
    assert!(corps["billet"].as_str().unwrap().len() >= 40);
}

// === @negative ===

#[actix_web::test]
async fn negative_un_billet_est_exige() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "sans-billet").await;
    let (mission_id, _) = mission(&pool, p.id).await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/v1/missions/{mission_id}/events"))
            .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::BAD_REQUEST);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "TICKET_MISSING");
}

#[actix_web::test]
async fn negative_un_billet_inconnu_est_refuse() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "billet-faux").await;
    let (mission_id, _) = mission(&pool, p.id).await;

    let reponse = test::call_service(
        &app,
        ouverture(mission_id, "billet-invente-de-toutes-pieces").to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "TICKET_INVALID");
}

#[actix_web::test]
async fn negative_un_billet_demande_sans_jeton_est_refuse() {
    let pool = pool().await;
    let app = bac!(pool);

    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/realtime/ticket")
            .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED);
}

// === @edge ===

#[actix_web::test]
async fn edge_un_identifiant_de_mission_illisible_est_refuse_avant_le_billet() {
    // Avant, et non après : dépenser un billet pour un identifiant que le
    // client a lui-même mal formé lui coûterait un aller-retour de plus.
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "id-casse").await;
    let jeton = jeton(&app, &email).await;
    let billet = billet(&app, &jeton).await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/v1/missions/pas-un-uuid/events?billet={billet}"
            ))
            .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::BAD_REQUEST);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "MISSION_ID_INVALID");

    // Et le billet n'a pas été dépensé : il ouvre encore.
    let (p, _) = prestataire(&pool, "id-casse-p").await;
    let (mission_id, _) = mission(&pool, p.id).await;
    let seconde = test::call_service(&app, ouverture(mission_id, &billet).to_request()).await;
    // Refusée pour cause de droits, pas de billet : la Mission est à un autre.
    assert_eq!(seconde.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn edge_une_mission_inconnue_est_introuvable() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "mission-absente").await;
    let jeton = jeton(&app, &email).await;
    let billet = billet(&app, &jeton).await;

    let reponse = test::call_service(&app, ouverture(Uuid::new_v4(), &billet).to_request()).await;

    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
}

// === @security ===

#[actix_web::test]
async fn security_un_billet_ne_sert_qu_une_fois() {
    // C'est ce qui rend acceptable son passage par l'URL, qui finit dans les
    // journaux du serveur, du proxy et du navigateur.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "usage-unique").await;
    let jeton = jeton(&app, &email).await;
    let (mission_id, _) = mission(&pool, p.id).await;
    let billet = billet(&app, &jeton).await;

    let premiere = test::call_service(&app, ouverture(mission_id, &billet).to_request()).await;
    assert_eq!(premiere.status(), StatusCode::SWITCHING_PROTOCOLS);

    let seconde = test::call_service(&app, ouverture(mission_id, &billet).to_request()).await;
    assert_eq!(seconde.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn security_la_mission_d_un_tiers_est_rendue_introuvable() {
    // 404 et non 403 : un 403 apprendrait que cet identifiant est celui d'une
    // Mission qui existe. Même précédence que partout ailleurs.
    let pool = pool().await;
    let app = bac!(pool);
    let (titulaire, _) = prestataire(&pool, "vrai-titulaire").await;
    let (_, email_tiers) = compte_actif(&pool, "tiers").await;
    let jeton_tiers = jeton(&app, &email_tiers).await;
    let (mission_id, _) = mission(&pool, titulaire.id).await;

    let billet = billet(&app, &jeton_tiers).await;
    let reponse = test::call_service(&app, ouverture(mission_id, &billet).to_request()).await;

    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "MISSION_NOT_FOUND");
}

#[actix_web::test]
async fn security_un_prestataire_non_attribue_est_refuse() {
    let pool = pool().await;
    let app = bac!(pool);
    let (titulaire, _) = prestataire(&pool, "attribue").await;
    let (_, email_autre) = prestataire(&pool, "non-attribue").await;
    let jeton_autre = jeton(&app, &email_autre).await;
    let (mission_id, _) = mission(&pool, titulaire.id).await;

    let billet = billet(&app, &jeton_autre).await;
    let reponse = test::call_service(&app, ouverture(mission_id, &billet).to_request()).await;

    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn security_un_billet_refuse_est_quand_meme_depense() {
    // Sinon le même billet servirait à essayer des identifiants de Mission
    // jusqu'à en trouver un qui réponde autre chose que 404.
    let pool = pool().await;
    let app = bac!(pool);
    let (titulaire, _) = prestataire(&pool, "essais").await;
    let (_, email_tiers) = compte_actif(&pool, "essayeur").await;
    let jeton_tiers = jeton(&app, &email_tiers).await;
    let (mission_id, _) = mission(&pool, titulaire.id).await;
    let billet = billet(&app, &jeton_tiers).await;

    let premiere = test::call_service(&app, ouverture(mission_id, &billet).to_request()).await;
    assert_eq!(premiere.status(), StatusCode::NOT_FOUND);

    let seconde = test::call_service(&app, ouverture(mission_id, &billet).to_request()).await;
    assert_eq!(
        seconde.status(),
        StatusCode::UNAUTHORIZED,
        "un billet présenté est un billet dépensé, même refusé"
    );
}

#[actix_web::test]
async fn security_le_billet_d_un_compte_n_ouvre_pas_la_mission_d_un_autre() {
    // Le compte vient du billet, jamais d'un paramètre.
    let pool = pool().await;
    let app = bac!(pool);
    let (p_a, email_a) = prestataire(&pool, "compte-a").await;
    let (p_b, _) = prestataire(&pool, "compte-b").await;
    let (_, _) = mission(&pool, p_a.id).await;
    let (mission_de_b, _) = mission(&pool, p_b.id).await;

    let jeton_a = jeton(&app, &email_a).await;
    let billet_a = billet(&app, &jeton_a).await;
    let reponse = test::call_service(&app, ouverture(mission_de_b, &billet_a).to_request()).await;

    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
}
