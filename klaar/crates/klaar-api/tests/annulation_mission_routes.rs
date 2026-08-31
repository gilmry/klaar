//! Story 4.7 — annulation d'une Mission en cours (FR-022), contre PostgreSQL.
//!
//! **Ce qui ne se teste qu'ici :** l'atomicité de la bascule et de la ligne
//! d'annulation, le compteur de désistements sur une fenêtre glissante, et la
//! suspension automatique qu'il déclenche.

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
    let email = format!("ann-{marqueur}-{id}@example.eu");
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

/// Statut de la fiche prestataire, pour observer la suspension automatique.
async fn statut_provider(pool: &PoolPg, provider_id: Uuid) -> String {
    sqlx::query_scalar("SELECT statut FROM provider WHERE id = $1")
        .bind(provider_id)
        .fetch_one(pool)
        .await
        .expect("prestataire relu")
}

// === @happy ===

#[actix_web::test]
async fn happy_le_demandeur_annule_avant_le_depart_et_est_rembourse() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "annule-tot").await;
    let (mission_id, email) = mission(&pool, p.id, "ACCEPTED").await;
    devis_accepte(&pool, mission_id, p.id, 18_000).await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        annuler(
            &jeton,
            mission_id,
            serde_json::json!({ "motif": "NO_LONGER_NEEDED" }),
        )
        .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "MISSION_CANCELLED");
    assert_eq!(corps["auteur"], "CANCELLED_USER");
    assert_eq!(corps["forfait_deplacement_cents"], 0);
    assert_eq!(corps["remboursement_cents"], 21_780);
    assert_eq!(corps["prestataire_suspendu"], false);

    let statut: String = sqlx::query_scalar("SELECT statut FROM mission WHERE id = $1")
        .bind(mission_id)
        .fetch_one(&pool)
        .await
        .expect("Mission relue");
    assert_eq!(statut, "CANCELLED");
}

#[actix_web::test]
async fn happy_le_prestataire_annule_et_son_desistement_est_compte() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "desiste").await;
    let (mission_id, _) = mission(&pool, p.id, "ACCEPTED").await;
    devis_accepte(&pool, mission_id, p.id, 18_000).await;
    let jeton_p = jeton(&app, &email_p).await;

    let reponse = test::call_service(
        &app,
        annuler(
            &jeton_p,
            mission_id,
            serde_json::json!({ "motif": "UNAVAILABLE" }),
        )
        .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["auteur"], "CANCELLED_PROVIDER");
    // Le demandeur récupère tout : ce n'est pas lui qui a renoncé.
    assert_eq!(corps["remboursement_cents"], 21_780);

    let penalise: bool = sqlx::query_scalar(
        "SELECT penalise_le_prestataire FROM annulation_mission WHERE mission_id = $1",
    )
    .bind(mission_id)
    .fetch_one(&pool)
    .await
    .expect("annulation relue");
    assert!(penalise);
}

#[actix_web::test]
async fn happy_l_annulation_est_consignee_dans_l_historique() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "historique-annul").await;
    let (mission_id, email) = mission(&pool, p.id, "ACCEPTED").await;
    let jeton = jeton(&app, &email).await;

    test::call_service(
        &app,
        annuler(&jeton, mission_id, serde_json::json!({})).to_request(),
    )
    .await;

    let consignees: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM mission_transition WHERE mission_id = $1 AND statut = 'CANCELLED'",
    )
    .bind(mission_id)
    .fetch_one(&pool)
    .await
    .expect("historique");
    assert_eq!(consignees, 1);
}

// === @negative ===

#[actix_web::test]
async fn negative_une_intervention_faite_ne_s_annule_pas() {
    // FR-022 `@negative` : 409, et renvoi vers le litige.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "faite").await;
    let (mission_id, email) = mission(&pool, p.id, "COMPLETED").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        annuler(&jeton, mission_id, serde_json::json!({})).to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::CONFLICT);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "MISSION_COMPLETED");
}

