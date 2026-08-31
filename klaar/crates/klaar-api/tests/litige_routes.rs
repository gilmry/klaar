//! Story 7.2 — ouverture de litige (FR-034), contre un vrai PostgreSQL.
//!
//! **Ce qui ne se teste qu'ici :** l'unicité « un litige par intervention »,
//! la fenêtre de quatorze jours qui se lit dans l'historique des transitions, et
//! le fait que la partie soit déduite du rôle réel plutôt que reçue.

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
    let email = format!("lit-{marqueur}-{id}@example.eu");
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

/// Récit assez long pour passer le minimum de vingt caractères.
const RECIT: &str = "Le joint fuit toujours et la trace d'eau s'est agrandie.";

fn ouvrir(jeton: &str, mission_id: Uuid, motif: &str, description: &str) -> test::TestRequest {
    test::TestRequest::post()
        .uri(&format!("/api/v1/missions/{mission_id}/dispute"))
        .insert_header(("Authorization", format!("Bearer {jeton}")))
        .set_json(serde_json::json!({ "motif": motif, "description": description }))
}

fn lire(jeton: &str, mission_id: Uuid) -> test::TestRequest {
    test::TestRequest::get()
        .uri(&format!("/api/v1/missions/{mission_id}/dispute"))
        .insert_header(("Authorization", format!("Bearer {jeton}")))
}

/// Consigne la fin de l'intervention, comme la route de transition le ferait.
async fn close_il_y_a(pool: &PoolPg, mission_id: Uuid, provider_id: Uuid, jours: i64) {
    sqlx::query(
        "INSERT INTO mission_transition
             (mission_id, provider_id, statut, horodate_le, enregistre_le, position, hors_zone)
         VALUES ($1, $2, 'COMPLETED', now() - ($3 || ' days')::interval,
                 now() - ($3 || ' days')::interval, NULL, FALSE)",
    )
    .bind(mission_id)
    .bind(provider_id)
    .bind(jours.to_string())
    .execute(pool)
    .await
    .expect("fin consignée");
}

// === @happy ===

#[actix_web::test]
async fn happy_le_demandeur_ouvre_un_litige_sur_la_qualite() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "qualite").await;
    let (mission_id, email) = mission(&pool, p.id, "COMPLETED").await;
    close_il_y_a(&pool, mission_id, p.id, 1).await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        ouvrir(&jeton, mission_id, "QUALITY", RECIT).to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::CREATED);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "DISPUTE_OPENED");
    assert_eq!(corps["partie"], "USER");
    assert_eq!(corps["statut"], "OPENED");
    assert_eq!(corps["a_examiner"], false);
}

#[actix_web::test]
async fn happy_le_prestataire_ouvre_pour_porte_close() {
    // FR-034 `@happy` : les deux parties peuvent ouvrir.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "porte-close").await;
    let (mission_id, _) = mission(&pool, p.id, "COMPLETED").await;
    close_il_y_a(&pool, mission_id, p.id, 1).await;
    let jeton_p = jeton(&app, &email_p).await;

    let reponse = test::call_service(
        &app,
        ouvrir(
            &jeton_p,
            mission_id,
            "USER_NO_SHOW",
            "Personne n'a ouvert après vingt minutes d'attente.",
        )
        .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::CREATED);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["partie"], "PROVIDER");
}

#[actix_web::test]
async fn happy_les_deux_parties_lisent_le_litige() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "lecture-litige").await;
    let (mission_id, email_d) = mission(&pool, p.id, "COMPLETED").await;
    close_il_y_a(&pool, mission_id, p.id, 1).await;
    let jeton_d = jeton(&app, &email_d).await;
    test::call_service(
        &app,
        ouvrir(&jeton_d, mission_id, "QUALITY", RECIT).to_request(),
    )
    .await;

    for jeton_lecteur in [jeton_d, jeton(&app, &email_p).await] {
        let lu = test::call_service(&app, lire(&jeton_lecteur, mission_id).to_request()).await;
        assert_eq!(lu.status(), StatusCode::OK);
        let corps: Value = test::read_body_json(lu).await;
        assert_eq!(corps["motif"], "QUALITY");
        assert_eq!(corps["description"], RECIT);
    }
}

// === @negative ===

