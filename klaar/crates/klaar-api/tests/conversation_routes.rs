//! Story 6.1 — conversation et anti-contournement (FR-030, FR-032).
//!
//! **Ce qui ne se teste qu'ici :** la fermeture de la conversation sept jours
//! après la fin de l'intervention, qui se lit dans l'historique des
//! transitions ; le comptage des tentatives d'échange de coordonnées ; et le
//! fait qu'un message refusé ne laisse aucune trace de son contenu.

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
    let email = format!("msg-{marqueur}-{id}@example.eu");
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

fn ecrire(jeton: &str, mission_id: Uuid, corps: &str) -> test::TestRequest {
    test::TestRequest::post()
        .uri(&format!("/api/v1/missions/{mission_id}/messages"))
        .insert_header(("Authorization", format!("Bearer {jeton}")))
        .set_json(serde_json::json!({ "corps": corps }))
}

fn lire(jeton: &str, mission_id: Uuid) -> test::TestRequest {
    test::TestRequest::get()
        .uri(&format!("/api/v1/missions/{mission_id}/messages"))
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
async fn happy_les_deux_parties_echangent() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "echange").await;
    let (mission_id, email_d) = mission(&pool, p.id, "ON_SITE").await;
    let jeton_d = jeton(&app, &email_d).await;
    let jeton_p = jeton(&app, &email_p).await;

    let envoi = test::call_service(
        &app,
        ecrire(&jeton_d, mission_id, "Bonjour, où êtes-vous ?").to_request(),
    )
    .await;
    assert_eq!(envoi.status(), StatusCode::CREATED);
    let corps: Value = test::read_body_json(envoi).await;
    assert_eq!(corps["code"], "MESSAGE_SENT");

    test::call_service(
        &app,
        ecrire(&jeton_p, mission_id, "J'arrive dans dix minutes.").to_request(),
    )
    .await;

    let fil: Value = test::read_body_json(
        test::call_service(&app, lire(&jeton_d, mission_id).to_request()).await,
    )
    .await;
    let messages = fil["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 2);
    // Le premier est du demandeur, le second du prestataire : c'est ce que
    // `de_moi` permet d'afficher du bon côté sans exposer d'identifiant.
    assert_eq!(messages[0]["de_moi"], true);
    assert_eq!(messages[1]["de_moi"], false);
}

// === @negative ===

#[actix_web::test]
async fn negative_un_message_trop_long_est_refuse() {
    // FR-030 `@negative` : 422 au-delà de quatre mille caractères.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "trop-long").await;
    let (mission_id, email) = mission(&pool, p.id, "ON_SITE").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        ecrire(&jeton, mission_id, &"x".repeat(4_001)).to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "MESSAGE_TOO_LONG");
}

#[actix_web::test]
async fn negative_une_conversation_close_depuis_sept_jours_refuse() {
    // FR-030 `@negative` : 410.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "close").await;
    let (mission_id, email) = mission(&pool, p.id, "COMPLETED").await;
    close_il_y_a(&pool, mission_id, p.id, 8).await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        ecrire(&jeton, mission_id, "encore une question").to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::GONE);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "CONVERSATION_CLOSED");
}

// === @edge ===

#[actix_web::test]
async fn edge_une_conversation_close_hier_reste_ouverte() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "close-hier").await;
    let (mission_id, email) = mission(&pool, p.id, "COMPLETED").await;
    close_il_y_a(&pool, mission_id, p.id, 1).await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        ecrire(&jeton, mission_id, "merci pour tout").to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::CREATED);
}

#[actix_web::test]
async fn edge_un_message_vide_est_refuse() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "vide").await;
    let (mission_id, email) = mission(&pool, p.id, "ON_SITE").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(&app, ecrire(&jeton, mission_id, "   ").to_request()).await;
    assert_eq!(reponse.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "MESSAGE_EMPTY");
}

// === @security ===

#[actix_web::test]
async fn security_un_numero_de_telephone_est_refuse_et_compte() {
    // FR-030 `@security` et FR-032 : bloqué, et la tentative est consignée.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "numero").await;
    let (mission_id, email) = mission(&pool, p.id, "ON_SITE").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        ecrire(&jeton, mission_id, "appelez-moi au 0470 12 34 56").to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::FORBIDDEN);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "CONTACT_INFO_FORBIDDEN");
    assert_eq!(corps["tentatives"], 1);
    assert_eq!(corps["signale"], false);

    // Rien n'a été écrit dans le fil.
    let ecrits: i64 = sqlx::query_scalar("SELECT count(*) FROM message WHERE mission_id = $1")
        .bind(mission_id)
        .fetch_one(&pool)
        .await
        .expect("comptage");
    assert_eq!(ecrits, 0);
}

