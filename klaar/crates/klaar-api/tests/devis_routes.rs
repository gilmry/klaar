//! Story 4.1 — envoi d'un devis (FR-016), contre un vrai PostgreSQL.
//!
//! Deux des trois règles de cette story vivent dans la base et nulle part
//! ailleurs : « un seul devis en attente » est un index partiel, « trois devis
//! au maximum » est un `WHERE` sur l'insertion. Ni le domaine ni un double en
//! mémoire ne diraient si elles tiennent.

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
    let email = format!("dev-{marqueur}-{id}@example.eu");
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
async fn mission(pool: &PoolPg, provider_id: Uuid, statut: &str) -> (Uuid, Uuid) {
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
    (id, demande_id)
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

fn devis(jeton: &str, mission_id: Uuid, corps: Value) -> test::TestRequest {
    test::TestRequest::post()
        .uri(&format!("/api/v1/missions/{mission_id}/quote"))
        .insert_header(("Authorization", format!("Bearer {jeton}")))
        .set_json(corps)
}

fn proposition() -> Value {
    serde_json::json!({
        "montant_htva_cents": 18_000,
        "taux_tva_bp": 2100,
        "delai_minutes": 45,
        "note": "remplacement joint"
    })
}

/// Fait passer le devis en attente à un statut terminal, comme le fera le
/// demandeur (FR-017) ou le balayage.
async fn clore_le_devis(pool: &PoolPg, mission_id: Uuid, statut: &str) {
    sqlx::query("UPDATE devis SET statut = $2 WHERE mission_id = $1 AND statut = 'SENT'")
        .bind(mission_id)
        .bind(statut)
        .execute(pool)
        .await
        .expect("clôture du devis");
}

// === @happy ===

#[actix_web::test]
async fn happy_le_devis_nominal_est_ecrit_et_rendu() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "nominal").await;
    let jeton = jeton(&app, &email).await;
    let (mission_id, _) = mission(&pool, p.id, "ACCEPTED").await;

    let reponse =
        test::call_service(&app, devis(&jeton, mission_id, proposition()).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::CREATED);
    let corps: Value = test::read_body_json(reponse).await;

    assert_eq!(corps["code"], "QUOTE_SENT");
    assert_eq!(corps["montant_htva_cents"], 18_000);
    assert_eq!(corps["tva_cents"], 3_780);
    assert_eq!(corps["total_ttc_cents"], 21_780);
    assert_eq!(corps["statut"], "SENT");

    // La ligne écrite porte l'émetteur et le montant : c'est le journal que
    // FR-016 `@security` demande.
    let (provider_id, montant): (Uuid, i64) =
        sqlx::query_as("SELECT provider_id, montant_htva_cents FROM devis WHERE mission_id = $1")
            .bind(mission_id)
            .fetch_one(&pool)
            .await
            .expect("devis en base");
    assert_eq!(provider_id, p.id);
    assert_eq!(montant, 18_000);
}

#[actix_web::test]
async fn happy_le_demandeur_voit_le_devis_dans_son_suivi() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "suivi").await;
    let jeton_p = jeton(&app, &email).await;
    let (mission_id, demande_id) = mission(&pool, p.id, "ACCEPTED").await;
    let email_demandeur: String = sqlx::query_scalar(
        "SELECT u.email FROM utilisateur u JOIN demande d ON d.demandeur_id = u.id WHERE d.id = $1",
    )
    .bind(demande_id)
    .fetch_one(&pool)
    .await
    .expect("demandeur");

    test::call_service(
        &app,
        devis(&jeton_p, mission_id, proposition()).to_request(),
    )
    .await;

    let jeton_d = jeton(&app, &email_demandeur).await;
    let reponse = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/v1/requests/{demande_id}"))
            .insert_header(("Authorization", format!("Bearer {jeton_d}")))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["devis"]["total_ttc_cents"], 21_780);
    assert_eq!(corps["devis"]["statut"], "SENT");
    assert_eq!(corps["devis"]["echu"], false);
}

// === @negative ===

