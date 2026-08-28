//! Story 2.2 — lecture du catalogue (FR-008), contre un vrai PostgreSQL.

use actix_web::{http::StatusCode, test};
use klaar_api::routes::catalogue::{CACHE_SECONDES, RETRY_MAINTENANCE_SECONDES};
use klaar_api::{app_de_test, etat_de_test, EtatApplication};
use klaar_sqlx_repos::{creer_pool, PoolPg};
use serde_json::Value;
use tokio::sync::Mutex;

/// Sérialise les cas qui modifient le contenu du catalogue, et ceux dont le
/// résultat en dépend.
///
/// Les tests d'un même binaire tournent en parallèle sur une base partagée : un
/// cas qui insère une fourchette change la réponse — donc l'`ETag` — que lit un
/// cas voisin au même instant. Un verrou de processus vaut mieux qu'un test qui
/// échoue une fois sur vingt sans qu'on sache pourquoi.
///
/// Verrou **asynchrone** et non `std::sync::Mutex` : le garde traverse des
/// `await`, et un verrou bloquant tenu à travers une attente immobilise le fil
/// d'exécution qui la porte.
static CONTENU: Mutex<()> = Mutex::const_new(());

async fn verrou() -> tokio::sync::MutexGuard<'static, ()> {
    CONTENU.lock().await
}

async fn pool() -> PoolPg {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI");
    creer_pool(&url).await.expect("connexion PostgreSQL")
}

fn requete(locale: Option<&str>) -> test::TestRequest {
    let uri = match locale {
        Some(l) => format!("/api/v1/catalog/sectors?locale={l}"),
        None => "/api/v1/catalog/sectors".to_string(),
    };
    test::TestRequest::get().uri(&uri)
}

/// Adresse source distincte : le quota de lecture est de 60 par minute, et
/// plusieurs cas de ce fichier tournent en parallèle sur le même limiteur.
fn depuis(locale: Option<&str>, source: u8) -> test::TestRequest {
    requete(locale).peer_addr(format!("10.1.0.{source}:40000").parse().unwrap())
}

#[actix_web::test]
async fn happy_sert_les_secteurs_du_mvp_avec_leurs_skills_en_francais() {
    let app = test::init_service(app_de_test(etat_de_test(pool().await, None))).await;
    let reponse = test::call_service(&app, depuis(Some("fr"), 1).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::OK);

    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["locale"], "fr");
    let secteurs = corps["secteurs"].as_array().expect("une liste");

    // **Présence, et non comptage exact.** La Story 2.4 rend le catalogue
    // extensible : l'exploitation peut publier de nouveaux secteurs, et un test
    // qui exige exactement cinq devient faux le jour où elle le fait. Ce que ce
    // cas vérifie est que le peuplement du MVP est servi avec ses libellés et
    // ses compétences — pas que personne n'a rien ajouté depuis.
    for attendu in [
        "plomberie",
        "serrurerie",
        "electricite",
        "auto",
        "livraison",
    ] {
        let trouve = secteurs
            .iter()
            .find(|s| s["code"] == attendu)
            .unwrap_or_else(|| panic!("secteur {attendu} absent du catalogue"));
        assert!(
            !trouve["libelle"].as_str().unwrap_or_default().is_empty(),
            "{attendu} sans libellé"
        );
    }
    let plomberie = secteurs.iter().find(|s| s["code"] == "plomberie").unwrap();
    assert_eq!(plomberie["libelle"], "Plomberie");
    assert!(!plomberie["skills"].as_array().unwrap().is_empty());
}

