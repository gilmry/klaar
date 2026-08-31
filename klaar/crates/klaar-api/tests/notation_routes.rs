//! Story 7.1 — notation double sens (FR-033), contre un vrai PostgreSQL.
//!
//! **Ce qui ne se teste qu'ici :** l'unicité « une note par côté » que FR-033
//! `@security` exige en base, l'anti-représailles qui retient les deux notes
//! jusqu'à ce que les deux existent, et l'agrégat de réputation mis à jour dans
//! la même transaction que la note.

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
    let email = format!("not-{marqueur}-{id}@example.eu");
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

fn noter(jeton: &str, mission_id: Uuid, corps: Value) -> test::TestRequest {
    test::TestRequest::post()
        .uri(&format!("/api/v1/missions/{mission_id}/rating"))
        .insert_header(("Authorization", format!("Bearer {jeton}")))
        .set_json(corps)
}

fn lire(jeton: &str, mission_id: Uuid) -> test::TestRequest {
    test::TestRequest::get()
        .uri(&format!("/api/v1/missions/{mission_id}/ratings"))
        .insert_header(("Authorization", format!("Bearer {jeton}")))
}

/// Pose une Mission déjà validée, avec sa transition d'historique.
///
/// Écrite directement : le chemin nominal — accepter, avancer, valider —
/// demanderait deux sessions et six requêtes pour arriver au point de départ de
/// ces cas. Les routes de validation ont leurs propres tests.
async fn mission_validee(pool: &PoolPg, provider_id: Uuid, il_y_a_jours: i64) -> (Uuid, String) {
    let (mission_id, email) = mission(pool, provider_id, "VALIDATED").await;
    sqlx::query(
        "INSERT INTO mission_transition
             (mission_id, provider_id, statut, horodate_le, enregistre_le, position, hors_zone)
         VALUES ($1, $2, 'VALIDATED', now() - ($3 || ' days')::interval,
                 now() - ($3 || ' days')::interval, NULL, FALSE)",
    )
    .bind(mission_id)
    .bind(provider_id)
    .bind(il_y_a_jours.to_string())
    .execute(pool)
    .await
    .expect("validation consignée");
    (mission_id, email)
}

/// Réputation agrégée d'un prestataire.
async fn reputation(pool: &PoolPg, provider_id: Uuid) -> Option<(i32, i32)> {
    sqlx::query_as(
        "SELECT somme_notes, nombre_notes FROM reputation_provider WHERE provider_id = $1",
    )
    .bind(provider_id)
    .fetch_optional(pool)
    .await
    .expect("réputation")
}

// === @happy ===

#[actix_web::test]
async fn happy_le_demandeur_note_le_prestataire() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "note-simple").await;
    let (mission_id, email) = mission_validee(&pool, p.id, 0).await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        noter(
            &jeton,
            mission_id,
            serde_json::json!({ "note": 5, "commentaire": "Intervention parfaite" }),
        )
        .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::CREATED);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "RATING_RECORDED");
    assert_eq!(corps["cible"], "PROVIDER");
    // Seule, elle reste cachée : l'autre partie n'a pas encore noté.
    assert_eq!(corps["publiee"], false);

    assert_eq!(reputation(&pool, p.id).await, Some((5, 1)));
}

#[actix_web::test]
async fn happy_les_deux_notes_se_publient_ensemble() {
    // FR-033 `@happy` : anti-représailles. Publier la première laisserait
    // l'autre ajuster la sienne, et les deux perdraient toute valeur.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "double-sens").await;
    let (mission_id, email_d) = mission_validee(&pool, p.id, 0).await;
    let jeton_d = jeton(&app, &email_d).await;
    let jeton_p = jeton(&app, &email_p).await;

    test::call_service(
        &app,
        noter(&jeton_d, mission_id, serde_json::json!({ "note": 5 })).to_request(),
    )
    .await;

    // Rien de visible tant que le prestataire n'a pas noté.
    let avant: Value = test::read_body_json(
        test::call_service(&app, lire(&jeton_d, mission_id).to_request()).await,
    )
    .await;
    assert_eq!(avant["notes"].as_array().unwrap().len(), 0);

    let seconde = test::call_service(
        &app,
        noter(&jeton_p, mission_id, serde_json::json!({ "note": 4 })).to_request(),
    )
    .await;
    let corps: Value = test::read_body_json(seconde).await;
    assert_eq!(corps["cible"], "USER");
    assert_eq!(corps["publiee"], true);

    let apres: Value = test::read_body_json(
        test::call_service(&app, lire(&jeton_d, mission_id).to_request()).await,
    )
    .await;
    assert_eq!(apres["notes"].as_array().unwrap().len(), 2);
}

