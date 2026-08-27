//! Story 1.1 — endpoint d'inscription, monté en mémoire sur une base réelle.
//!
//! Le dépôt réel plutôt qu'un double : ce qui peut casser ici — l'unicité de
//! l'adresse, l'atomicité du couple compte/jeton — n'existe que dans
//! PostgreSQL. Un double en mémoire dirait que tout va bien.

use actix_web::{http::StatusCode, test};
use klaar_api::{app_de_test, etat_de_test};
use klaar_sqlx_repos::creer_pool;
use serde_json::Value;
use uuid::Uuid;

async fn pool() -> klaar_sqlx_repos::PoolPg {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI");
    creer_pool(&url).await.expect("connexion PostgreSQL")
}

/// Adresse neuve à chaque appel : les tests partagent une base et ne doivent
/// pas se gêner, ni dépendre de leur ordre d'exécution.
fn adresse(marqueur: &str) -> String {
    format!("{marqueur}-{}@example.eu", Uuid::new_v4())
}

fn corps(email: &str, mot_de_passe: &str) -> Value {
    serde_json::json!({ "email": email, "mot_de_passe": mot_de_passe })
}

#[actix_web::test]
async fn happy_une_inscription_valide_est_acceptee() {
    let app = test::init_service(app_de_test(etat_de_test(pool().await, None))).await;
    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/signup")
            .set_json(corps(&adresse("happy"), "Marie@2026Secure"))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::ACCEPTED);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "SIGNUP_ACCEPTED");
}

#[actix_web::test]
async fn happy_le_compte_est_cree_en_attente_de_verification() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let email = adresse("statut");

    test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/signup")
            .set_json(corps(&email, "Marie@2026Secure"))
            .to_request(),
    )
    .await;

    let ligne: (String, String, String) = sqlx::query_as(
        "SELECT statut, locale, empreinte_mot_de_passe FROM utilisateur WHERE email = $1",
    )
    .bind(&email)
    .fetch_one(&pool)
    .await
    .expect("le compte doit exister");
    assert_eq!(ligne.0, "PENDING_EMAIL_VERIFY");
    assert_eq!(ligne.1, "fr");
    assert!(ligne.2.starts_with("$argon2id$"));

    // Compte et jeton dans la même transaction : un compte sans jeton ne
    // pourrait jamais être activé, et l'anti-énumération empêcherait de le
    // recréer.
    let jetons: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM jeton_verification_email j
         JOIN utilisateur u ON u.id = j.utilisateur_id WHERE u.email = $1",
    )
    .bind(&email)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(jetons, 1);
}

#[actix_web::test]
async fn negative_les_quatre_saisies_invalides_du_prd_donnent_400() {
    let app = test::init_service(app_de_test(etat_de_test(pool().await, None))).await;
    let cas = [
        ("invalide", "Marie@2026Secure", "EMAIL_MALFORMED"),
        ("marie@example.eu", "court", "PASSWORD_TOO_SHORT"),
        ("marie@example.eu", "", "PASSWORD_EMPTY"),
        ("", "Marie@2026Secure", "EMAIL_EMPTY"),
    ];
    for (email, mdp, code) in cas {
        let reponse = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/auth/signup")
                .set_json(corps(email, mdp))
                .to_request(),
        )
        .await;
        assert_eq!(reponse.status(), StatusCode::BAD_REQUEST, "cas {email:?}");
        let corps: Value = test::read_body_json(reponse).await;
        assert_eq!(corps["code"], code);
    }
}

#[actix_web::test]
async fn negative_un_corps_sans_mot_de_passe_est_refuse_par_le_contrat() {
    let app = test::init_service(app_de_test(etat_de_test(pool().await, None))).await;
    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/signup")
            .set_json(serde_json::json!({ "email": "marie@example.eu" }))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn edge_deux_inscriptions_sur_la_meme_adresse_ne_creent_qu_un_compte() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let email = adresse("doublon");

    for _ in 0..2 {
        let reponse = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/auth/signup")
                .set_json(corps(&email, "Marie@2026Secure"))
                .to_request(),
        )
        .await;
        assert_eq!(reponse.status(), StatusCode::ACCEPTED);
    }

    let comptes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM utilisateur WHERE email = $1")
        .bind(&email)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(comptes, 1);
}

