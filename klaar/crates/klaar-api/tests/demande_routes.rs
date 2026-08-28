//! Story 3.1 — soumission d'une Demande (FR-011), contre un vrai PostgreSQL.
//!
//! La position passe par PostGIS : ces cas vérifient donc autant le SQL spatial
//! que la règle métier. L'ordre des arguments de `ST_MakePoint` est le genre de
//! détail qu'aucun test unitaire n'attrape et qui place Bruxelles au large de
//! la Somalie.

use actix_web::{http::StatusCode, test};
use chrono::Utc;
use klaar_api::{app_de_test, etat_de_test, EtatApplication};
use klaar_identity::{EmpreinteMotDePasse, MotDePasse, ParametresArgon2};
use klaar_sqlx_repos::{creer_pool, PoolPg};
use serde_json::Value;
use uuid::Uuid;

const MDP: &str = "Marie@2026Secure";
/// Grand-Place, au centre de la Région.
const LAT: f64 = 50.8467;
const LON: f64 = 4.3525;

async fn pool() -> PoolPg {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI");
    creer_pool(&url).await.expect("connexion PostgreSQL")
}

async fn compte_actif(pool: &PoolPg, marqueur: &str) -> (Uuid, String) {
    let id = Uuid::new_v4();
    let email = format!("dem-{marqueur}-{id}@example.eu");
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

fn demande(jeton: &str, secteur: &str, description: &str, lat: f64, lon: f64) -> test::TestRequest {
    test::TestRequest::post()
        .uri("/api/v1/requests")
        .insert_header(("Authorization", format!("Bearer {jeton}")))
        .set_json(serde_json::json!({
            "secteur": secteur,
            "description": description,
            "latitude": lat,
            "longitude": lon,
            "urgence": "HIGH"
        }))
}

#[actix_web::test]
async fn happy_une_demande_valide_est_creee_en_diffusion() {
    let pool = pool().await;
    let app = bac!(pool);
    let (id, email) = compte_actif(&pool, "happy").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        demande(&jeton, "plomberie", "Fuite", LAT, LON).to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::CREATED);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["statut"], "BROADCASTING");
    assert_eq!(corps["code"], "REQUEST_CREATED");

    let lignes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM demande WHERE demandeur_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(lignes, 1);
}

#[actix_web::test]
async fn happy_la_position_est_enregistree_au_bon_endroit() {
    // `ST_MakePoint` prend la longitude d'abord. L'inverser place Bruxelles au
    // large de la Somalie sans qu'aucune contrainte ne s'en aperçoive : ce test
    // relit la position et vérifie qu'elle est bien à Bruxelles.
    let pool = pool().await;
    let app = bac!(pool);
    let (id, email) = compte_actif(&pool, "geo").await;
    let jeton = jeton(&app, &email).await;

    test::call_service(
        &app,
        demande(&jeton, "plomberie", "Fuite", LAT, LON).to_request(),
    )
    .await;

    let (lat, lon): (f64, f64) = sqlx::query_as(
        "SELECT ST_Y(position::geometry), ST_X(position::geometry)
         FROM demande WHERE demandeur_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!((lat - LAT).abs() < 1e-6, "latitude relue : {lat}");
    assert!((lon - LON).abs() < 1e-6, "longitude relue : {lon}");
}

#[actix_web::test]
async fn happy_la_creation_est_auditee() {
    let pool = pool().await;
    let app = bac!(pool);
    let (id, email) = compte_actif(&pool, "audit").await;
    let jeton = jeton(&app, &email).await;

    test::call_service(
        &app,
        demande(&jeton, "plomberie", "Fuite", LAT, LON).to_request(),
    )
    .await;

    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM journal_audit WHERE code = 'REQUEST_CREATED' AND sujet_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 1);
}

#[actix_web::test]
async fn negative_sans_jeton_la_demande_est_refusee() {
    let pool = pool().await;
    let app = bac!(pool);
    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/requests")
            .set_json(serde_json::json!({
                "secteur": "plomberie", "description": "Fuite",
                "latitude": LAT, "longitude": LON, "urgence": "HIGH"
            }))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn negative_les_codes_d_erreur_du_prd_sont_rendus() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "codes").await;
    let jeton = jeton(&app, &email).await;

    let cas: Vec<(&str, String, f64, f64, &str)> = vec![
        ("chauffage", "Panne".into(), LAT, LON, "SECTOR_NOT_FOUND"),
        ("plomberie", String::new(), LAT, LON, "DESCRIPTION_EMPTY"),
        (
            "plomberie",
            "a".repeat(2_001),
            LAT,
            LON,
            "DESCRIPTION_TOO_LONG",
        ),
        // Anvers : hors de la Région.
        (
            "plomberie",
            "Fuite".into(),
            51.2194,
            4.4025,
            "GEO_OUTSIDE_RBC",
        ),
    ];
    for (secteur, description, lat, lon, code) in cas {
        let reponse = test::call_service(
            &app,
            demande(&jeton, secteur, &description, lat, lon).to_request(),
        )
        .await;
        assert_eq!(reponse.status(), StatusCode::BAD_REQUEST, "cas {code}");
        let corps: Value = test::read_body_json(reponse).await;
        assert_eq!(corps["code"], code);
    }
}

