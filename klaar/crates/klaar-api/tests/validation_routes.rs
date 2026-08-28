//! Story 4.6 — validation de fin de Mission (FR-021), contre un vrai PostgreSQL.
//!
//! **L'atomicité est la garantie centrale**, et elle ne se teste qu'ici : la
//! bascule de la Mission, l'entrée d'historique et la ligne de libération sont
//! une seule transaction. Un double en mémoire dirait ce qu'on lui a fait dire.

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
    let email = format!("val-{marqueur}-{id}@example.eu");
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

/// Pose un devis accepté sur la Mission, sans passer par les routes.
///
/// Écrit directement parce que le chemin nominal — émettre puis accepter —
/// demanderait deux sessions et trois requêtes pour arriver à l'état que ces
/// cas prennent comme point de départ. Les routes de devis ont leurs propres
/// tests.
async fn devis_accepte(pool: &PoolPg, mission_id: Uuid, provider_id: Uuid, htva: i64) -> Uuid {
    let tva = htva * 21 / 100;
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO devis (id, mission_id, provider_id, montant_htva_cents, taux_tva_bp,
                            tva_cents, total_ttc_cents, delai_minutes, statut, cree_le, expire_le)
         VALUES ($1, $2, $3, $4, 2100, $5, $6, 45, 'ACCEPTED', now(), now() + interval '1 hour')",
    )
    .bind(id)
    .bind(mission_id)
    .bind(provider_id)
    .bind(htva)
    .bind(tva)
    .bind(htva + tva)
    .execute(pool)
    .await
    .expect("devis accepté");
    id
}

/// Consigne la fin de l'intervention, comme la route de transition le ferait.
async fn terminee_il_y_a(pool: &PoolPg, mission_id: Uuid, provider_id: Uuid, heures: i64) {
    sqlx::query(
        "INSERT INTO mission_transition
             (mission_id, provider_id, statut, horodate_le, enregistre_le, position, hors_zone)
         VALUES ($1, $2, 'COMPLETED', now() - ($3 || ' hours')::interval,
                 now() - ($3 || ' hours')::interval, NULL, FALSE)",
    )
    .bind(mission_id)
    .bind(provider_id)
    .bind(heures.to_string())
    .execute(pool)
    .await
    .expect("fin consignée");
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

fn valider(jeton: &str, mission_id: Uuid) -> test::TestRequest {
    test::TestRequest::post()
        .uri(&format!("/api/v1/missions/{mission_id}/validate"))
        .insert_header(("Authorization", format!("Bearer {jeton}")))
}

// === @happy ===

#[actix_web::test]
async fn happy_le_demandeur_valide_et_la_repartition_est_ecrite() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "valide").await;
    let (mission_id, email) = mission(&pool, p.id, "COMPLETED").await;
    devis_accepte(&pool, mission_id, p.id, 18_000).await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(&app, valider(&jeton, mission_id).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::CREATED);
    let corps: Value = test::read_body_json(reponse).await;

    // Les nombres de l'exemple du PRD, jusqu'à la réponse HTTP.
    assert_eq!(corps["code"], "MISSION_VALIDATED");
    assert_eq!(corps["statut"], "AUTHORISED");
    assert_eq!(corps["total_ttc_cents"], 21_780);
    assert_eq!(corps["commission_ttc_cents"], 3_920);
    assert_eq!(corps["reversement_cents"], 17_860);
    assert_eq!(corps["origine"], "USER_VALIDATION");

    let statut: String = sqlx::query_scalar("SELECT statut FROM mission WHERE id = $1")
        .bind(mission_id)
        .fetch_one(&pool)
        .await
        .expect("Mission relue");
    assert_eq!(statut, "VALIDATED");
}

#[actix_web::test]
async fn happy_la_validation_est_consignee_dans_l_historique() {
    // FR-018 `@security` vaut aussi pour la transition que le demandeur
    // déclenche : une Mission avancée dont plus rien ne dit quand.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "historique").await;
    let (mission_id, email) = mission(&pool, p.id, "COMPLETED").await;
    devis_accepte(&pool, mission_id, p.id, 18_000).await;
    let jeton = jeton(&app, &email).await;

    test::call_service(&app, valider(&jeton, mission_id).to_request()).await;

    let consignees: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM mission_transition WHERE mission_id = $1 AND statut = 'VALIDATED'",
    )
    .bind(mission_id)
    .fetch_one(&pool)
    .await
    .expect("historique");
    assert_eq!(consignees, 1);
}

// === @negative ===

