//! Story 4.8 — reprogrammation (FR-023), contre un vrai PostgreSQL.
//!
//! **Ce qui ne se teste qu'ici :** que l'index partiel de V28 laisse bien
//! naître une seconde Mission sur la même Demande une fois la première annulée,
//! et que le devis convenu soit recopié plutôt que déplacé.

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
    let email = format!("rep-{marqueur}-{id}@example.eu");
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

fn annuler(jeton: &str, mission_id: Uuid, corps: Value) -> test::TestRequest {
    test::TestRequest::post()
        .uri(&format!("/api/v1/missions/{mission_id}/cancel"))
        .insert_header(("Authorization", format!("Bearer {jeton}")))
        .set_json(corps)
}

fn reprogrammer(jeton: &str, mission_id: Uuid) -> test::TestRequest {
    test::TestRequest::post()
        .uri(&format!("/api/v1/missions/{mission_id}/reschedule"))
        .insert_header(("Authorization", format!("Bearer {jeton}")))
}

fn repondre(jeton: &str, mission_id: Uuid, accepte: bool) -> test::TestRequest {
    test::TestRequest::post()
        .uri(&format!("/api/v1/missions/{mission_id}/reschedule/answer"))
        .insert_header(("Authorization", format!("Bearer {jeton}")))
        .set_json(serde_json::json!({ "accepte": accepte }))
}

