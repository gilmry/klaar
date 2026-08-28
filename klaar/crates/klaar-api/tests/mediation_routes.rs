//! Story 7.4 — médiation d'un litige (FR-036), contre un vrai PostgreSQL.
//!
//! **Ce qui ne se teste qu'ici :** que deux médiateurs sur le même dossier ne
//! produisent qu'une décision, que la base refuse de retrancher un litige clos,
//! et que la file remonte le plus ancien d'abord.

use actix_web::{http::StatusCode, test};
use chrono::{Duration, Utc};
use klaar_api::{app_de_test, etat_de_test};
use klaar_identity::{
    calculer_totp, CompteOps, EmpreinteMotDePasse, MotDePasse, ParametresArgon2, TOTP_PAS_SECONDES,
};
use klaar_shared_kernel::Email;
use klaar_sqlx_repos::{creer_pool, PgOpsRepository, PoolPg};
use serde_json::Value;
use uuid::Uuid;

use klaar_application::ports::ops_repository::OpsRepository;

const MDP: &str = "Ops@2026Securise";

async fn pool() -> PoolPg {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI");
    creer_pool(&url).await.expect("connexion PostgreSQL")
}

async fn ops(pool: &PoolPg, role: &str, marqueur: &str) -> (CompteOps, Vec<u8>) {
    let email = Email::parse(&format!("med-{marqueur}-{}@klaar.test", Uuid::new_v4())).unwrap();
    let empreinte =
        EmpreinteMotDePasse::calculer(&MotDePasse::parse(MDP).unwrap(), ParametresArgon2::tests())
            .unwrap();
    let mut compte = CompteOps::creer(email, empreinte, role, Utc::now()).expect("rôle connu");
    let secret = vec![7u8; 20];
    compte.secret_totp = Some(secret.clone());
    assert!(PgOpsRepository::new(pool.clone())
        .creer(&compte)
        .await
        .expect("création"));
    (compte, secret)
}

fn code(secret: &[u8]) -> String {
    calculer_totp(secret, Utc::now().timestamp().div_euclid(TOTP_PAS_SECONDES))
}

macro_rules! bac {
    ($pool:expr) => {
        test::init_service(app_de_test(etat_de_test($pool.clone(), None))).await
    };
}

async fn porteur<S>(app: &S, compte: &CompteOps, secret: &[u8]) -> String
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
            .uri("/api/v1/ops/login")
            .set_json(serde_json::json!({
                "email": compte.email.as_str(), "mot_de_passe": MDP, "code": code(secret)
            }))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::OK, "connexion d'exploitation");
    let corps: Value = test::read_body_json(reponse).await;
    format!("Bearer {}", corps["jeton"].as_str().expect("jeton"))
}