#[actix_web::test]
async fn negative_les_montants_impossibles_sont_refuses() {
    let pool = pool().await;
    let app = bac!(pool);
    for (cents, code) in [
        (0_i64, "AMOUNT_ZERO"),
        (-1_000, "AMOUNT_NEGATIVE"),
        (10_000_000, "AMOUNT_TOO_HIGH"),
    ] {
        // Une Mission par cas : un refus ne doit rien laisser derrière lui, et
        // réutiliser la même masquerait un devis écrit par erreur. Un
        // prestataire par cas aussi, faute de quoi l'index « une Mission à la
        // fois » (V13) refuserait la deuxième — c'est le domaine qui parle,
        // pas une gêne de test.
        let (p, email) = prestataire(&pool, &format!("montant-{cents}")).await;
        let jeton = jeton(&app, &email).await;
        let (mission_id, _) = mission(&pool, p.id, "ACCEPTED").await;
        let mut corps = proposition();
        corps["montant_htva_cents"] = serde_json::json!(cents);
        let reponse = test::call_service(&app, devis(&jeton, mission_id, corps).to_request()).await;
        assert_eq!(reponse.status(), StatusCode::BAD_REQUEST, "montant {cents}");
        let rendu: Value = test::read_body_json(reponse).await;
        assert_eq!(rendu["code"], code);

        let ecrits: i64 = sqlx::query_scalar("SELECT count(*) FROM devis WHERE mission_id = $1")
            .bind(mission_id)
            .fetch_one(&pool)
            .await
            .expect("comptage");
        assert_eq!(ecrits, 0, "aucun devis ne doit rester pour {cents}");
    }
}

#[actix_web::test]
async fn negative_un_delai_de_plus_de_vingt_quatre_heures_est_refuse() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "delai").await;
    let jeton = jeton(&app, &email).await;
    let (mission_id, _) = mission(&pool, p.id, "ACCEPTED").await;

    let mut corps = proposition();
    corps["delai_minutes"] = serde_json::json!(25 * 60);
    let reponse = test::call_service(&app, devis(&jeton, mission_id, corps).to_request()).await;

    assert_eq!(reponse.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let rendu: Value = test::read_body_json(reponse).await;
    assert_eq!(rendu["code"], "DELAY_TOO_LONG");
}

#[actix_web::test]
async fn negative_un_taux_reduit_sans_preuve_est_refuse() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "tva").await;
    let jeton = jeton(&app, &email).await;
    let (mission_id, _) = mission(&pool, p.id, "ACCEPTED").await;

    let mut corps = proposition();
    corps["taux_tva_bp"] = serde_json::json!(600);
    let reponse = test::call_service(&app, devis(&jeton, mission_id, corps).to_request()).await;

    assert_eq!(reponse.status(), StatusCode::BAD_REQUEST);
    let rendu: Value = test::read_body_json(reponse).await;
    assert_eq!(rendu["code"], "VAT_PROOF_REQUIRED");
}

// === @edge ===

#[actix_web::test]
async fn edge_un_second_devis_est_refuse_tant_que_le_premier_attend() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "double").await;
    let jeton = jeton(&app, &email).await;
    let (mission_id, _) = mission(&pool, p.id, "ACCEPTED").await;

    let premier =
        test::call_service(&app, devis(&jeton, mission_id, proposition()).to_request()).await;
    assert_eq!(premier.status(), StatusCode::CREATED);

    let second =
        test::call_service(&app, devis(&jeton, mission_id, proposition()).to_request()).await;
    assert_eq!(second.status(), StatusCode::CONFLICT);
    let rendu: Value = test::read_body_json(second).await;
    assert_eq!(rendu["code"], "QUOTE_ALREADY_PENDING");
}

#[actix_web::test]
async fn edge_le_quatrieme_devis_est_refuse_et_annule_la_mission() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "plafond").await;
    let jeton = jeton(&app, &email).await;
    let (mission_id, _) = mission(&pool, p.id, "ACCEPTED").await;

    for tour in 1..=3 {
        let reponse =
            test::call_service(&app, devis(&jeton, mission_id, proposition()).to_request()).await;
        assert_eq!(reponse.status(), StatusCode::CREATED, "devis {tour}");
        clore_le_devis(&pool, mission_id, "REFUSED").await;
    }

    let quatrieme =
        test::call_service(&app, devis(&jeton, mission_id, proposition()).to_request()).await;
    assert_eq!(quatrieme.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let rendu: Value = test::read_body_json(quatrieme).await;
    assert_eq!(rendu["code"], "MAX_QUOTES_REACHED");

    let statut: String = sqlx::query_scalar("SELECT statut FROM mission WHERE id = $1")
        .bind(mission_id)
        .fetch_one(&pool)
        .await
        .expect("Mission relue");
    assert_eq!(statut, "CANCELLED", "la Mission doit être annulée");

    // Et l'annulation est consignée : FR-018 `@security` vaut aussi pour celle
    // que le service décide tout seul.
    let consignees: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM mission_transition WHERE mission_id = $1 AND statut = 'CANCELLED'",
    )
    .bind(mission_id)
    .fetch_one(&pool)
    .await
    .expect("historique");
    assert_eq!(consignees, 1);
}