/// Une intervention annulée par le prestataire, devis accepté à l'appui.
///
/// C'est le seul point de départ d'une reprogrammation : un demandeur qui a
/// renoncé refait une Demande.
async fn annulee_par_le_prestataire(
    pool: &PoolPg,
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    marqueur: &str,
) -> (Uuid, String, String) {
    let (p, email_p) = prestataire(pool, marqueur).await;
    let (mission_id, email_d) = mission(pool, p.id, "ACCEPTED").await;
    devis_accepte(pool, mission_id, p.id, 18_000).await;
    let jeton_p = jeton(app, &email_p).await;
    let reponse = test::call_service(
        app,
        annuler(
            &jeton_p,
            mission_id,
            serde_json::json!({ "motif": "UNAVAILABLE" }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::OK, "annulation prestataire");
    (mission_id, email_d, email_p)
}

// === @happy ===

#[actix_web::test]
async fn happy_le_demandeur_propose_et_le_prestataire_accepte() {
    let pool = pool().await;
    let app = bac!(pool);
    let (mission_id, email_d, email_p) = annulee_par_le_prestataire(&pool, &app, "reprise").await;
    let jeton_d = jeton(&app, &email_d).await;
    let jeton_p = jeton(&app, &email_p).await;

    let proposition =
        test::call_service(&app, reprogrammer(&jeton_d, mission_id).to_request()).await;
    assert_eq!(proposition.status(), StatusCode::CREATED);
    let corps: Value = test::read_body_json(proposition).await;
    assert_eq!(corps["code"], "RESCHEDULE_PROPOSED");
    assert_eq!(corps["statut"], "PROPOSED");

    let reponse = test::call_service(&app, repondre(&jeton_p, mission_id, true).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "RESCHEDULE_ACCEPTED");
    let nouvelle: Uuid = corps["nouvelle_mission_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    // La nouvelle intervention est vivante, l'ancienne reste annulée : c'est
    // l'index partiel de V28 qui l'autorise.
    let statuts: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, statut FROM mission WHERE id = ANY($1) ORDER BY cree_le")
            .bind(vec![mission_id, nouvelle])
            .fetch_all(&pool)
            .await
            .expect("missions");
    for (id, statut) in statuts {
        if id == mission_id {
            assert_eq!(statut, "CANCELLED");
        } else {
            assert_eq!(statut, "ACCEPTED");
        }
    }
}

#[actix_web::test]
async fn happy_le_devis_convenu_est_recopie_a_l_identique() {
    // C'est ce qui distingue une reprogrammation d'une nouvelle Demande : le
    // prix a déjà été convenu, et il ne se renégocie pas au passage.
    let pool = pool().await;
    let app = bac!(pool);
    let (mission_id, email_d, email_p) = annulee_par_le_prestataire(&pool, &app, "devis").await;
    let jeton_d = jeton(&app, &email_d).await;
    let jeton_p = jeton(&app, &email_p).await;
    test::call_service(&app, reprogrammer(&jeton_d, mission_id).to_request()).await;
    let reponse = test::call_service(&app, repondre(&jeton_p, mission_id, true).to_request()).await;
    let corps: Value = test::read_body_json(reponse).await;
    let nouvelle: Uuid = corps["nouvelle_mission_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let (htva, ttc, statut): (i64, i64, String) = sqlx::query_as(
        "SELECT montant_htva_cents, total_ttc_cents, statut FROM devis WHERE mission_id = $1",
    )
    .bind(nouvelle)
    .fetch_one(&pool)
    .await
    .expect("devis repris");
    assert_eq!((htva, ttc), (18_000, 21_780));
    // Il naît accepté : les deux parties se sont déjà mises d'accord, et le
    // refaire valider serait leur demander deux fois la même chose.
    assert_eq!(statut, "ACCEPTED");

    // **Et l'ancien devis reste attaché à l'intervention annulée** : c'est lui
    // qui explique ce qui avait été convenu, et le déplacer réécrirait
    // l'histoire de l'annulation.
    let restant: i64 = sqlx::query_scalar("SELECT count(*) FROM devis WHERE mission_id = $1")
        .bind(mission_id)
        .fetch_one(&pool)
        .await
        .expect("comptage");
    assert_eq!(restant, 1);
}

// === @negative ===

#[actix_web::test]
async fn negative_un_prestataire_qui_decline_ferme_la_porte() {
    // FR-023 `@negative` : 409 `PROVIDER_DECLINED`.
    let pool = pool().await;
    let app = bac!(pool);
    let (mission_id, email_d, email_p) = annulee_par_le_prestataire(&pool, &app, "decline").await;
    let jeton_d = jeton(&app, &email_d).await;
    let jeton_p = jeton(&app, &email_p).await;
    test::call_service(&app, reprogrammer(&jeton_d, mission_id).to_request()).await;

    let refus = test::call_service(&app, repondre(&jeton_p, mission_id, false).to_request()).await;
    assert_eq!(refus.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(refus).await;
    assert_eq!(corps["code"], "RESCHEDULE_DECLINED");

    // Une nouvelle tentative dit le refus, et non « déjà proposée » : envoyer
    // le demandeur attendre une réponse déjà tombée serait cruel.
    let seconde = test::call_service(&app, reprogrammer(&jeton_d, mission_id).to_request()).await;
    assert_eq!(seconde.status(), StatusCode::CONFLICT);
    let corps: Value = test::read_body_json(seconde).await;
    assert_eq!(corps["code"], "PROVIDER_DECLINED");
}

#[actix_web::test]
async fn negative_une_annulation_du_demandeur_ne_se_reprogramme_pas() {
    // Un demandeur qui a renoncé refait une Demande : elle rediffusera, et il
    // trouvera peut-être mieux.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "annul-demandeur").await;
    let (mission_id, email_d) = mission(&pool, p.id, "ACCEPTED").await;
    devis_accepte(&pool, mission_id, p.id, 18_000).await;
    let jeton_d = jeton(&app, &email_d).await;
    test::call_service(
        &app,
        annuler(&jeton_d, mission_id, serde_json::json!({})).to_request(),
    )
    .await;

    let reponse = test::call_service(&app, reprogrammer(&jeton_d, mission_id).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "CANCELLED_BY_USER");
}

#[actix_web::test]
async fn negative_une_intervention_non_annulee_ne_se_reprogramme_pas() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "en-cours-rep").await;
    let (mission_id, email_d) = mission(&pool, p.id, "ON_SITE").await;
    let jeton_d = jeton(&app, &email_d).await;

    let reponse = test::call_service(&app, reprogrammer(&jeton_d, mission_id).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "MISSION_NOT_CANCELLED");
}

// === @edge ===

#[actix_web::test]
async fn edge_sans_devis_accepte_il_n_y_a_rien_a_reprendre() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "sans-devis-rep").await;
    let (mission_id, email_d) = mission(&pool, p.id, "ACCEPTED").await;
    let jeton_p = jeton(&app, &email_p).await;
    test::call_service(
        &app,
        annuler(&jeton_p, mission_id, serde_json::json!({})).to_request(),
    )
    .await;

    let jeton_d = jeton(&app, &email_d).await;
    let reponse = test::call_service(&app, reprogrammer(&jeton_d, mission_id).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "QUOTE_NOT_ACCEPTED");
}