/// Un litige ouvert, avec son devis accepté. Rend (litige_id, total TTC).
///
/// Écrit directement plutôt que par les routes : le chemin nominal demande une
/// intervention terminée, deux comptes et six requêtes pour arriver au point de
/// départ de ces cas. Les routes d'ouverture ont leurs propres tests.
async fn litige_ouvert(pool: &PoolPg, marqueur: &str, jours: i64, htva: i64) -> (Uuid, i64) {
    let demandeur = Uuid::new_v4();
    let empreinte =
        EmpreinteMotDePasse::calculer(&MotDePasse::parse(MDP).unwrap(), ParametresArgon2::tests())
            .unwrap();
    sqlx::query(
        "INSERT INTO utilisateur (id, email, empreinte_mot_de_passe, statut, locale, cree_le)
         VALUES ($1, $2, $3, 'ACTIVE', 'fr', now())",
    )
    .bind(demandeur)
    .bind(format!("med-{marqueur}-{demandeur}@example.eu"))
    .bind(empreinte.as_str())
    .execute(pool)
    .await
    .expect("demandeur");

    let provider = Uuid::new_v4();
    let corps = (Uuid::new_v4().as_u128() as u64) % 20_000_000;
    let bce = format!("{corps:08}{:02}", 97 - (corps % 97));
    let utilisateur_p = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO utilisateur (id, email, empreinte_mot_de_passe, statut, locale, cree_le)
         VALUES ($1, $2, $3, 'ACTIVE', 'fr', now())",
    )
    .bind(utilisateur_p)
    .bind(format!("med-p-{marqueur}-{utilisateur_p}@example.eu"))
    .bind(empreinte.as_str())
    .execute(pool)
    .await
    .expect("compte prestataire");
    sqlx::query(
        "INSERT INTO provider
             (id, utilisateur_id, numero_bce, raison_sociale, base,
              statut, origine_kyc, kyc_verifie_le, disponible, cree_le)
         VALUES ($1, $2, $3, 'Médiation SPRL',
                 ST_SetSRID(ST_MakePoint(4.3525, 50.8467), 4326)::geography,
                 'ACTIVE', 'DEMONSTRATION', now(), TRUE, now())",
    )
    .bind(provider)
    .bind(utilisateur_p)
    .bind(&bce)
    .execute(pool)
    .await
    .expect("prestataire");

    let demande = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO demande
             (id, demandeur_id, secteur_code, description, position, urgence, statut,
              rayon_metres, elargissements, diffuse_depuis, cree_le)
         VALUES ($1, $2, 'plomberie', 'Fuite',
                 ST_SetSRID(ST_MakePoint(4.3525, 50.8467), 4326)::geography,
                 'HIGH', 'MATCHED', 5000, 0, now(), now())",
    )
    .bind(demande)
    .bind(demandeur)
    .execute(pool)
    .await
    .expect("demande");

    let mission = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO mission (id, demande_id, provider_id, statut, cree_le)
         VALUES ($1, $2, $3, 'COMPLETED', now())",
    )
    .bind(mission)
    .bind(demande)
    .bind(provider)
    .execute(pool)
    .await
    .expect("mission");

    let tva = htva * 21 / 100;
    let total = htva + tva;
    sqlx::query(
        "INSERT INTO devis (id, mission_id, provider_id, montant_htva_cents, taux_tva_bp,
                            tva_cents, total_ttc_cents, delai_minutes, statut, cree_le, expire_le)
         VALUES ($1, $2, $3, $4, 2100, $5, $6, 45, 'ACCEPTED', now(), now() + interval '1 hour')",
    )
    .bind(Uuid::new_v4())
    .bind(mission)
    .bind(provider)
    .bind(htva)
    .bind(tva)
    .bind(total)
    .execute(pool)
    .await
    .expect("devis accepté");

    let litige = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO litige (id, mission_id, auteur_id, partie, motif, description,
                             statut, ouvert_le)
         VALUES ($1, $2, $3, 'USER', 'NOT_DONE',
                 'Le travail annoncé n''a pas été réalisé sur place.', 'OPENED', $4)",
    )
    .bind(litige)
    .bind(mission)
    .bind(demandeur)
    .bind(Utc::now() - Duration::days(jours))
    .execute(pool)
    .await
    .expect("litige");

    (litige, total)
}

fn trancher(entete: &str, litige: Uuid, corps: Value) -> test::TestRequest {
    test::TestRequest::post()
        .uri(&format!("/api/v1/ops/disputes/{litige}/resolve"))
        .insert_header(("Authorization", entete.to_string()))
        .set_json(corps)
}

#[actix_web::test]
async fn happy_le_mediateur_tranche_partiellement_et_la_repartition_est_exacte() {
    let pool = pool().await;
    let app = bac!(pool);
    let (mediateur, secret) = ops(&pool, "MEDIATOR", "partiel").await;
    let entete = porteur(&app, &mediateur, &secret).await;
    // 18 000 HTVA à 21 % → 21 780 TTC, l'exemple du PRD.
    let (litige, total) = litige_ouvert(&pool, "partiel", 3, 18_000).await;
    assert_eq!(total, 21_780);

    let corps: Value = test::call_and_read_body_json(
        &app,
        trancher(
            &entete,
            litige,
            serde_json::json!({ "decision": "PARTIAL_REFUND", "part_bp": 3000 }),
        )
        .to_request(),
    )
    .await;

    // FR-036 `@happy` : « PARTIAL_REFUND 30 % ».
    assert_eq!(corps["remboursement_cents"], 6_534);
    assert_eq!(corps["reversement_cents"], 15_246);
    // Un partiel reste une décision en faveur du demandeur : c'est ce statut que
    // les comptages de sanctions (FR-035) doivent voir.
    assert_eq!(corps["statut"], "RESOLVED_USER_FAVOR");
    // **Rien n'a bougé sur l'argent, et l'API le dit.** Annoncer un
    // remboursement qui n'aura pas lieu serait pire que de ne rien annoncer.
    assert_eq!(corps["execute"], false);

    let (statut, rembourse, par): (String, i64, Option<Uuid>) =
        sqlx::query_as("SELECT statut, remboursement_cents, tranche_par FROM litige WHERE id = $1")
            .bind(litige)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(statut, "RESOLVED_USER_FAVOR");
    assert_eq!(rembourse, 6_534);
    assert_eq!(par, Some(mediateur.id), "la décision porte son auteur");
}