// === @negative ===

#[actix_web::test]
async fn negative_une_note_hors_echelle_est_refusee() {
    // FR-033 `@negative` : ni zéro ni six.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "hors-echelle").await;
    let (mission_id, email) = mission_validee(&pool, p.id, 0).await;
    let jeton = jeton(&app, &email).await;

    for note in [0, 6, 42] {
        let reponse = test::call_service(
            &app,
            noter(&jeton, mission_id, serde_json::json!({ "note": note })).to_request(),
        )
        .await;
        assert_eq!(
            reponse.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "note {note}"
        );
        let corps: Value = test::read_body_json(reponse).await;
        assert_eq!(corps["code"], "RATING_OUT_OF_RANGE");
    }
    assert_eq!(
        reputation(&pool, p.id).await,
        None,
        "rien ne doit être agrégé"
    );
}

#[actix_web::test]
async fn negative_un_commentaire_trop_long_est_refuse() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "commentaire-long").await;
    let (mission_id, email) = mission_validee(&pool, p.id, 0).await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        noter(
            &jeton,
            mission_id,
            serde_json::json!({ "note": 5, "commentaire": "x".repeat(501) }),
        )
        .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "COMMENT_TOO_LONG");
}

#[actix_web::test]
async fn negative_une_intervention_non_validee_ne_se_note_pas() {
    // Noter avant que quelqu'un ait dit que c'était fini reviendrait à juger un
    // travail en cours.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "non-validee").await;
    let (mission_id, email) = mission(&pool, p.id, "COMPLETED").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        noter(&jeton, mission_id, serde_json::json!({ "note": 5 })).to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::CONFLICT);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "MISSION_NOT_VALIDATED");
}

// === @edge ===

#[actix_web::test]
async fn edge_la_fenetre_se_ferme_apres_quatorze_jours() {
    // FR-033 `@edge` : 410 après quinze jours.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "fenetre").await;
    let (mission_id, email) = mission_validee(&pool, p.id, 15).await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        noter(&jeton, mission_id, serde_json::json!({ "note": 5 })).to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::GONE);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "RATING_WINDOW_CLOSED");
}

#[actix_web::test]
async fn edge_la_fermeture_de_la_fenetre_publie_ce_qui_existe() {
    // Celui qui n'a pas noté a eu deux semaines : sa note ne viendra plus, et
    // retenir celle de l'autre pour toujours n'aurait plus d'objet.
    //
    // La note est écrite directement en base : la poser par la route
    // demanderait une fenêtre ouverte, et la refermer ensuite est impossible —
    // `mission_transition` est append-only, et c'est très bien. Ce cas porte sur
    // la **lecture**, pas sur l'écriture.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "publication-tardive").await;
    let (mission_id, email) = mission_validee(&pool, p.id, 15).await;
    let jeton = jeton(&app, &email).await;

    let (demandeur_id,): (Uuid,) = sqlx::query_as(
        "SELECT d.demandeur_id FROM demande d JOIN mission m ON m.demande_id = d.id WHERE m.id = $1",
    )
    .bind(mission_id)
    .fetch_one(&pool)
    .await
    .expect("demandeur");
    sqlx::query(
        "INSERT INTO notation (id, mission_id, auteur_id, cible, note, commentaire, cree_le)
         VALUES ($1, $2, $3, 'PROVIDER', 5, NULL, now() - interval '14 days')",
    )
    .bind(Uuid::new_v4())
    .bind(mission_id)
    .bind(demandeur_id)
    .execute(&pool)
    .await
    .expect("note posée avant la fermeture");

    let apres: Value =
        test::read_body_json(test::call_service(&app, lire(&jeton, mission_id).to_request()).await)
            .await;
    assert_eq!(
        apres["notes"].as_array().unwrap().len(),
        1,
        "la fenêtre close publie la note isolée"
    );
}