#[actix_web::test]
async fn negative_une_description_trop_courte_est_refusee() {
    // FR-034 `@negative` : « pas content » ne permet à personne de trancher.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "maigre").await;
    let (mission_id, email) = mission(&pool, p.id, "COMPLETED").await;
    close_il_y_a(&pool, mission_id, p.id, 1).await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        ouvrir(&jeton, mission_id, "QUALITY", "pas content").to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "MOTIVE_REQUIRED");
}

#[actix_web::test]
async fn negative_la_fenetre_se_ferme_apres_quatorze_jours() {
    // FR-034 `@negative` : 410.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "fenetre-litige").await;
    let (mission_id, email) = mission(&pool, p.id, "COMPLETED").await;
    close_il_y_a(&pool, mission_id, p.id, 15).await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        ouvrir(&jeton, mission_id, "QUALITY", RECIT).to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::GONE);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "DISPUTE_WINDOW_CLOSED");
}

#[actix_web::test]
async fn negative_une_intervention_en_cours_ne_se_conteste_pas() {
    // Elle peut encore bien se terminer, et ouvrir un litige à mi-parcours
    // transformerait chaque contrariété en procédure.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "en-cours-litige").await;
    let (mission_id, email) = mission(&pool, p.id, "ON_SITE").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        ouvrir(&jeton, mission_id, "QUALITY", RECIT).to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::CONFLICT);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "MISSION_NOT_FINISHED");
}

// === @edge ===

#[actix_web::test]
async fn edge_un_second_litige_sur_la_meme_intervention_est_refuse() {
    // FR-034 `@edge` : 409.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "deux-litiges").await;
    let (mission_id, email) = mission(&pool, p.id, "COMPLETED").await;
    close_il_y_a(&pool, mission_id, p.id, 1).await;
    let jeton = jeton(&app, &email).await;

    let premier = test::call_service(
        &app,
        ouvrir(&jeton, mission_id, "QUALITY", RECIT).to_request(),
    )
    .await;
    assert_eq!(premier.status(), StatusCode::CREATED);

    let second = test::call_service(
        &app,
        ouvrir(&jeton, mission_id, "NOT_DONE", RECIT).to_request(),
    )
    .await;
    assert_eq!(second.status(), StatusCode::CONFLICT);
    let corps: Value = test::read_body_json(second).await;
    assert_eq!(corps["code"], "ALREADY_DISPUTED");
}

#[actix_web::test]
async fn edge_deux_litiges_du_meme_compte_levent_un_examen() {
    // FR-034 `@edge` : un signal, pas une sanction. Le compte reste libre
    // d'ouvrir le second — c'est l'exploitation qui regardera.
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "recidive-litige").await;
    let jeton = jeton(&app, &email).await;

    let mut derniere = None;
    for tour in 1..=2 {
        let (p, _) = prestataire(&pool, &format!("cible-{tour}")).await;
        // Une Mission par tour, rattachée à ce demandeur précis.
        let mission_id = Uuid::new_v4();
        let demande_id = Uuid::new_v4();
        let (demandeur_id,): (Uuid,) =
            sqlx::query_as("SELECT id FROM utilisateur WHERE email = $1")
                .bind(&email)
                .fetch_one(&pool)
                .await
                .expect("compte");
        sqlx::query(
            "INSERT INTO demande
                 (id, demandeur_id, secteur_code, description, position, urgence, statut,
                  rayon_metres, elargissements, diffuse_depuis, cree_le)
             VALUES ($1, $2, 'plomberie', 'Fuite',
                     ST_SetSRID(ST_MakePoint($3, $4), 4326)::geography,
                     'HIGH', 'MATCHED', 5000, 0, now(), now())",
        )
        .bind(demande_id)
        .bind(demandeur_id)
        .bind(LON)
        .bind(LAT)
        .execute(&pool)
        .await
        .expect("Demande");
        sqlx::query(
            "INSERT INTO mission (id, demande_id, provider_id, statut, cree_le)
             VALUES ($1, $2, $3, 'COMPLETED', now())",
        )
        .bind(mission_id)
        .bind(demande_id)
        .bind(p.id)
        .execute(&pool)
        .await
        .expect("Mission");
        close_il_y_a(&pool, mission_id, p.id, 1).await;

        derniere = Some(
            test::call_service(
                &app,
                ouvrir(&jeton, mission_id, "QUALITY", RECIT).to_request(),
            )
            .await,
        );
    }

    let reponse = derniere.expect("deux tours");
    assert_eq!(reponse.status(), StatusCode::CREATED);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["a_examiner"], true, "le second litige lève un examen");
}