#[actix_web::test]
async fn negative_une_intervention_en_cours_ne_se_valide_pas() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "en-cours").await;
    let (mission_id, email) = mission(&pool, p.id, "ON_SITE").await;
    devis_accepte(&pool, mission_id, p.id, 18_000).await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(&app, valider(&jeton, mission_id).to_request()).await;

    assert_eq!(reponse.status(), StatusCode::CONFLICT);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "MISSION_NOT_COMPLETED");
}

#[actix_web::test]
async fn negative_sans_devis_accepte_il_n_y_a_rien_a_liberer() {
    // Pas d'accord de prix, pas de montant dû : prononcer une libération
    // reviendrait à décider seul de ce que quelqu'un doit payer.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "sans-accord").await;
    let (mission_id, email) = mission(&pool, p.id, "COMPLETED").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(&app, valider(&jeton, mission_id).to_request()).await;

    assert_eq!(reponse.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "QUOTE_NOT_ACCEPTED");

    let statut: String = sqlx::query_scalar("SELECT statut FROM mission WHERE id = $1")
        .bind(mission_id)
        .fetch_one(&pool)
        .await
        .expect("Mission relue");
    assert_eq!(statut, "COMPLETED", "rien ne doit avoir bougé");
}

// === @edge ===

#[actix_web::test]
async fn edge_valider_deux_fois_rend_le_code_du_prd() {
    // FR-021 `@negative` : 409 `ALREADY_RELEASED`.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "double-validation").await;
    let (mission_id, email) = mission(&pool, p.id, "COMPLETED").await;
    devis_accepte(&pool, mission_id, p.id, 18_000).await;
    let jeton = jeton(&app, &email).await;

    let premiere = test::call_service(&app, valider(&jeton, mission_id).to_request()).await;
    assert_eq!(premiere.status(), StatusCode::CREATED);

    let seconde = test::call_service(&app, valider(&jeton, mission_id).to_request()).await;
    assert_eq!(seconde.status(), StatusCode::CONFLICT);
    let corps: Value = test::read_body_json(seconde).await;
    assert_eq!(corps["code"], "ALREADY_RELEASED");

    let liberations: i64 =
        sqlx::query_scalar("SELECT count(*) FROM liberation WHERE mission_id = $1")
            .bind(mission_id)
            .fetch_one(&pool)
            .await
            .expect("comptage");
    assert_eq!(liberations, 1, "on ne paie pas deux fois");
}

#[actix_web::test]
async fn edge_au_dela_de_cinq_cents_euros_la_liberation_attend_un_second_regard() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "gros-montant").await;
    let (mission_id, email) = mission(&pool, p.id, "COMPLETED").await;
    // 600 € HTVA font 726 € TTC, au-dessus du seuil.
    devis_accepte(&pool, mission_id, p.id, 60_000).await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(&app, valider(&jeton, mission_id).to_request()).await;

    assert_eq!(reponse.status(), StatusCode::CREATED);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["statut"], "PENDING_OPS");
}

#[actix_web::test]
async fn edge_le_balayage_valide_apres_soixante_douze_heures() {
    use klaar_application::ports::horloge::HorlogeSysteme;
    use klaar_application::usecases::valider_mission::valider_les_echues;
    use klaar_sqlx_repos::{PgDevisRepository, PgLiberationRepository};

    let pool = pool().await;
    let (p, _) = prestataire(&pool, "balayage").await;
    let (mission_id, _) = mission(&pool, p.id, "COMPLETED").await;
    devis_accepte(&pool, mission_id, p.id, 18_000).await;
    terminee_il_y_a(&pool, mission_id, p.id, 73).await;

    let devis = PgDevisRepository::new(pool.clone());
    let liberations = PgLiberationRepository::new(pool.clone());
    // La base est partagée : plusieurs passages peuvent être nécessaires, et
    // l'assertion porte sur **notre** Mission, jamais sur le total.
    for _ in 0..10 {
        let bilan = valider_les_echues(&devis, &liberations, &HorlogeSysteme)
            .await
            .expect("balayage");
        let statut: String = sqlx::query_scalar("SELECT statut FROM mission WHERE id = $1")
            .bind(mission_id)
            .fetch_one(&pool)
            .await
            .expect("Mission relue");
        if statut == "VALIDATED" {
            break;
        }
        if bilan.validees == 0 && bilan.sans_accord == 0 {
            break;
        }
    }

    let (statut, origine): (String, String) = sqlx::query_as(
        "SELECT m.statut, l.origine FROM mission m
         JOIN liberation l ON l.mission_id = m.id WHERE m.id = $1",
    )
    .bind(mission_id)
    .fetch_one(&pool)
    .await
    .expect("Mission validée par le balayage");
    assert_eq!(statut, "VALIDATED");
    assert_eq!(origine, "AUTO_RELEASE_72H");
}