#[actix_web::test]
async fn edge_une_mission_terminee_ne_se_chiffre_plus() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "close").await;
    let jeton = jeton(&app, &email).await;
    let (mission_id, _) = mission(&pool, p.id, "COMPLETED").await;

    let reponse =
        test::call_service(&app, devis(&jeton, mission_id, proposition()).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::CONFLICT);
    let rendu: Value = test::read_body_json(reponse).await;
    assert_eq!(rendu["code"], "MISSION_CLOSED");
}

#[actix_web::test]
async fn edge_un_nouveau_devis_passe_apres_un_refus() {
    // C'est l'autre moitié de l'index partiel : il ne doit bloquer que tant
    // qu'une réponse est attendue, sinon un refus fermerait l'affaire pour de
    // bon alors que FR-016 prévoit trois tentatives.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "apres-refus").await;
    let jeton = jeton(&app, &email).await;
    let (mission_id, _) = mission(&pool, p.id, "ON_SITE").await;

    test::call_service(&app, devis(&jeton, mission_id, proposition()).to_request()).await;
    clore_le_devis(&pool, mission_id, "REFUSED").await;

    let second =
        test::call_service(&app, devis(&jeton, mission_id, proposition()).to_request()).await;
    assert_eq!(second.status(), StatusCode::CREATED);
}

// === @security ===

#[actix_web::test]
async fn security_la_mission_d_un_autre_est_rendue_introuvable() {
    let pool = pool().await;
    let app = bac!(pool);
    let (titulaire, _) = prestataire(&pool, "titulaire").await;
    let (_, email_tiers) = prestataire(&pool, "tiers").await;
    let jeton_tiers = jeton(&app, &email_tiers).await;
    let (mission_id, _) = mission(&pool, titulaire.id, "ACCEPTED").await;

    let reponse = test::call_service(
        &app,
        devis(&jeton_tiers, mission_id, proposition()).to_request(),
    )
    .await;

    // 404 et non 403 : un 403 apprendrait que cet identifiant est celui d'une
    // Mission qui existe.
    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
    let rendu: Value = test::read_body_json(reponse).await;
    assert_eq!(rendu["code"], "MISSION_NOT_FOUND");

    let ecrits: i64 = sqlx::query_scalar("SELECT count(*) FROM devis WHERE mission_id = $1")
        .bind(mission_id)
        .fetch_one(&pool)
        .await
        .expect("comptage");
    assert_eq!(ecrits, 0);
}

#[actix_web::test]
async fn security_un_compte_sans_fiche_prestataire_ne_chiffre_rien() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "vrai").await;
    let (_, email) = compte_actif(&pool, "simple").await;
    let jeton = jeton(&app, &email).await;
    let (mission_id, _) = mission(&pool, p.id, "ACCEPTED").await;

    let reponse =
        test::call_service(&app, devis(&jeton, mission_id, proposition()).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::FORBIDDEN);
    let rendu: Value = test::read_body_json(reponse).await;
    assert_eq!(rendu["code"], "PROVIDER_NOT_ELIGIBLE");
}