#[actix_web::test]
async fn edge_repondre_deux_fois_ne_cree_qu_une_intervention() {
    let pool = pool().await;
    let app = bac!(pool);
    let (mission_id, email_d, email_p) =
        annulee_par_le_prestataire(&pool, &app, "double-rep").await;
    let jeton_d = jeton(&app, &email_d).await;
    let jeton_p = jeton(&app, &email_p).await;
    test::call_service(&app, reprogrammer(&jeton_d, mission_id).to_request()).await;

    let premiere =
        test::call_service(&app, repondre(&jeton_p, mission_id, true).to_request()).await;
    assert_eq!(premiere.status(), StatusCode::OK);
    let seconde = test::call_service(&app, repondre(&jeton_p, mission_id, true).to_request()).await;
    assert_eq!(seconde.status(), StatusCode::CONFLICT);

    let demande_id: Uuid = sqlx::query_scalar("SELECT demande_id FROM mission WHERE id = $1")
        .bind(mission_id)
        .fetch_one(&pool)
        .await
        .expect("Demande");
    let vivantes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM mission WHERE demande_id = $1 AND statut <> 'CANCELLED'",
    )
    .bind(demande_id)
    .fetch_one(&pool)
    .await
    .expect("comptage");
    assert_eq!(vivantes, 1, "une seule intervention vivante par Demande");
}

// === @security ===

#[actix_web::test]
async fn security_seul_le_demandeur_propose() {
    let pool = pool().await;
    let app = bac!(pool);
    let (mission_id, _, email_p) = annulee_par_le_prestataire(&pool, &app, "propose-tiers").await;
    let jeton_p = jeton(&app, &email_p).await;

    // Le prestataire ne se réattribue pas lui-même l'intervention qu'il vient
    // d'annuler.
    let reponse = test::call_service(&app, reprogrammer(&jeton_p, mission_id).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn security_seul_le_prestataire_concerne_repond() {
    let pool = pool().await;
    let app = bac!(pool);
    let (mission_id, email_d, _) = annulee_par_le_prestataire(&pool, &app, "repond-tiers").await;
    let jeton_d = jeton(&app, &email_d).await;
    test::call_service(&app, reprogrammer(&jeton_d, mission_id).to_request()).await;

    // Le demandeur ne s'accorde pas la reprise tout seul.
    let par_le_demandeur =
        test::call_service(&app, repondre(&jeton_d, mission_id, true).to_request()).await;
    assert_eq!(par_le_demandeur.status(), StatusCode::NOT_FOUND);

    let (_, email_autre) = prestataire(&pool, "autre-prestataire-rep").await;
    let jeton_autre = jeton(&app, &email_autre).await;
    let par_un_autre =
        test::call_service(&app, repondre(&jeton_autre, mission_id, true).to_request()).await;
    assert_eq!(par_un_autre.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn security_une_proposition_acceptee_porte_sa_nouvelle_intervention() {
    // La contrainte de V28 le grave : un « accepté » sans Mission laisserait le
    // demandeur devant une promesse que rien ne porte.
    let pool = pool().await;
    let app = bac!(pool);
    let (mission_id, email_d, email_p) = annulee_par_le_prestataire(&pool, &app, "coherence").await;
    let jeton_d = jeton(&app, &email_d).await;
    let jeton_p = jeton(&app, &email_p).await;
    test::call_service(&app, reprogrammer(&jeton_d, mission_id).to_request()).await;
    test::call_service(&app, repondre(&jeton_p, mission_id, true).to_request()).await;

    let refus =
        sqlx::query("UPDATE reprogrammation SET nouvelle_mission_id = NULL WHERE mission_id = $1")
            .bind(mission_id)
            .execute(&pool)
            .await
            .expect_err("la contrainte doit refuser un accepté sans Mission");
    assert!(
        refus
            .to_string()
            .contains("reprogrammation_mission_si_acceptee"),
        "contrainte attendue, obtenu : {refus}"
    );
}