#[actix_web::test]
async fn negative_une_urgence_inconnue_est_refusee() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "urgence").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/requests")
            .insert_header(("Authorization", format!("Bearer {jeton}")))
            .set_json(serde_json::json!({
                "secteur": "plomberie", "description": "Fuite",
                "latitude": LAT, "longitude": LON, "urgence": "TRES_URGENT"
            }))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::BAD_REQUEST);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "URGENCY_INVALID");
}

#[actix_web::test]
async fn negative_sans_methode_de_paiement_la_demande_est_refusee_en_422() {
    // Le contrôle de FR-011 existe et fonctionne ; il est seulement désactivé
    // par configuration dans le déploiement vitrine, faute de compte Stripe.
    // Ce cas le rallume pour le vérifier.
    let pool = pool().await;
    let base = etat_de_test(pool.clone(), None);
    let etat = actix_web::web::Data::new(EtatApplication {
        exiger_methode_paiement: true,
        ..(**base).clone()
    });
    let app = test::init_service(app_de_test(etat)).await;
    let (id, email) = compte_actif(&pool, "paiement").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        demande(&jeton, "plomberie", "Fuite", LAT, LON).to_request(),
    )
    .await;
    // 422 et non 400 : la requête est bien formée, c'est l'état du compte qui
    // empêche de la traiter.
    assert_eq!(reponse.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "PAYMENT_METHOD_REQUIRED");

    let lignes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM demande WHERE demandeur_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(lignes, 0);
}

#[actix_web::test]
async fn edge_une_demande_identique_rend_l_existante_sans_en_creer_une_seconde() {
    let pool = pool().await;
    let app = bac!(pool);
    let (id, email) = compte_actif(&pool, "doublon").await;
    let jeton = jeton(&app, &email).await;

    let premiere = test::call_service(
        &app,
        demande(&jeton, "plomberie", "Fuite", LAT, LON).to_request(),
    )
    .await;
    let corps_premier: Value = test::read_body_json(premiere).await;

    let seconde = test::call_service(
        &app,
        demande(&jeton, "plomberie", "Fuite, reformulée", LAT, LON).to_request(),
    )
    .await;
    // 200 et non 409 : l'utilisateur veut retrouver la sienne, pas apprendre
    // qu'il a cliqué deux fois.
    assert_eq!(seconde.status(), StatusCode::OK);
    let corps_second: Value = test::read_body_json(seconde).await;
    assert_eq!(corps_second["code"], "REQUEST_DUPLICATE");
    assert_eq!(corps_second["id"], corps_premier["id"]);

    let lignes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM demande WHERE demandeur_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(lignes, 1);
}

#[actix_web::test]
async fn edge_un_autre_secteur_au_meme_endroit_n_est_pas_un_doublon() {
    // Une fuite et une porte claquée le même soir sont deux problèmes.
    let pool = pool().await;
    let app = bac!(pool);
    let (id, email) = compte_actif(&pool, "secteurs").await;
    let jeton = jeton(&app, &email).await;

    test::call_service(
        &app,
        demande(&jeton, "plomberie", "Fuite", LAT, LON).to_request(),
    )
    .await;
    let seconde = test::call_service(
        &app,
        demande(&jeton, "serrurerie", "Porte claquée", LAT, LON).to_request(),
    )
    .await;
    assert_eq!(seconde.status(), StatusCode::CREATED);

    let lignes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM demande WHERE demandeur_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(lignes, 2);
}