#[actix_web::test]
async fn negative_un_motif_hors_vocabulaire_est_refuse() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "motif-annul").await;
    let (mission_id, email) = mission(&pool, p.id, "ACCEPTED").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        annuler(
            &jeton,
            mission_id,
            serde_json::json!({ "motif": "il m'a mal parlé" }),
        )
        .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::BAD_REQUEST);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "REASON_UNKNOWN");
}

// === @edge ===

#[actix_web::test]
async fn edge_sur_place_le_forfait_de_deplacement_est_retenu() {
    // FR-022 `@negative` : trente euros pour le déplacement, le reste rendu.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "sur-place").await;
    let (mission_id, email) = mission(&pool, p.id, "ON_SITE").await;
    devis_accepte(&pool, mission_id, p.id, 18_000).await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        annuler(&jeton, mission_id, serde_json::json!({})).to_request(),
    )
    .await;

    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["forfait_deplacement_cents"], 3_000);
    assert_eq!(corps["remboursement_cents"], 18_780);
}

#[actix_web::test]
async fn edge_sans_devis_accepte_l_annulation_ne_coute_rien() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "sans-devis-annul").await;
    let (mission_id, email) = mission(&pool, p.id, "ON_SITE").await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        annuler(&jeton, mission_id, serde_json::json!({})).to_request(),
    )
    .await;

    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["forfait_deplacement_cents"], 0);
    assert_eq!(corps["remboursement_cents"], 0);
}

#[actix_web::test]
async fn edge_annuler_deux_fois_ne_passe_qu_une_fois() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "double-annul").await;
    let (mission_id, email) = mission(&pool, p.id, "ACCEPTED").await;
    let jeton = jeton(&app, &email).await;

    let premiere = test::call_service(
        &app,
        annuler(&jeton, mission_id, serde_json::json!({})).to_request(),
    )
    .await;
    assert_eq!(premiere.status(), StatusCode::OK);

    let seconde = test::call_service(
        &app,
        annuler(&jeton, mission_id, serde_json::json!({})).to_request(),
    )
    .await;
    assert_eq!(seconde.status(), StatusCode::CONFLICT);

    let lignes: i64 =
        sqlx::query_scalar("SELECT count(*) FROM annulation_mission WHERE mission_id = $1")
            .bind(mission_id)
            .fetch_one(&pool)
            .await
            .expect("comptage");
    assert_eq!(lignes, 1, "on ne rembourse pas deux fois");
}

#[actix_web::test]
async fn edge_le_troisieme_desistement_suspend_le_prestataire() {
    // FR-022 `@edge` : trois désistements en trente jours suspendent.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "recidive").await;
    let jeton_p = jeton(&app, &email_p).await;

    for tour in 1..=3 {
        // Une Mission par tour : « une Mission à la fois » interdit d'en ouvrir
        // plusieurs, et l'annulation en libère une à chaque fois.
        let (mission_id, _) = mission(&pool, p.id, "ACCEPTED").await;
        let reponse = test::call_service(
            &app,
            annuler(
                &jeton_p,
                mission_id,
                serde_json::json!({ "motif": "UNAVAILABLE" }),
            )
            .to_request(),
        )
        .await;
        assert_eq!(reponse.status(), StatusCode::OK, "désistement {tour}");
        let corps: Value = test::read_body_json(reponse).await;
        assert_eq!(
            corps["prestataire_suspendu"],
            tour == 3,
            "la suspension tombe au troisième, pas avant"
        );
    }

    assert_eq!(statut_provider(&pool, p.id).await, "SUSPENDED");
}

#[actix_web::test]
async fn edge_deux_desistements_ne_suspendent_pas() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, email_p) = prestataire(&pool, "deux-fois").await;
    let jeton_p = jeton(&app, &email_p).await;

    for _ in 0..2 {
        let (mission_id, _) = mission(&pool, p.id, "ACCEPTED").await;
        test::call_service(
            &app,
            annuler(&jeton_p, mission_id, serde_json::json!({})).to_request(),
        )
        .await;
    }

    assert_eq!(statut_provider(&pool, p.id).await, "ACTIVE");
}