#[actix_web::test]
async fn edge_la_casse_de_l_adresse_ne_permet_pas_un_second_compte() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let email = adresse("Casse");

    for variante in [email.to_uppercase(), email.to_lowercase()] {
        test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/auth/signup")
                .set_json(corps(&variante, "Marie@2026Secure"))
                .to_request(),
        )
        .await;
    }

    let comptes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM utilisateur WHERE email = $1")
        .bind(email.to_lowercase())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(comptes, 1);
}

#[actix_web::test]
async fn edge_une_locale_non_supportee_se_replie_sur_le_francais() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let email = adresse("locale");

    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/signup")
            .set_json(serde_json::json!({
                "email": email, "mot_de_passe": "Marie@2026Secure", "locale": "de"
            }))
            .to_request(),
    )
    .await;
    // 202 et non 400 : une préférence d'affichage inconnue ne doit pas empêcher
    // quelqu'un de créer son compte.
    assert_eq!(reponse.status(), StatusCode::ACCEPTED);

    let locale: String = sqlx::query_scalar("SELECT locale FROM utilisateur WHERE email = $1")
        .bind(&email)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(locale, "fr");
}

#[actix_web::test]
async fn edge_les_trois_locales_supportees_sont_conservees() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    for demandee in ["fr", "nl", "en"] {
        let email = adresse(demandee);
        test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/auth/signup")
                .set_json(serde_json::json!({
                    "email": email, "mot_de_passe": "Marie@2026Secure", "locale": demandee
                }))
                .to_request(),
        )
        .await;
        let locale: String = sqlx::query_scalar("SELECT locale FROM utilisateur WHERE email = $1")
            .bind(&email)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(locale, demandee);
    }
}

#[actix_web::test]
async fn edge_la_sixieme_tentative_depuis_la_meme_source_est_limitee() {
    let app = test::init_service(app_de_test(etat_de_test(pool().await, None))).await;
    for _ in 0..5 {
        let reponse = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/auth/signup")
                .set_json(corps(&adresse("limite"), "Marie@2026Secure"))
                .to_request(),
        )
        .await;
        assert_eq!(reponse.status(), StatusCode::ACCEPTED);
    }

    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/signup")
            .set_json(corps(&adresse("limite"), "Marie@2026Secure"))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::TOO_MANY_REQUESTS);
    // FR-001 `@edge` écrit `Retry-After: 3600`. La valeur servie est le délai
    // qui libère réellement une place, donc la fenêtre moins le temps déjà
    // écoulé depuis la première tentative — 3599 ici, et non 3600. Annoncer la
    // fenêtre entière ferait attendre pour rien ; l'assertion porte donc sur un
    // intervalle, sans quoi ce test échouerait au gré de la seconde qui tourne.
    let retry_after: i64 = reponse
        .headers()
        .get("Retry-After")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .expect("Retry-After doit être présent et numérique");
    assert!(
        (3500..=3600).contains(&retry_after),
        "Retry-After hors de la fenêtre annoncée : {retry_after}"
    );
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "RATE_LIMIT_EXCEEDED");
}

#[actix_web::test]
async fn security_la_reponse_est_identique_que_l_adresse_existe_ou_non() {
    // Le coeur de l'arbitrage : c'est ce test qui échouerait si quelqu'un
    // rétablissait le 409 du scénario @negative de FR-001.
    let app = test::init_service(app_de_test(etat_de_test(pool().await, None))).await;
    let email = adresse("enum");

    let mut vues = Vec::new();
    for _ in 0..2 {
        let reponse = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/auth/signup")
                .set_json(corps(&email, "Marie@2026Secure"))
                .to_request(),
        )
        .await;
        let statut = reponse.status();
        let entetes: Vec<String> = reponse
            .headers()
            .iter()
            .filter(|(n, _)| *n != "date")
            .map(|(n, v)| format!("{n}: {}", v.to_str().unwrap_or("")))
            .collect();
        let corps = test::read_body(reponse).await;
        vues.push((statut, entetes, corps));
    }

    assert_eq!(vues[0].0, vues[1].0, "statut différent");
    assert_eq!(vues[0].2, vues[1].2, "corps différent");
    let (mut a, mut b) = (vues[0].1.clone(), vues[1].1.clone());
    a.sort();
    b.sort();
    assert_eq!(a, b, "en-têtes différents");
}