#[actix_web::test]
async fn happy_le_meme_catalogue_change_de_langue() {
    let app = test::init_service(app_de_test(etat_de_test(pool().await, None))).await;

    let fr: Value =
        test::read_body_json(test::call_service(&app, depuis(Some("fr"), 2).to_request()).await)
            .await;
    let nl: Value =
        test::read_body_json(test::call_service(&app, depuis(Some("nl"), 3).to_request()).await)
            .await;

    // Mêmes codes, libellés différents : c'est le code qui identifie, jamais le
    // libellé.
    let codes = |v: &Value| -> Vec<String> {
        v["secteurs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["code"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(codes(&fr), codes(&nl));
    assert_eq!(nl["locale"], "nl");
    assert_eq!(nl["secteurs"][0]["libelle"], "Loodgieterij");
    assert_ne!(fr["secteurs"][0]["libelle"], nl["secteurs"][0]["libelle"]);
}

#[actix_web::test]
async fn negative_une_langue_non_prise_en_charge_replie_et_le_dit() {
    // FR-008 `@negative` : 200 avec repli, et l'avertissement **rendu au
    // client**. C'est à lui d'apprendre qu'il n'aura pas la langue demandée,
    // pas à l'exploitant de le découvrir dans ses journaux.
    let app = test::init_service(app_de_test(etat_de_test(pool().await, None))).await;
    let reponse = test::call_service(&app, depuis(Some("de"), 4).to_request()).await;
    assert_eq!(reponse.status(), StatusCode::OK);

    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["locale"], "fr");
    assert_eq!(corps["avertissement"], "LOCALE_FALLBACK");
}

#[actix_web::test]
async fn negative_la_maintenance_repond_503_avec_un_delai() {
    let pool = pool().await;
    let etat = etat_de_test(pool.clone(), None);
    // L'état de test n'expose pas de constructeur paramétré : la maintenance se
    // pose ici, ce qui est le seul endroit où elle doit pouvoir l'être.
    let etat = actix_web::web::Data::new(EtatApplication {
        catalogue_en_maintenance: true,
        ..(**etat).clone()
    });
    let app = test::init_service(app_de_test(etat)).await;

    let reponse = test::call_service(&app, depuis(Some("fr"), 5).to_request()).await;
    // 503 et non 500 : le service n'est pas en panne, il est retiré le temps
    // d'une mise à jour.
    assert_eq!(reponse.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        reponse
            .headers()
            .get("Retry-After")
            .and_then(|v| v.to_str().ok()),
        Some(RETRY_MAINTENANCE_SECONDES.to_string().as_str())
    );
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "CATALOG_MAINTENANCE");
}

#[actix_web::test]
async fn edge_l_absence_de_parametre_sert_le_francais_sans_avertir() {
    // Ne rien demander n'est pas demander une langue inconnue : l'avertissement
    // signalerait un problème là où il n'y en a pas.
    let app = test::init_service(app_de_test(etat_de_test(pool().await, None))).await;
    let corps: Value =
        test::read_body_json(test::call_service(&app, depuis(None, 6).to_request()).await).await;
    assert_eq!(corps["locale"], "fr");
    assert!(corps.get("avertissement").is_none());
}

#[actix_web::test]
async fn edge_un_etag_presente_repond_304_sans_corps() {
    let _garde = verrou().await;
    let app = test::init_service(app_de_test(etat_de_test(pool().await, None))).await;

    let premiere = test::call_service(&app, depuis(Some("fr"), 7).to_request()).await;
    let etag = premiere
        .headers()
        .get("ETag")
        .and_then(|v| v.to_str().ok())
        .expect("un ETag")
        .to_string();

    let seconde = test::call_service(
        &app,
        depuis(Some("fr"), 7)
            .insert_header(("If-None-Match", etag.clone()))
            .to_request(),
    )
    .await;
    assert_eq!(seconde.status(), StatusCode::NOT_MODIFIED);
    assert!(
        test::read_body(seconde).await.is_empty(),
        "un 304 ne porte pas de corps"
    );
}

