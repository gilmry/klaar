//! Ce que rend l'API quand aucune route ne correspond (ADR-004).
//!
//! Le routage d'actix-web par macro `#[get("…")]` crée une ressource par
//! gestionnaire, gardée par sa méthode : une méthode non déclarée ne satisfait
//! aucune garde et tombait donc sur un **404**, là où HTTP demande un **405**
//! assorti d'un en-tête `Allow`. La nuance dit à qui appelle « ce n'est pas
//! l'adresse qui est fausse, c'est le verbe », soit la moitié du diagnostic.
//!
//! C'est aussi ce que vérifie le check `unsupported_method` de `schemathesis`,
//! exclu en CI jusqu'ici faute de ce repli, et réactivé avec lui.

use actix_web::{http::StatusCode, test};
use klaar_api::{app_de_test, etat_de_test};
use klaar_sqlx_repos::{creer_pool, PoolPg};

async fn pool() -> PoolPg {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI");
    creer_pool(&url).await.expect("connexion PostgreSQL")
}

async fn app() -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    test::init_service(app_de_test(etat_de_test(pool().await, None))).await
}

#[actix_web::test]
async fn happy_une_methode_non_declaree_rend_405_et_annonce_les_autres() {
    let app = app().await;

    // `/api/v1/catalog/sectors` n'existe qu'en lecture.
    let reponse = test::call_service(
        &app,
        test::TestRequest::delete()
            .uri("/api/v1/catalog/sectors")
            .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::METHOD_NOT_ALLOWED);
    let permis = reponse
        .headers()
        .get(actix_web::http::header::ALLOW)
        .expect("un 405 sans `Allow` laisse l'appelant deviner")
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        permis.contains("GET"),
        "Allow devrait citer GET, vu : {permis}"
    );
}

#[actix_web::test]
async fn happy_un_chemin_a_plusieurs_methodes_les_annonce_toutes() {
    // Le cas qui rendait la correction nécessaire : plusieurs verbes sur un même
    // chemin. `Allow` doit les citer tous, pas seulement celui qu'on a essayé.
    let app = app().await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::put()
            .uri("/api/v1/providers/me/availability")
            .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::METHOD_NOT_ALLOWED);
    let permis = reponse
        .headers()
        .get(actix_web::http::header::ALLOW)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(permis.contains("GET"), "vu : {permis}");
    assert!(permis.contains("PATCH"), "vu : {permis}");
}

#[actix_web::test]
async fn edge_un_chemin_parametre_est_reconnu_comme_tel() {
    // Le repli compare à des gabarits (`/missions/{id}/dispute`), pas à des
    // chaînes : sans cela, tout chemin portant un identifiant retomberait en
    // 404 et la correction ne servirait que les rares routes fixes.
    let app = app().await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::put()
            .uri("/api/v1/missions/9f1f0e2a-0000-4000-8000-000000000000/dispute")
            .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[actix_web::test]
async fn negative_un_chemin_inconnu_reste_un_404() {
    // Le repli ne doit pas transformer toute erreur en 405 : une adresse qui
    // n'est pas au contrat n'existe pas, et le dire autrement inviterait à
    // chercher le bon verbe pour une porte qui n'est pas là.
    let app = app().await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/ceci-nexiste-pas")
            .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
    assert!(
        reponse
            .headers()
            .get(actix_web::http::header::ALLOW)
            .is_none(),
        "un 404 ne doit pas annoncer de méthodes"
    );
}

#[actix_web::test]
async fn security_le_repli_ne_dit_rien_de_plus_que_le_verbe() {
    // Un corps d'erreur bavard sur un chemin refusé est une aide au balayage.
    // Ici, un code et rien d'autre.
    let app = app().await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::delete()
            .uri("/api/v1/catalog/sectors")
            .to_request(),
    )
    .await;

    let corps: serde_json::Value = test::read_body_json(reponse).await;
    assert_eq!(corps, serde_json::json!({ "code": "METHOD_NOT_ALLOWED" }));
}