#[actix_web::test]
async fn security_le_doublon_est_audite_sans_designer_le_titulaire() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let email = adresse("audit");

    for _ in 0..2 {
        test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/auth/signup")
                .set_json(corps(&email, "Marie@2026Secure"))
                .to_request(),
        )
        .await;
    }

    let id: Uuid = sqlx::query_scalar("SELECT id FROM utilisateur WHERE email = $1")
        .bind(&email)
        .fetch_one(&pool)
        .await
        .unwrap();

    let signup: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM journal_audit WHERE code = 'USER_SIGNUP' AND sujet_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(signup, 1);

    // Le doublon est consigné, mais jamais relié au compte visé : sinon le
    // journal d'audit devient l'oracle que la réponse HTTP refuse d'être.
    let doublons_relies: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM journal_audit
         WHERE code = 'USER_SIGNUP_DUPLICATE' AND sujet_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(doublons_relies, 0);
}

#[actix_web::test]
async fn security_le_mot_de_passe_n_apparait_nulle_part_en_base() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let email = adresse("secret");
    let mot_de_passe = "MotDePasseTresParticulier@2026";

    test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/signup")
            .set_json(corps(&email, mot_de_passe))
            .to_request(),
    )
    .await;

    let trouve: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM utilisateur
         WHERE email = $1 AND empreinte_mot_de_passe LIKE '%' || $2 || '%'",
    )
    .bind(&email)
    .bind(mot_de_passe)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(trouve, 0);
}

#[actix_web::test]
async fn security_le_jeton_de_verification_n_est_pas_conserve_en_clair() {
    let pool = pool().await;
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;
    let email = adresse("jeton");

    test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/signup")
            .set_json(corps(&email, "Marie@2026Secure"))
            .to_request(),
    )
    .await;

    let empreinte: String = sqlx::query_scalar(
        "SELECT j.empreinte FROM jeton_verification_email j
         JOIN utilisateur u ON u.id = j.utilisateur_id WHERE u.email = $1",
    )
    .bind(&email)
    .fetch_one(&pool)
    .await
    .unwrap();

    // 64 hexadécimaux : une empreinte SHA-256, pas un jeton base64url de 43
    // caractères.
    assert_eq!(empreinte.len(), 64);
    assert!(empreinte.chars().all(|c| c.is_ascii_hexdigit()));
}

#[actix_web::test]
async fn security_la_reponse_ne_renvoie_jamais_le_jeton_ni_l_identifiant() {
    let app = test::init_service(app_de_test(etat_de_test(pool().await, None))).await;
    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/signup")
            .set_json(corps(&adresse("fuite"), "Marie@2026Secure"))
            .to_request(),
    )
    .await;
    let corps: Value = test::read_body_json(reponse).await;
    let objet = corps.as_object().expect("un objet JSON");
    // Un seul champ : tout ajout futur devra passer par ce test, et donc par
    // la question « est-ce que cela permet d'énumérer ? ».
    assert_eq!(objet.len(), 1, "réponse inattendue : {corps}");
    assert!(objet.contains_key("code"));
}

#[actix_web::test]
async fn security_un_champ_inconnu_dans_la_charge_est_refuse() {
    // `deny_unknown_fields` : accepter silencieusement un champ non prévu
    // laisse croire qu'il a été pris en compte, ce qui est la manière habituelle
    // dont un `role: "admin"` finit par être ignoré sans que personne ne le voie.
    let app = test::init_service(app_de_test(etat_de_test(pool().await, None))).await;
    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/signup")
            .set_json(serde_json::json!({
                "email": adresse("inconnu"),
                "mot_de_passe": "Marie@2026Secure",
                "statut": "ACTIVE"
            }))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::BAD_REQUEST);
}