#[actix_web::test]
async fn security_sans_jeton_la_route_refuse() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "anonyme").await;
    let (mission_id, _) = mission(&pool, p.id, "ACCEPTED").await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/missions/{mission_id}/quote"))
            .set_json(proposition())
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn security_le_prix_traverse_le_service_sans_etre_touche() {
    // L'invariant §10.2 de bout en bout : ce qui est saisi est ce qui est
    // écrit, sur toute l'échelle admissible. Le jour où une grille tarifaire
    // apparaît quelque part entre la route et la base, ce test tombe.
    let pool = pool().await;
    let app = bac!(pool);
    for cents in [1_i64, 4_999, 18_000, 99_999, 1_000_000] {
        // Un prestataire par montant : « une Mission à la fois » interdit d'en
        // ouvrir plusieurs pour le même.
        let (p, email) = prestataire(&pool, &format!("libre-{cents}")).await;
        let jeton = jeton(&app, &email).await;
        let (mission_id, _) = mission(&pool, p.id, "ACCEPTED").await;
        let mut corps = proposition();
        corps["montant_htva_cents"] = serde_json::json!(cents);
        let reponse = test::call_service(&app, devis(&jeton, mission_id, corps).to_request()).await;
        assert_eq!(reponse.status(), StatusCode::CREATED, "montant {cents}");

        let ecrit: i64 =
            sqlx::query_scalar("SELECT montant_htva_cents FROM devis WHERE mission_id = $1")
                .bind(mission_id)
                .fetch_one(&pool)
                .await
                .expect("devis en base");
        assert_eq!(ecrit, cents);
    }
}

#[actix_web::test]
async fn security_un_champ_inconnu_dans_le_corps_est_refuse() {
    // `deny_unknown_fields` : un `provider_id` glissé dans le corps ne doit pas
    // être ignoré en silence, il doit faire échouer la requête. Ignorer aurait
    // l'air de marcher jusqu'au jour où quelqu'un croit s'en servir.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email) = prestataire(&pool, "inconnu").await;
    let jeton = jeton(&app, &email).await;
    let (mission_id, _) = mission(&pool, p.id, "ACCEPTED").await;

    let mut corps = proposition();
    corps["provider_id"] = serde_json::json!(Uuid::new_v4().to_string());
    let reponse = test::call_service(&app, devis(&jeton, mission_id, corps).to_request()).await;

    assert_eq!(reponse.status(), StatusCode::BAD_REQUEST);
}

// === Réponse du demandeur (FR-017, Story 4.2 sans le séquestre) ===

/// Email du demandeur d'une Demande, pour ouvrir sa session.
async fn email_demandeur(pool: &PoolPg, demande_id: Uuid) -> String {
    sqlx::query_scalar(
        "SELECT u.email FROM utilisateur u
         JOIN demande d ON d.demandeur_id = u.id WHERE d.id = $1",
    )
    .bind(demande_id)
    .fetch_one(pool)
    .await
    .expect("demandeur")
}

fn accepter(jeton: &str, mission_id: Uuid) -> test::TestRequest {
    test::TestRequest::post()
        .uri(&format!("/api/v1/missions/{mission_id}/accept-quote"))
        .insert_header(("Authorization", format!("Bearer {jeton}")))
}

fn refuser(jeton: &str, mission_id: Uuid, corps: Value) -> test::TestRequest {
    test::TestRequest::post()
        .uri(&format!("/api/v1/missions/{mission_id}/refuse-quote"))
        .insert_header(("Authorization", format!("Bearer {jeton}")))
        .set_json(corps)
}

#[actix_web::test]
async fn happy_le_demandeur_accepte_le_devis() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "accepte").await;
    let jeton_p = jeton(&app, &email_p).await;
    let (mission_id, demande_id) = mission(&pool, p.id, "ON_SITE").await;
    test::call_service(
        &app,
        devis(&jeton_p, mission_id, proposition()).to_request(),
    )
    .await;

    let jeton_d = jeton(&app, &email_demandeur(&pool, demande_id).await).await;
    let reponse = test::call_service(&app, accepter(&jeton_d, mission_id).to_request()).await;

    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "QUOTE_ACCEPTED");
    assert_eq!(corps["statut"], "ACCEPTED");

    let (statut, motif): (String, Option<String>) =
        sqlx::query_as("SELECT statut, motif_refus FROM devis WHERE mission_id = $1")
            .bind(mission_id)
            .fetch_one(&pool)
            .await
            .expect("devis relu");
    assert_eq!(statut, "ACCEPTED");
    assert_eq!(motif, None);
}