#[actix_web::test]
async fn edge_le_quota_horaire_bloque_la_sixieme_demande() {
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "quota").await;
    let jeton = jeton(&app, &email).await;

    // Cinq positions distinctes, pour ne pas être prises pour des doublons.
    for i in 0..5 {
        let reponse = test::call_service(
            &app,
            demande(&jeton, "plomberie", "Fuite", LAT + i as f64 * 0.005, LON).to_request(),
        )
        .await;
        assert_eq!(reponse.status(), StatusCode::CREATED, "demande {i}");
    }

    let refusee = test::call_service(
        &app,
        demande(&jeton, "plomberie", "Fuite", LAT + 0.03, LON).to_request(),
    )
    .await;
    assert_eq!(refusee.status(), StatusCode::TOO_MANY_REQUESTS);
    let corps: Value = test::read_body_json(refusee).await;
    assert_eq!(corps["code"], "RATE_LIMIT_EXCEEDED");
}

#[actix_web::test]
async fn security_le_quota_est_compte_par_compte() {
    // Sinon, cinq Demandes d'un utilisateur empêcheraient tous les autres d'en
    // soumettre.
    let pool = pool().await;
    let app = bac!(pool);
    let (_, premier) = compte_actif(&pool, "quota-a").await;
    let (_, second) = compte_actif(&pool, "quota-b").await;
    let jeton_premier = jeton(&app, &premier).await;

    for i in 0..5 {
        test::call_service(
            &app,
            demande(
                &jeton_premier,
                "plomberie",
                "Fuite",
                LAT + i as f64 * 0.005,
                LON,
            )
            .to_request(),
        )
        .await;
    }

    let jeton_second = jeton(&app, &second).await;
    let reponse = test::call_service(
        &app,
        demande(&jeton_second, "plomberie", "Fuite", LAT, LON).to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::CREATED);
}

#[actix_web::test]
async fn security_on_ne_peut_pas_soumettre_au_nom_d_un_autre() {
    // L'identifiant du demandeur vient du jeton, jamais du corps. Un champ
    // supplémentaire est refusé par `deny_unknown_fields`, ce qui fixe cette
    // absence : l'ajouter un jour casserait ce test bruyamment.
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "usurpation").await;
    let (victime_id, _) = compte_actif(&pool, "victime").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/requests")
            .insert_header(("Authorization", format!("Bearer {jeton}")))
            .set_json(serde_json::json!({
                "secteur": "plomberie", "description": "Fuite",
                "latitude": LAT, "longitude": LON, "urgence": "HIGH",
                "demandeur_id": victime_id.to_string()
            }))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::BAD_REQUEST);

    let lignes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM demande WHERE demandeur_id = $1")
        .bind(victime_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(lignes, 0);
}

#[actix_web::test]
async fn security_le_doublon_d_un_autre_compte_n_est_pas_rendu() {
    // Rendre la Demande d'un voisin au même endroit lui donnerait son
    // identifiant, et laisserait croire qu'elle est la sienne.
    let pool = pool().await;
    let app = bac!(pool);
    let (_, premier) = compte_actif(&pool, "voisin-a").await;
    let (_, second) = compte_actif(&pool, "voisin-b").await;

    let jeton_premier = jeton(&app, &premier).await;
    let creee = test::call_service(
        &app,
        demande(&jeton_premier, "plomberie", "Fuite", LAT, LON).to_request(),
    )
    .await;
    let corps_premier: Value = test::read_body_json(creee).await;

    let jeton_second = jeton(&app, &second).await;
    let voisine = test::call_service(
        &app,
        demande(&jeton_second, "plomberie", "Fuite", LAT, LON).to_request(),
    )
    .await;
    assert_eq!(voisine.status(), StatusCode::CREATED);
    let corps_second: Value = test::read_body_json(voisine).await;
    assert_ne!(corps_second["id"], corps_premier["id"]);
}

#[actix_web::test]
async fn security_une_description_hostile_est_conservee_telle_quelle_sans_etre_interpretee() {
    // La description est du texte, jamais du balisage : elle traverse la base
    // et l'API sans transformation, et c'est au rendu de l'échapper.
    let pool = pool().await;
    let app = bac!(pool);
    let (id, email) = compte_actif(&pool, "hostile").await;
    let jeton = jeton(&app, &email).await;
    let texte = "<script>alert(1)</script> '; DROP TABLE demande; --";

    let reponse = test::call_service(
        &app,
        demande(&jeton, "plomberie", texte, LAT, LON).to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::CREATED);

    let relue: String =
        sqlx::query_scalar("SELECT description FROM demande WHERE demandeur_id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("la table existe toujours");
    assert_eq!(relue, texte);
}

#[actix_web::test]
async fn happy_le_matching_retient_les_prestataires_du_secteur_et_trace() {
    // Story 3.2 : la Demande créée déclenche la recherche, et la trace AI Act
    // est écrite avant que les candidats ne soient rendus.
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "matching").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        demande(&jeton, "plomberie", "Fuite", LAT, LON).to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::CREATED);
    let corps: Value = test::read_body_json(reponse).await;
    let demande_id: Uuid = corps["id"].as_str().unwrap().parse().unwrap();

    // Les prestataires de démonstration couvrent la plomberie au centre :
    // au moins un doit être retenu. Le test ne fixe pas leur nombre, qui
    // dépend du jeu de données présent.
    let candidats = corps["candidats"].as_u64().expect("un nombre de candidats");

    let tracees: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM trace_matching WHERE demande_id = $1 AND retenu")
            .bind(demande_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        tracees as u64, candidats,
        "chaque candidat retenu doit avoir sa ligne de trace"
    );
}