#[actix_web::test]
async fn security_le_message_refuse_n_est_pas_conserve() {
    // **Garder le texte reviendrait à constituer un fichier de ce que les gens
    // ont essayé de s'écrire**, pour une finalité — compter les récidives — qui
    // n'en a pas besoin. La tentative est consignée, pas son contenu.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "sans-trace").await;
    let (mission_id, email) = mission(&pool, p.id, "ON_SITE").await;
    let jeton = jeton(&app, &email).await;

    test::call_service(
        &app,
        ecrire(&jeton, mission_id, "mon numéro : 0470123456").to_request(),
    )
    .await;

    let (genre,): (String,) =
        sqlx::query_as("SELECT genre FROM tentative_contournement WHERE mission_id = $1")
            .bind(mission_id)
            .fetch_one(&pool)
            .await
            .expect("tentative consignée");
    assert_eq!(genre, "PHONE");

    // Et le numéro ne se retrouve nulle part.
    let colonnes: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns
         WHERE table_name = 'tentative_contournement'",
    )
    .fetch_all(&pool)
    .await
    .expect("colonnes");
    assert!(
        !colonnes.iter().any(|c| c == "corps" || c == "message"),
        "la table ne doit pas porter le texte : {colonnes:?}"
    );
}

#[actix_web::test]
async fn security_la_troisieme_tentative_est_signalee() {
    // FR-032 `@security` : trois tentatives valent un signalement.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "recidive-msg").await;
    let (mission_id, email) = mission(&pool, p.id, "ON_SITE").await;
    let jeton = jeton(&app, &email).await;

    for tour in 1..=3 {
        let reponse = test::call_service(
            &app,
            ecrire(&jeton, mission_id, "écrivez à moi@exemple.eu").to_request(),
        )
        .await;
        assert_eq!(reponse.status(), StatusCode::FORBIDDEN, "tentative {tour}");
        let corps: Value = test::read_body_json(reponse).await;
        assert_eq!(corps["tentatives"], tour);
        assert_eq!(
            corps["signale"],
            tour == 3,
            "le signalement tombe à la troisième, pas avant"
        );
    }
}

#[actix_web::test]
async fn security_un_message_legitime_avec_des_chiffres_passe() {
    // Le faux positif coûte plus cher que le faux négatif : un message bloqué
    // est une conversation cassée entre deux personnes qui ont un problème à
    // régler.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "chiffres").await;
    let (mission_id, email) = mission(&pool, p.id, "ON_SITE").await;
    let jeton = jeton(&app, &email).await;

    for legitime in [
        "on dit le 24/12/2026 à 14h ?",
        "j'ai 47 ans et 2 enfants",
        "le devis est à 180,50 €",
        "l'immeuble est au 12, troisième étage",
    ] {
        let reponse =
            test::call_service(&app, ecrire(&jeton, mission_id, legitime).to_request()).await;
        assert_eq!(reponse.status(), StatusCode::CREATED, "{legitime}");
    }
}

#[actix_web::test]
async fn security_un_tiers_ne_lit_pas_la_conversation_d_autrui() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "tiers-msg").await;
    let (mission_id, email_d) = mission(&pool, p.id, "ON_SITE").await;
    let jeton_d = jeton(&app, &email_d).await;
    test::call_service(&app, ecrire(&jeton_d, mission_id, "un secret").to_request()).await;

    let (_, email_tiers) = compte_actif(&pool, "curieux").await;
    let jeton_tiers = jeton(&app, &email_tiers).await;

    let lecture = test::call_service(&app, lire(&jeton_tiers, mission_id).to_request()).await;
    assert_eq!(lecture.status(), StatusCode::NOT_FOUND);

    let ecriture = test::call_service(
        &app,
        ecrire(&jeton_tiers, mission_id, "coucou").to_request(),
    )
    .await;
    assert_eq!(ecriture.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn security_un_message_envoye_ne_se_reecrit_pas() {
    // Une conversation sert de trace de ce qui a été convenu ; pouvoir la
    // réécrire après coup la viderait de tout intérêt en cas de désaccord.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "fige-msg").await;
    let (mission_id, email) = mission(&pool, p.id, "ON_SITE").await;
    let jeton = jeton(&app, &email).await;
    test::call_service(
        &app,
        ecrire(&jeton, mission_id, "on avait dit 14h").to_request(),
    )
    .await;

    let refus = sqlx::query("UPDATE message SET corps = 'on avait dit 16h' WHERE mission_id = $1")
        .bind(mission_id)
        .execute(&pool)
        .await
        .expect_err("le message doit être figé");
    assert!(
        refus.to_string().contains("ne se réécrit pas"),
        "déclencheur attendu, obtenu : {refus}"
    );
}

#[actix_web::test]
async fn security_la_conversation_exige_un_jeton() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "anonyme-msg").await;
    let (mission_id, _) = mission(&pool, p.id, "ON_SITE").await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/v1/missions/{mission_id}/messages"))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED);
}