#[actix_web::test]
async fn edge_le_balayage_laisse_les_interventions_recentes() {
    use klaar_application::ports::horloge::HorlogeSysteme;
    use klaar_application::usecases::valider_mission::valider_les_echues;
    use klaar_sqlx_repos::{PgDevisRepository, PgLiberationRepository};

    let pool = pool().await;
    let (p, _) = prestataire(&pool, "recente").await;
    let (mission_id, _) = mission(&pool, p.id, "COMPLETED").await;
    devis_accepte(&pool, mission_id, p.id, 18_000).await;
    // Terminée il y a une heure : le demandeur a encore le temps de regarder.
    terminee_il_y_a(&pool, mission_id, p.id, 1).await;

    valider_les_echues(
        &PgDevisRepository::new(pool.clone()),
        &PgLiberationRepository::new(pool.clone()),
        &HorlogeSysteme,
    )
    .await
    .expect("balayage");

    let statut: String = sqlx::query_scalar("SELECT statut FROM mission WHERE id = $1")
        .bind(mission_id)
        .fetch_one(&pool)
        .await
        .expect("Mission relue");
    assert_eq!(statut, "COMPLETED");
}

// === @security ===

#[actix_web::test]
async fn security_le_prestataire_ne_valide_pas_son_propre_travail() {
    // Sinon il signerait la réception de ce qu'il vient de faire, et l'accord
    // du demandeur ne vaudrait plus rien.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "auto-validation").await;
    let (mission_id, _) = mission(&pool, p.id, "COMPLETED").await;
    devis_accepte(&pool, mission_id, p.id, 18_000).await;
    let jeton_p = jeton(&app, &email_p).await;

    let reponse = test::call_service(&app, valider(&jeton_p, mission_id).to_request()).await;

    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
    let liberations: i64 =
        sqlx::query_scalar("SELECT count(*) FROM liberation WHERE mission_id = $1")
            .bind(mission_id)
            .fetch_one(&pool)
            .await
            .expect("comptage");
    assert_eq!(liberations, 0);
}

#[actix_web::test]
async fn security_un_tiers_ne_valide_pas_l_intervention_d_autrui() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "tiers-validation").await;
    let (mission_id, _) = mission(&pool, p.id, "COMPLETED").await;
    devis_accepte(&pool, mission_id, p.id, 18_000).await;
    let (_, email_tiers) = compte_actif(&pool, "curieux").await;
    let jeton_tiers = jeton(&app, &email_tiers).await;

    let reponse = test::call_service(&app, valider(&jeton_tiers, mission_id).to_request()).await;

    // 404 et non 403 : la même précédence anti-énumération que partout.
    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "MISSION_NOT_FOUND");
}

#[actix_web::test]
async fn security_les_montants_de_la_liberation_sont_figes() {
    // Un audit de versement ne vaut que si ce qu'il lit est ce qui a été
    // décidé. Le déclencheur de V22 le grave.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "fige").await;
    let (mission_id, email) = mission(&pool, p.id, "COMPLETED").await;
    devis_accepte(&pool, mission_id, p.id, 18_000).await;
    let jeton = jeton(&app, &email).await;
    test::call_service(&app, valider(&jeton, mission_id).to_request()).await;

    let refus = sqlx::query("UPDATE liberation SET reversement_cents = 1 WHERE mission_id = $1")
        .bind(mission_id)
        .execute(&pool)
        .await
        .expect_err("les montants doivent être figés");
    assert!(
        refus.to_string().contains("ne change plus de montant"),
        "déclencheur attendu, obtenu : {refus}"
    );
}

#[actix_web::test]
async fn security_la_somme_des_parts_fait_le_total_jusque_dans_la_base() {
    // L'invariant comptable est gravé par une contrainte : une erreur
    // d'arrondi introduite un jour échouera ici plutôt que de se retrouver dans
    // une comptabilité.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "somme").await;
    let (mission_id, email) = mission(&pool, p.id, "COMPLETED").await;
    devis_accepte(&pool, mission_id, p.id, 18_000).await;
    let jeton = jeton(&app, &email).await;
    test::call_service(&app, valider(&jeton, mission_id).to_request()).await;

    let (total, commission, reversement): (i64, i64, i64) = sqlx::query_as(
        "SELECT total_ttc_cents, commission_ttc_cents, reversement_cents
         FROM liberation WHERE mission_id = $1",
    )
    .bind(mission_id)
    .fetch_one(&pool)
    .await
    .expect("libération relue");
    assert_eq!(commission + reversement, total);
}