#[actix_web::test]
async fn edge_les_annulations_du_demandeur_ne_penalisent_pas_le_prestataire() {
    // Le compteur ne doit compter que les désistements : sinon un prestataire
    // serait suspendu parce que trois clients ont changé d'avis.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "innocent").await;

    for _ in 0..3 {
        let (mission_id, email) = mission(&pool, p.id, "ACCEPTED").await;
        let jeton_d = jeton(&app, &email).await;
        test::call_service(
            &app,
            annuler(&jeton_d, mission_id, serde_json::json!({})).to_request(),
        )
        .await;
    }

    assert_eq!(statut_provider(&pool, p.id).await, "ACTIVE");
}

// === @security ===

#[actix_web::test]
async fn security_un_tiers_n_annule_pas_l_intervention_d_autrui() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "tiers-annul").await;
    let (mission_id, _) = mission(&pool, p.id, "ACCEPTED").await;
    let (_, email_tiers) = compte_actif(&pool, "curieux").await;
    let jeton_tiers = jeton(&app, &email_tiers).await;

    let reponse = test::call_service(
        &app,
        annuler(&jeton_tiers, mission_id, serde_json::json!({})).to_request(),
    )
    .await;

    // 404 et non 403 : la même précédence anti-énumération que partout.
    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
    let statut: String = sqlx::query_scalar("SELECT statut FROM mission WHERE id = $1")
        .bind(mission_id)
        .fetch_one(&pool)
        .await
        .expect("Mission relue");
    assert_eq!(statut, "ACCEPTED", "rien ne doit avoir bougé");
}

#[actix_web::test]
async fn security_un_autre_prestataire_n_annule_pas_la_mission() {
    let pool = pool().await;
    let app = bac!(pool);
    let (titulaire, _) = prestataire(&pool, "titulaire-annul").await;
    let (_, email_autre) = prestataire(&pool, "autre-annul").await;
    let jeton_autre = jeton(&app, &email_autre).await;
    let (mission_id, _) = mission(&pool, titulaire.id, "ACCEPTED").await;

    let reponse = test::call_service(
        &app,
        annuler(&jeton_autre, mission_id, serde_json::json!({})).to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn security_l_annulation_consignee_ne_se_reecrit_pas() {
    // Une annulation est un fait daté, pas un état qu'on ajuste : le
    // déclencheur de V23 le grave.
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "fige-annul").await;
    let (mission_id, email) = mission(&pool, p.id, "ON_SITE").await;
    devis_accepte(&pool, mission_id, p.id, 18_000).await;
    let jeton = jeton(&app, &email).await;
    test::call_service(
        &app,
        annuler(&jeton, mission_id, serde_json::json!({})).to_request(),
    )
    .await;

    let refus =
        sqlx::query("UPDATE annulation_mission SET remboursement_cents = 0 WHERE mission_id = $1")
            .bind(mission_id)
            .execute(&pool)
            .await
            .expect_err("l'annulation doit être figée");
    assert!(
        refus.to_string().contains("append-only"),
        "déclencheur attendu, obtenu : {refus}"
    );
}

#[actix_web::test]
async fn security_le_forfait_et_le_remboursement_font_l_engagement() {
    let pool = pool().await;
    let app = bac!(pool);
    let (p, _) = prestataire(&pool, "somme-annul").await;
    let (mission_id, email) = mission(&pool, p.id, "ON_SITE").await;
    devis_accepte(&pool, mission_id, p.id, 18_000).await;
    let jeton = jeton(&app, &email).await;

    let reponse = test::call_service(
        &app,
        annuler(&jeton, mission_id, serde_json::json!({})).to_request(),
    )
    .await;
    let corps: Value = test::read_body_json(reponse).await;

    let forfait = corps["forfait_deplacement_cents"].as_i64().unwrap();
    let remboursement = corps["remboursement_cents"].as_i64().unwrap();
    assert_eq!(forfait + remboursement, 21_780);
}
