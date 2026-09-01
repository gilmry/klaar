//! Story 0.12 — endpoints d'abonnement push, montés en mémoire.
//!
//! `DATABASE_URL` est requise : le dépôt réel est utilisé plutôt qu'un double,
//! parce que ce qui peut casser ici (`ON CONFLICT`, unicité) n'existe que dans
//! PostgreSQL.

use std::sync::Arc;

use actix_web::{http::StatusCode, test, web};
use klaar_api::{app_de_test, etat_de_test, EtatApplication};
use klaar_push_adapter::{ClesVapid, WebPushSender};
use klaar_sqlx_repos::creer_pool;
use uuid::Uuid;

const UA_PUBLIC: &str =
    "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4";

async fn etat(avec_push: bool) -> web::Data<EtatApplication> {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI");
    let pool = creer_pool(&url).await.expect("connexion PostgreSQL");
    etat_de_test(
        pool,
        avec_push.then(|| {
            let (cles, _, _) = ClesVapid::generer("mailto:ops@klaar.be").unwrap();
            Arc::new(WebPushSender::new(cles))
        }),
    )
}

fn corps_abonnement(endpoint: &str, auth: &str) -> serde_json::Value {
    serde_json::json!({
        "endpoint": endpoint,
        "keys": { "p256dh": UA_PUBLIC, "auth": auth }
    })
}

fn endpoint_unique(marqueur: &str) -> String {
    format!("https://push.example.net/e/{marqueur}-{}", Uuid::new_v4())
}

#[actix_web::test]
async fn happy_enregistre_puis_supprime_un_abonnement() {
    let app = test::init_service(app_de_test(etat(true).await)).await;
    let endpoint = endpoint_unique("happy");

    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/push/abonnements")
            .set_json(corps_abonnement(&endpoint, "BTBZMqHH6r4Tts7J_aSIgg"))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::CREATED);

    let reponse = test::call_service(
        &app,
        test::TestRequest::delete()
            .uri("/api/v1/push/abonnements")
            .set_json(serde_json::json!({ "endpoint": endpoint }))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::NO_CONTENT);
}

#[actix_web::test]
async fn happy_expose_la_cle_publique_vapid() {
    let app = test::init_service(app_de_test(etat(true).await)).await;
    let corps: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/push/cle-publique")
            .to_request(),
    )
    .await;
    let cle = corps["cle"].as_str().unwrap();
    // Forme non compressée : 65 octets, soit 87 caractères en base64url.
    assert_eq!(cle.len(), 87);
    assert!(cle.starts_with('B'), "préfixe 0x04 attendu");
}

#[actix_web::test]
async fn negative_refuse_un_abonnement_dont_le_secret_est_trop_court() {
    let app = test::init_service(app_de_test(etat(true).await)).await;
    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/push/abonnements")
            .set_json(corps_abonnement(&endpoint_unique("court"), "AAAA"))
            .to_request(),
    )
    .await;
    // **422 et non 400.** Le corps est lisible et bien formé ; c'est son
    // contenu qui n'est pas un abonnement utilisable.
    assert_eq!(reponse.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[actix_web::test]
async fn negative_repond_503_quand_le_push_n_est_pas_configure() {
    // 503 et non 500 : le client doit pouvoir distinguer « pas activé ici » de
    // « en panne », pour masquer l'invitation au lieu d'afficher une erreur.
    let app = test::init_service(app_de_test(etat(false).await)).await;
    let reponse = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/push/cle-publique")
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[actix_web::test]
async fn edge_reenregistrer_le_meme_endpoint_reste_un_succes() {
    let app = test::init_service(app_de_test(etat(true).await)).await;
    let endpoint = endpoint_unique("rejeu");
    let corps = corps_abonnement(&endpoint, "BTBZMqHH6r4Tts7J_aSIgg");

    for _ in 0..2 {
        let reponse = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/push/abonnements")
                .set_json(corps.clone())
                .to_request(),
        )
        .await;
        assert_eq!(reponse.status(), StatusCode::CREATED);
    }

    test::call_service(
        &app,
        test::TestRequest::delete()
            .uri("/api/v1/push/abonnements")
            .set_json(serde_json::json!({ "endpoint": endpoint }))
            .to_request(),
    )
    .await;
}

#[actix_web::test]
async fn security_supprimer_ne_revele_pas_l_existence_d_un_abonnement() {
    // Répondre 404 sur un endpoint absent et 204 sur un endpoint présent
    // ferait de cette route un oracle : n'importe qui pourrait tester si une
    // URL de push donnée est enregistrée chez nous.
    let app = test::init_service(app_de_test(etat(true).await)).await;
    let reponse = test::call_service(
        &app,
        test::TestRequest::delete()
            .uri("/api/v1/push/abonnements")
            .set_json(serde_json::json!({ "endpoint": endpoint_unique("inconnu") }))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::NO_CONTENT);
}

#[actix_web::test]
async fn security_rejette_un_champ_inconnu_dans_la_charge() {
    // `deny_unknown_fields` : un champ non prévu est le signe d'un client
    // désynchronisé ou d'une tentative de contrebande, pas d'une extension
    // inoffensive.
    let app = test::init_service(app_de_test(etat(true).await)).await;
    let mut corps = corps_abonnement(&endpoint_unique("espion"), "BTBZMqHH6r4Tts7J_aSIgg");
    corps["espion"] = serde_json::json!("charge utile");

    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/push/abonnements")
            .set_json(corps)
            .to_request(),
    )
    .await;
    // 400 ici, et non 422 : un champ inconnu rend le corps illisible pour le
    // lecteur JSON, qui refuse avant que la moindre règle métier ne s'applique.
    assert_eq!(reponse.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn security_une_adresse_hors_bornes_est_refusee() {
    // **Le contrat annonce des bornes, le serveur doit les tenir.** Il ne les
    // tenait pas : une adresse de quatre mille caractères était acceptée, et la
    // suppression rendait 204 sur n'importe quelle chaîne. Une adresse de cette
    // taille n'a jamais été produite par un service de push ; elle finit
    // seulement en ligne de base de données.
    let app = test::init_service(app_de_test(etat(true).await)).await;

    for adresse in ["", "https://", &format!("https://x.example/{}", "a".repeat(4096))] {
        let reponse = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/push/abonnements")
                .set_json(corps_abonnement(adresse, "AAAA"))
                .to_request(),
        )
        .await;
        assert_eq!(
            reponse.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "adresse acceptée à tort à l'enregistrement : {} caractères",
            adresse.len()
        );

        let suppression = test::call_service(
            &app,
            test::TestRequest::delete()
                .uri("/api/v1/push/abonnements")
                .set_json(serde_json::json!({ "endpoint": adresse }))
                .to_request(),
        )
        .await;
        assert_eq!(
            suppression.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "adresse acceptée à tort à la suppression : {} caractères",
            adresse.len()
        );
    }
}