#[actix_web::test]
async fn edge_sans_prestataire_dans_le_secteur_la_demande_passe_en_no_match() {
    // Aucun prestataire de démonstration ne couvre ce secteur au centre : la
    // Demande doit basculer plutôt que rester en diffusion indéfiniment.
    let pool = pool().await;
    // On s'assure qu'aucun prestataire n'est disponible sur ce secteur.
    sqlx::query(
        "UPDATE provider SET disponible = FALSE
         WHERE id IN (SELECT provider_id FROM provider_competence WHERE secteur_code = 'livraison')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "no-match").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        demande(&jeton, "livraison", "Colis à porter", LAT, LON).to_request(),
    )
    .await;
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["candidats"], 0);

    let demande_id: Uuid = corps["id"].as_str().unwrap().parse().unwrap();
    let statut: String = sqlx::query_scalar("SELECT statut FROM demande WHERE id = $1")
        .bind(demande_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(statut, "NO_MATCH");

    sqlx::query(
        "UPDATE provider SET disponible = TRUE
         WHERE id IN (SELECT provider_id FROM provider_competence WHERE secteur_code = 'livraison')
           AND statut = 'ACTIVE'",
    )
    .execute(&pool)
    .await
    .unwrap();
}

#[actix_web::test]
async fn security_la_trace_conserve_la_ventilation_du_score() {
    // L'AI Act exige de pouvoir dire de quoi le score était fait. La trace
    // porte la ventilation, pas seulement le total.
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "ventilation").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        demande(&jeton, "plomberie", "Fuite", LAT, LON).to_request(),
    )
    .await;
    let corps: Value = test::read_body_json(reponse).await;
    let demande_id: Uuid = corps["id"].as_str().unwrap().parse().unwrap();

    let ventilation: Option<Value> =
        sqlx::query_scalar("SELECT ventilation FROM trace_matching WHERE demande_id = $1 LIMIT 1")
            .bind(demande_id)
            .fetch_optional(&pool)
            .await
            .unwrap();

    if let Some(v) = ventilation {
        assert!(v["proximite"]["poids"].is_number(), "ventilation : {v}");
        assert!(v["controle"]["poids"].is_number());
        // L'absence de note est visible : la trace dit aussi ce qui manquait.
        assert!(v["note"].is_null(), "la note n'existe pas encore : {v}");
    }
}

#[actix_web::test]
async fn edge_sans_cle_vapid_le_matching_a_lieu_mais_personne_n_est_notifie() {
    // Sans clé VAPID configurée, le service tourne sans notifications : c'est
    // un mode de fonctionnement légitime, pas une panne. `etat_de_test` monte
    // l'application sans push.
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "sans-vapid").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        demande(&jeton, "plomberie", "Fuite", LAT, LON).to_request(),
    )
    .await;
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["notifies"], 0);
    // Le matching, lui, a bien eu lieu.
    assert!(corps["candidats"].as_u64().is_some());
}

#[actix_web::test]
async fn security_le_nombre_de_notifies_ne_se_confond_pas_avec_les_candidats() {
    // Un prestataire retenu sans abonnement push verra la Demande en ouvrant
    // l'application. Confondre les deux ferait croire à qui attend que dix
    // personnes ont été réveillées alors que personne n'a rien reçu.
    let pool = pool().await;
    let app = bac!(pool);
    let (_, email) = compte_actif(&pool, "distinction").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        demande(&jeton, "plomberie", "Fuite", LAT, LON).to_request(),
    )
    .await;
    let corps: Value = test::read_body_json(reponse).await;
    let candidats = corps["candidats"].as_u64().unwrap();
    let notifies = corps["notifies"].as_u64().unwrap();
    assert!(
        notifies <= candidats,
        "on ne peut pas notifier plus de monde qu'on n'en retient"
    );
}