#[actix_web::test]
async fn happy_le_demandeur_refuse_avec_un_motif() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "refuse-motif").await;
    let jeton_p = jeton(&app, &email_p).await;
    let (mission_id, demande_id) = mission(&pool, p.id, "ON_SITE").await;
    test::call_service(
        &app,
        devis(&jeton_p, mission_id, proposition()).to_request(),
    )
    .await;

    let jeton_d = jeton(&app, &email_demandeur(&pool, demande_id).await).await;
    let reponse = test::call_service(
        &app,
        refuser(
            &jeton_d,
            mission_id,
            serde_json::json!({ "motif": "TOO_EXPENSIVE" }),
        )
        .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "QUOTE_REFUSED");

    let motif: Option<String> =
        sqlx::query_scalar("SELECT motif_refus FROM devis WHERE mission_id = $1")
            .bind(mission_id)
            .fetch_one(&pool)
            .await
            .expect("devis relu");
    assert_eq!(motif.as_deref(), Some("TOO_EXPENSIVE"));
}

#[actix_web::test]
async fn happy_un_refus_libere_la_place_pour_un_nouveau_devis() {
    // FR-017 `@happy` : « le Provider peut émettre un nouveau Devis (jusqu'à 3) ».
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "apres-refus-user").await;
    let jeton_p = jeton(&app, &email_p).await;
    let (mission_id, demande_id) = mission(&pool, p.id, "ON_SITE").await;
    test::call_service(
        &app,
        devis(&jeton_p, mission_id, proposition()).to_request(),
    )
    .await;

    let jeton_d = jeton(&app, &email_demandeur(&pool, demande_id).await).await;
    test::call_service(
        &app,
        refuser(&jeton_d, mission_id, serde_json::json!({})).to_request(),
    )
    .await;

    let second = test::call_service(
        &app,
        devis(&jeton_p, mission_id, proposition()).to_request(),
    )
    .await;
    assert_eq!(second.status(), StatusCode::CREATED);
}

#[actix_web::test]
async fn negative_un_motif_hors_vocabulaire_est_refuse() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "motif-libre").await;
    let jeton_p = jeton(&app, &email_p).await;
    let (mission_id, demande_id) = mission(&pool, p.id, "ON_SITE").await;
    test::call_service(
        &app,
        devis(&jeton_p, mission_id, proposition()).to_request(),
    )
    .await;

    let jeton_d = jeton(&app, &email_demandeur(&pool, demande_id).await).await;
    let reponse = test::call_service(
        &app,
        refuser(
            &jeton_d,
            mission_id,
            serde_json::json!({ "motif": "ce plombier est un voleur" }),
        )
        .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::BAD_REQUEST);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "REASON_UNKNOWN");
}

#[actix_web::test]
async fn negative_sans_devis_en_attente_il_n_y_a_rien_a_accepter() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "sans-devis").await;
    let (mission_id, demande_id) = mission(&pool, p.id, "ON_SITE").await;

    let jeton_d = jeton(&app, &email_demandeur(&pool, demande_id).await).await;
    let reponse = test::call_service(&app, accepter(&jeton_d, mission_id).to_request()).await;

    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "QUOTE_NOT_FOUND");
}

#[actix_web::test]
async fn edge_un_devis_expire_ne_s_accepte_plus() {
    // FR-017 `@edge` : accepter après l'échéance rend 410, et le devis ne bouge
    // pas. Sans cette garde, le prestataire serait engagé sur un prix qu'il ne
    // tient plus.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "devis-expire").await;
    let jeton_p = jeton(&app, &email_p).await;
    let (mission_id, demande_id) = mission(&pool, p.id, "ON_SITE").await;
    test::call_service(
        &app,
        devis(&jeton_p, mission_id, proposition()).to_request(),
    )
    .await;

    // Vieilli directement en base : attendre une heure dans un test n'a pas de
    // sens, et le déclencheur de V20 ne gèle ni `cree_le` ni `expire_le` du
    // même coup — il les gèle, justement, donc l'écriture passe par une
    // désactivation locale du déclencheur pour ce test.
    sqlx::query("ALTER TABLE devis DISABLE TRIGGER devis_contenu_fige")
        .execute(&pool)
        .await
        .expect("déclencheur suspendu");
    sqlx::query(
        "UPDATE devis SET cree_le = now() - interval '2 hours',
                          expire_le = now() - interval '1 hour'
         WHERE mission_id = $1",
    )
    .bind(mission_id)
    .execute(&pool)
    .await
    .expect("devis vieilli");
    sqlx::query("ALTER TABLE devis ENABLE TRIGGER devis_contenu_fige")
        .execute(&pool)
        .await
        .expect("déclencheur rétabli");

    let jeton_d = jeton(&app, &email_demandeur(&pool, demande_id).await).await;
    let reponse = test::call_service(&app, accepter(&jeton_d, mission_id).to_request()).await;

    assert_eq!(reponse.status(), StatusCode::GONE);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "QUOTE_EXPIRED");

    let statut: String = sqlx::query_scalar("SELECT statut FROM devis WHERE mission_id = $1")
        .bind(mission_id)
        .fetch_one(&pool)
        .await
        .expect("devis relu");
    assert_eq!(statut, "SENT", "un refus ne doit rien écrire");
}