#[actix_web::test]
async fn security_deux_mediateurs_sur_le_meme_dossier_ne_produisent_qu_une_decision() {
    let pool = pool().await;
    let app = bac!(pool);
    let (a, secret_a) = ops(&pool, "MEDIATOR", "concurrent-a").await;
    let (b, secret_b) = ops(&pool, "MEDIATOR", "concurrent-b").await;
    let entete_a = porteur(&app, &a, &secret_a).await;
    let entete_b = porteur(&app, &b, &secret_b).await;
    let (litige, _) = litige_ouvert(&pool, "concurrent", 2, 10_000).await;

    let premiere = test::call_service(
        &app,
        trancher(
            &entete_a,
            litige,
            serde_json::json!({ "decision": "USER_FAVOR" }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(premiere.status(), StatusCode::OK);

    // Le second médiateur arrive après. Sans le compare-and-swap, un second
    // remboursement partirait sans que personne ne s'en aperçoive.
    let seconde = test::call_service(
        &app,
        trancher(
            &entete_b,
            litige,
            serde_json::json!({ "decision": "PROVIDER_FAVOR" }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(seconde.status(), StatusCode::CONFLICT);

    let statut: String = sqlx::query_scalar("SELECT statut FROM litige WHERE id = $1")
        .bind(litige)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(statut, "RESOLVED_USER_FAVOR", "la première décision tient");
}

#[actix_web::test]
async fn security_la_base_refuse_de_retrancher_un_litige_clos() {
    let pool = pool().await;
    let app = bac!(pool);
    let (mediateur, secret) = ops(&pool, "MEDIATOR", "immuable").await;
    let entete = porteur(&app, &mediateur, &secret).await;
    let (litige, _) = litige_ouvert(&pool, "immuable", 1, 10_000).await;

    test::call_service(
        &app,
        trancher(
            &entete,
            litige,
            serde_json::json!({ "decision": "NO_FAULT" }),
        )
        .to_request(),
    )
    .await;

    // **Même en écrivant directement.** Le service refuse déjà, mais une
    // garantie qui ne tient que dans le code s'évapore au premier script de
    // maintenance : c'est le déclencheur de V31 qui doit refuser.
    let ecrasement = sqlx::query(
        "UPDATE litige SET statut = 'RESOLVED_USER_FAVOR', remboursement_cents = 999
          WHERE id = $1",
    )
    .bind(litige)
    .execute(&pool)
    .await;
    assert!(
        ecrasement.is_err(),
        "un litige tranché ne se retranche pas, même en SQL direct"
    );
}

#[actix_web::test]
async fn happy_la_file_remonte_le_plus_ancien_d_abord_et_signale_l_escalade() {
    let pool = pool().await;
    let app = bac!(pool);
    let (mediateur, secret) = ops(&pool, "MEDIATOR", "file").await;
    let entete = porteur(&app, &mediateur, &secret).await;
    let (vieux, _) = litige_ouvert(&pool, "vieux", 31, 10_000).await;
    let (recent, _) = litige_ouvert(&pool, "recent", 1, 10_000).await;

    let corps: Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/ops/disputes")
            .insert_header(("Authorization", entete))
            .to_request(),
    )
    .await;

    let dossiers = corps["dossiers"].as_array().expect("liste");
    let position = |id: Uuid| {
        dossiers
            .iter()
            .position(|d| d["id"] == id.to_string())
            .unwrap_or(usize::MAX)
    };
    // Assertion relative, non absolue : la base est partagée avec d'autres
    // tests, et affirmer un rang exact dépendrait de ce qu'ils écrivent.
    assert!(
        position(vieux) < position(recent),
        "le dossier le plus ancien doit remonter en premier"
    );

    let le_vieux = dossiers
        .iter()
        .find(|d| d["id"] == vieux.to_string())
        .unwrap();
    // FR-036 `@edge` : au-delà de trente jours, l'alerte est portée par le
    // service et non recalculée par l'écran.
    assert_eq!(le_vieux["a_escalader"], true);
    assert!(le_vieux["age_jours"].as_i64().unwrap() >= 31);
    assert_eq!(le_vieux["total_ttc_cents"], 12_100);
}

#[actix_web::test]
async fn security_un_role_sans_droit_ne_voit_pas_la_file() {
    let pool = pool().await;
    let app = bac!(pool);
    // Le lecteur lit le journal, pas les dossiers de litige : ce sont deux
    // droits distincts, et le premier ne donne pas le second.
    let (lecteur, secret) = ops(&pool, "READER", "sans-droit").await;
    let entete = porteur(&app, &lecteur, &secret).await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/ops/disputes")
            .insert_header(("Authorization", entete))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::FORBIDDEN);

    // Le refus est journalisé, comme tout refus d'exploitation.
    let refus: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM journal_ops WHERE ops_id = $1 AND geste = 'DISPUTE_RESOLVE_DENIED'",
    )
    .bind(lecteur.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(refus, 1);
}

#[actix_web::test]
async fn negative_une_part_sur_une_decision_qui_n_en_prend_pas_est_refusee() {
    let pool = pool().await;
    let app = bac!(pool);
    let (mediateur, secret) = ops(&pool, "MEDIATOR", "part-parasite").await;
    let entete = porteur(&app, &mediateur, &secret).await;
    let (litige, _) = litige_ouvert(&pool, "part-parasite", 1, 10_000).await;

    // Un taux passé avec « tout au demandeur » : l'ignorer laisserait croire
    // qu'il a été appliqué, et quelqu'un compterait sur un partiel qui n'a pas
    // eu lieu.
    let reponse = test::call_service(
        &app,
        trancher(
            &entete,
            litige,
            serde_json::json!({ "decision": "USER_FAVOR", "part_bp": 3000 }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // Et un partiel sans taux, symétriquement.
    let reponse = test::call_service(
        &app,
        trancher(
            &entete,
            litige,
            serde_json::json!({ "decision": "PARTIAL_REFUND" }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let statut: String = sqlx::query_scalar("SELECT statut FROM litige WHERE id = $1")
        .bind(litige)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(statut, "OPENED", "aucune décision ne doit avoir été prise");
}

#[actix_web::test]
async fn negative_un_litige_inconnu_donne_404_apres_verification_du_droit() {
    let pool = pool().await;
    let app = bac!(pool);
    let (mediateur, secret) = ops(&pool, "MEDIATOR", "inconnu").await;
    let entete = porteur(&app, &mediateur, &secret).await;

    let reponse = test::call_service(
        &app,
        trancher(
            &entete,
            Uuid::new_v4(),
            serde_json::json!({ "decision": "NO_FAULT" }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn edge_un_litige_sans_devis_accepte_se_tranche_sur_zero() {
    let pool = pool().await;
    let app = bac!(pool);
    let (mediateur, secret) = ops(&pool, "MEDIATOR", "sans-devis").await;
    let entete = porteur(&app, &mediateur, &secret).await;
    let (litige, _) = litige_ouvert(&pool, "sans-devis", 1, 10_000).await;
    // Le devis est retiré : un litige peut naître d'un travail jamais commencé,
    // donc d'une intervention sans accord de prix.
    sqlx::query(
        "DELETE FROM devis WHERE mission_id = (SELECT mission_id FROM litige WHERE id = $1)",
    )
    .bind(litige)
    .execute(&pool)
    .await
    .unwrap();

    let corps: Value = test::call_and_read_body_json(
        &app,
        trancher(
            &entete,
            litige,
            serde_json::json!({ "decision": "USER_FAVOR" }),
        )
        .to_request(),
    )
    .await;

    // Zéro, et non une erreur : il n'y a rien à rendre, mais le litige doit
    // pouvoir être clos. Le laisser ouvert faute de montant l'enverrait à
    // l'escalade des trente jours pour rien.
    assert_eq!(corps["remboursement_cents"], 0);
    assert_eq!(corps["reversement_cents"], 0);
    assert_eq!(corps["statut"], "RESOLVED_USER_FAVOR");
}