#[actix_web::test]
async fn edge_deux_langues_ont_deux_etags_distincts() {
    let _garde = verrou().await;
    // Sinon un cache servirait le catalogue néerlandais à qui demande le
    // français, ce que le même ETag l'autoriserait à croire correct.
    let app = test::init_service(app_de_test(etat_de_test(pool().await, None))).await;
    let etag = |r: actix_web::dev::ServiceResponse| {
        r.headers()
            .get("ETag")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };

    let fr = etag(test::call_service(&app, depuis(Some("fr"), 8).to_request()).await);
    let nl = etag(test::call_service(&app, depuis(Some("nl"), 9).to_request()).await);
    assert!(fr.is_some() && nl.is_some());
    assert_ne!(fr, nl);
}

#[actix_web::test]
async fn edge_un_etag_perime_redonne_le_contenu() {
    let app = test::init_service(app_de_test(etat_de_test(pool().await, None))).await;
    let reponse = test::call_service(
        &app,
        depuis(Some("fr"), 10)
            .insert_header(("If-None-Match", "\"une-empreinte-qui-n-existe-plus\""))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    // Le contenu est bien redonné : ce que ce cas vérifie est qu'une empreinte
    // périmée ne produit pas un 304 vide, pas le nombre de secteurs — lequel
    // bouge depuis que le catalogue est extensible (Story 2.4).
    assert!(!corps["secteurs"].as_array().unwrap().is_empty());
}

#[actix_web::test]
async fn security_la_reponse_est_cachable_publiquement_cinq_minutes() {
    // `public` : le catalogue est le même pour tout le monde, aucun cache
    // intermédiaire ne risque de servir à l'un ce qui était destiné à l'autre.
    let app = test::init_service(app_de_test(etat_de_test(pool().await, None))).await;
    let reponse = test::call_service(&app, depuis(Some("fr"), 11).to_request()).await;
    let cache = reponse
        .headers()
        .get("Cache-Control")
        .and_then(|v| v.to_str().ok())
        .expect("un Cache-Control")
        .to_string();
    assert!(cache.contains("public"), "en-tête : {cache}");
    assert!(
        cache.contains(&format!("max-age={CACHE_SECONDES}")),
        "en-tête : {cache}"
    );
}

#[actix_web::test]
async fn security_la_lecture_est_limitee_a_soixante_par_minute() {
    let app = test::init_service(app_de_test(etat_de_test(pool().await, None))).await;
    for i in 0..60 {
        let reponse = test::call_service(&app, depuis(Some("fr"), 200).to_request()).await;
        assert_eq!(reponse.status(), StatusCode::OK, "appel {i}");
    }
    let refuse = test::call_service(&app, depuis(Some("fr"), 200).to_request()).await;
    assert_eq!(refuse.status(), StatusCode::TOO_MANY_REQUESTS);
    let corps: Value = test::read_body_json(refuse).await;
    assert_eq!(corps["code"], "RATE_LIMIT_EXCEEDED");
}

#[actix_web::test]
async fn security_le_catalogue_ne_partage_pas_son_quota_avec_la_connexion() {
    // Sans préfixe de clé, consulter le catalogue épuiserait le droit de se
    // connecter, et le lien entre les deux serait incompréhensible.
    let app = test::init_service(app_de_test(etat_de_test(pool().await, None))).await;
    for _ in 0..10 {
        test::call_service(&app, depuis(Some("fr"), 201).to_request()).await;
    }

    let connexion = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/login")
            .peer_addr("10.1.0.201:40000".parse().unwrap())
            .set_json(serde_json::json!({
                "email": "personne@example.eu",
                "mot_de_passe": "Marie@2026Secure"
            }))
            .to_request(),
    )
    .await;
    // 401 et non 429 : le budget de connexion est intact.
    assert_eq!(connexion.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn security_la_reponse_ne_porte_aucune_donnee_personnelle() {
    // Un catalogue mis en cache `public` ne doit rien contenir de propre à
    // celui qui l'a demandé. Ce test échouerait si quelqu'un y ajoutait, par
    // exemple, les secteurs récemment consultés.
    let app = test::init_service(app_de_test(etat_de_test(pool().await, None))).await;
    let reponse = test::call_service(&app, depuis(Some("fr"), 12).to_request()).await;
    let brut = String::from_utf8(test::read_body(reponse).await.to_vec()).unwrap();
    assert!(!brut.contains('@'), "corps : {brut}");
    for champ in ["utilisateur", "session", "jeton", "email"] {
        assert!(
            !brut.contains(champ),
            "le champ {champ} n'a rien à faire ici"
        );
    }
}

#[actix_web::test]
async fn negative_sans_historique_aucun_secteur_ne_porte_de_fourchette() {
    // FR-009 `@negative` : au lancement, aucune fourchette. L'absence du champ
    // dit « prix sur devis », et non « prix inconnu ».
    let _garde = verrou().await;
    let pool = pool().await;
    sqlx::query("DELETE FROM fourchette_prix")
        .execute(&pool)
        .await
        .unwrap();
    let app = test::init_service(app_de_test(etat_de_test(pool, None))).await;

    let corps: Value =
        test::read_body_json(test::call_service(&app, depuis(Some("fr"), 20).to_request()).await)
            .await;
    for secteur in corps["secteurs"].as_array().unwrap() {
        assert!(
            secteur.get("fourchette").is_none(),
            "secteur {} : {secteur}",
            secteur["code"]
        );
    }
}

#[actix_web::test]
async fn happy_une_fourchette_calculee_est_servie_en_centimes() {
    let _garde = verrou().await;
    let pool = pool().await;
    sqlx::query(
        "INSERT INTO fourchette_prix (secteur_code, min_cents, max_cents, nb_missions, calculee_le)
         VALUES ('plomberie', 8000, 20000, 12, now())
         ON CONFLICT (secteur_code) DO UPDATE
             SET min_cents = EXCLUDED.min_cents, max_cents = EXCLUDED.max_cents",
    )
    .execute(&pool)
    .await
    .unwrap();
    let app = test::init_service(app_de_test(etat_de_test(pool.clone(), None))).await;

    let corps: Value =
        test::read_body_json(test::call_service(&app, depuis(Some("fr"), 21).to_request()).await)
            .await;
    let plomberie = corps["secteurs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["code"] == "plomberie")
        .expect("le secteur plomberie");
    // En centimes, pas en euros : c'est au client de choisir son format, et un
    // arrondi côté serveur ferait diverger l'affiché du calculé.
    assert_eq!(plomberie["fourchette"]["min_cents"], 8000);
    assert_eq!(plomberie["fourchette"]["max_cents"], 20000);

    // Les autres secteurs restent sans fourchette : une seule ligne insérée.
    let serrurerie = corps["secteurs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["code"] == "serrurerie")
        .unwrap();
    assert!(serrurerie.get("fourchette").is_none());

    sqlx::query("DELETE FROM fourchette_prix WHERE secteur_code = 'plomberie'")
        .execute(&pool)
        .await
        .unwrap();
}

#[actix_web::test]
async fn security_la_base_refuse_une_fourchette_sous_le_seuil_d_anonymat() {
    // Le seuil de FR-009 `@security` n'est pas seulement dans le calcul : la
    // base le repose, pour qu'aucun chemin d'écriture ne puisse le contourner.
    let pool = pool().await;
    let erreur = sqlx::query(
        "INSERT INTO fourchette_prix (secteur_code, min_cents, max_cents, nb_missions, calculee_le)
         VALUES ('auto', 8000, 20000, 2, now())",
    )
    .execute(&pool)
    .await;
    assert!(erreur.is_err(), "deux Missions ne doivent pas passer");

    // Et des bornes inversées non plus : elles signaleraient un calcul faux.
    let inversee = sqlx::query(
        "INSERT INTO fourchette_prix (secteur_code, min_cents, max_cents, nb_missions, calculee_le)
         VALUES ('auto', 20000, 8000, 12, now())",
    )
    .execute(&pool)
    .await;
    assert!(inversee.is_err());
}