// === @security ===

#[actix_web::test]
async fn security_un_motif_hors_propos_est_refuse() {
    // Un prestataire ne conteste pas sa propre qualité : l'accepter rendrait
    // tout comptage par motif ininterprétable.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "hors-propos").await;
    let (mission_id, _) = mission(&pool, p.id, "COMPLETED").await;
    close_il_y_a(&pool, mission_id, p.id, 1).await;
    let jeton_p = jeton(&app, &email_p).await;

    let reponse = test::call_service(
        &app,
        ouvrir(&jeton_p, mission_id, "QUALITY", RECIT).to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "MOTIVE_NOT_APPLICABLE");
}

#[actix_web::test]
async fn security_un_tiers_n_ouvre_pas_de_litige_sur_l_intervention_d_autrui() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "tiers-litige").await;
    let (mission_id, _) = mission(&pool, p.id, "COMPLETED").await;
    close_il_y_a(&pool, mission_id, p.id, 1).await;
    let (_, email_tiers) = compte_actif(&pool, "curieux-litige").await;
    let jeton_tiers = jeton(&app, &email_tiers).await;

    let ouverture = test::call_service(
        &app,
        ouvrir(&jeton_tiers, mission_id, "QUALITY", RECIT).to_request(),
    )
    .await;
    assert_eq!(ouverture.status(), StatusCode::NOT_FOUND);

    let lecture = test::call_service(&app, lire(&jeton_tiers, mission_id).to_request()).await;
    assert_eq!(lecture.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn security_le_recit_d_un_litige_ne_se_reecrit_pas() {
    // Laisser réécrire la description permettrait d'adapter son histoire à la
    // décision qui se dessine, ce qui viderait l'examen de son objet.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "fige-litige").await;
    let (mission_id, email) = mission(&pool, p.id, "COMPLETED").await;
    close_il_y_a(&pool, mission_id, p.id, 1).await;
    let jeton = jeton(&app, &email).await;
    test::call_service(
        &app,
        ouvrir(&jeton, mission_id, "QUALITY", RECIT).to_request(),
    )
    .await;

    let refus = sqlx::query("UPDATE litige SET description = $2 WHERE mission_id = $1")
        .bind(mission_id)
        .bind("En fait tout allait bien, je me suis trompé de prestataire.")
        .execute(&pool)
        .await
        .expect_err("le récit doit être figé");
    assert!(
        refus.to_string().contains("ne se réécrit pas"),
        "déclencheur attendu, obtenu : {refus}"
    );
}

#[actix_web::test]
async fn security_un_litige_nait_toujours_ouvert() {
    // Rien ne permet d'en fabriquer un déjà tranché, ce qui court-circuiterait
    // l'examen et pourrait suspendre quelqu'un sans qu'on l'ait entendu.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "naissance").await;
    let (mission_id, email) = mission(&pool, p.id, "COMPLETED").await;
    close_il_y_a(&pool, mission_id, p.id, 1).await;
    let jeton = jeton(&app, &email).await;
    test::call_service(
        &app,
        ouvrir(&jeton, mission_id, "QUALITY", RECIT).to_request(),
    )
    .await;

    let (statut, tranche): (String, Option<chrono::DateTime<Utc>>) =
        sqlx::query_as("SELECT statut, tranche_le FROM litige WHERE mission_id = $1")
            .bind(mission_id)
            .fetch_one(&pool)
            .await
            .expect("litige relu");
    assert_eq!(statut, "OPENED");
    assert_eq!(
        tranche, None,
        "un litige ouvert n'a pas de date de décision"
    );
}

#[actix_web::test]
async fn security_le_litige_exige_un_jeton() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "anonyme-litige").await;
    let (mission_id, _) = mission(&pool, p.id, "COMPLETED").await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/v1/missions/{mission_id}/dispute"))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED);
}