#[actix_web::test]
async fn edge_accepter_deux_fois_ne_passe_qu_une_fois() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "double-accept").await;
    let jeton_p = jeton(&app, &email_p).await;
    let (mission_id, demande_id) = mission(&pool, p.id, "ON_SITE").await;
    test::call_service(
        &app,
        devis(&jeton_p, mission_id, proposition()).to_request(),
    )
    .await;

    let jeton_d = jeton(&app, &email_demandeur(&pool, demande_id).await).await;
    let premiere = test::call_service(&app, accepter(&jeton_d, mission_id).to_request()).await;
    assert_eq!(premiere.status(), StatusCode::OK);

    // La seconde ne trouve plus de devis en attente : c'est le même refus que
    // s'il n'y en avait jamais eu, et c'est exact — il n'y a plus rien à
    // accepter.
    let seconde = test::call_service(&app, accepter(&jeton_d, mission_id).to_request()).await;
    assert_eq!(seconde.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn security_le_prestataire_ne_repond_pas_a_son_propre_devis() {
    // Sinon il s'accorderait son prix tout seul, et l'accord du demandeur ne
    // vaudrait plus rien.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "auto-accept").await;
    let jeton_p = jeton(&app, &email_p).await;
    let (mission_id, _) = mission(&pool, p.id, "ON_SITE").await;
    test::call_service(
        &app,
        devis(&jeton_p, mission_id, proposition()).to_request(),
    )
    .await;

    let reponse = test::call_service(&app, accepter(&jeton_p, mission_id).to_request()).await;

    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
    let statut: String = sqlx::query_scalar("SELECT statut FROM devis WHERE mission_id = $1")
        .bind(mission_id)
        .fetch_one(&pool)
        .await
        .expect("devis relu");
    assert_eq!(statut, "SENT");
}

#[actix_web::test]
async fn security_un_tiers_ne_repond_pas_au_devis_d_autrui() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "tiers-devis").await;
    let jeton_p = jeton(&app, &email_p).await;
    let (mission_id, _) = mission(&pool, p.id, "ON_SITE").await;
    test::call_service(
        &app,
        devis(&jeton_p, mission_id, proposition()).to_request(),
    )
    .await;

    let (_, email_tiers) = compte_actif(&pool, "curieux").await;
    let jeton_tiers = jeton(&app, &email_tiers).await;
    let reponse = test::call_service(&app, accepter(&jeton_tiers, mission_id).to_request()).await;

    // 404 et non 403 : la même précédence anti-énumération que partout.
    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "QUOTE_NOT_FOUND");
}

#[actix_web::test]
async fn security_l_acceptation_ne_touche_a_aucun_montant() {
    // L'accord porte sur ce qui a été présenté, au centime près. V20 le grave
    // dans la base ; ce test vérifie que le chemin d'acceptation ne tente même
    // pas de le contourner.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "montant-fige").await;
    let jeton_p = jeton(&app, &email_p).await;
    let (mission_id, demande_id) = mission(&pool, p.id, "ON_SITE").await;
    test::call_service(
        &app,
        devis(&jeton_p, mission_id, proposition()).to_request(),
    )
    .await;

    let jeton_d = jeton(&app, &email_demandeur(&pool, demande_id).await).await;
    test::call_service(&app, accepter(&jeton_d, mission_id).to_request()).await;

    let (htva, tva, ttc): (i64, i64, i64) = sqlx::query_as(
        "SELECT montant_htva_cents, tva_cents, total_ttc_cents FROM devis WHERE mission_id = $1",
    )
    .bind(mission_id)
    .fetch_one(&pool)
    .await
    .expect("devis relu");
    assert_eq!((htva, tva, ttc), (18_000, 3_780, 21_780));
}