#[actix_web::test]
async fn edge_l_agregat_cumule_les_notes_successives() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "cumul").await;

    for note in [5, 3] {
        let (mission_id, email) = mission_validee(&pool, p.id, 0).await;
        let jeton = jeton(&app, &email).await;
        test::call_service(
            &app,
            noter(&jeton, mission_id, serde_json::json!({ "note": note })).to_request(),
        )
        .await;
    }

    assert_eq!(reputation(&pool, p.id).await, Some((8, 2)));
}

// === @security ===

#[actix_web::test]
async fn security_une_seconde_note_du_meme_cote_est_refusee() {
    // FR-033 `@security` : « la contrainte unique est en base, la tentative de
    // double est techniquement impossible ».
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "double-note").await;
    let (mission_id, email) = mission_validee(&pool, p.id, 0).await;
    let jeton = jeton(&app, &email).await;

    let premiere = test::call_service(
        &app,
        noter(&jeton, mission_id, serde_json::json!({ "note": 5 })).to_request(),
    )
    .await;
    assert_eq!(premiere.status(), StatusCode::CREATED);

    let seconde = test::call_service(
        &app,
        noter(&jeton, mission_id, serde_json::json!({ "note": 1 })).to_request(),
    )
    .await;
    assert_eq!(seconde.status(), StatusCode::CONFLICT);
    let corps: Value = test::read_body_json(seconde).await;
    assert_eq!(corps["code"], "ALREADY_RATED");

    // Et l'agrégat n'a pas bougé : une seconde note refusée ne doit pas
    // compter, sinon on se noterait en boucle.
    assert_eq!(reputation(&pool, p.id).await, Some((5, 1)));
}

#[actix_web::test]
async fn security_un_tiers_ne_note_pas_une_intervention_d_autrui() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "tiers-note").await;
    let (mission_id, _) = mission_validee(&pool, p.id, 0).await;
    let (_, email_tiers) = compte_actif(&pool, "curieux").await;
    let jeton_tiers = jeton(&app, &email_tiers).await;

    let reponse = test::call_service(
        &app,
        noter(&jeton_tiers, mission_id, serde_json::json!({ "note": 1 })).to_request(),
    )
    .await;

    // 404 et non 403 : la même précédence anti-énumération que partout.
    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
    assert_eq!(reputation(&pool, p.id).await, None);
}

#[actix_web::test]
async fn security_la_note_du_prestataire_ne_gonfle_pas_sa_propre_reputation() {
    // Le prestataire note le **demandeur**. Si sa note alimentait l'agrégat, il
    // lui suffirait de se mettre cinq étoiles à chaque intervention.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "auto-reputation").await;
    let (mission_id, _) = mission_validee(&pool, p.id, 0).await;
    let jeton_p = jeton(&app, &email_p).await;

    let reponse = test::call_service(
        &app,
        noter(&jeton_p, mission_id, serde_json::json!({ "note": 5 })).to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::CREATED);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["cible"], "USER");

    assert_eq!(
        reputation(&pool, p.id).await,
        None,
        "noter le demandeur ne doit rien ajouter à sa propre réputation"
    );
}

#[actix_web::test]
async fn security_une_note_ecrite_ne_se_modifie_plus() {
    // Une note est un avis daté : la modifier après coup permettrait de la
    // retourner sous pression.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "note-figee").await;
    let (mission_id, email) = mission_validee(&pool, p.id, 0).await;
    let jeton = jeton(&app, &email).await;
    test::call_service(
        &app,
        noter(&jeton, mission_id, serde_json::json!({ "note": 1 })).to_request(),
    )
    .await;

    let refus = sqlx::query("UPDATE notation SET note = 5 WHERE mission_id = $1")
        .bind(mission_id)
        .execute(&pool)
        .await
        .expect_err("la note doit être figée");
    assert!(
        refus.to_string().contains("ne se modifie plus"),
        "déclencheur attendu, obtenu : {refus}"
    );
}

#[actix_web::test]
async fn security_la_lecture_des_notes_exige_un_jeton() {
    // Une note publiée est publique, mais la réputation ne doit pas s'aspirer
    // anonymement en masse.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "lecture-anonyme").await;
    let (mission_id, _) = mission_validee(&pool, p.id, 0).await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/v1/missions/{mission_id}/ratings"))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED);
}
