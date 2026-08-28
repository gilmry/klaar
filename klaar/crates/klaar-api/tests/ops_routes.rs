//! Story 8.4 — console d'exploitation (FR-041, FR-042), contre PostgreSQL.
//!
//! **Ce qui ne se teste qu'ici :** que la seconde authentification soit
//! réellement exigée, que le rejeu d'un code soit fermé par la base, et que le
//! journal d'exploitation refuse toute modification.

use actix_web::{http::StatusCode, test};
use chrono::Utc;
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

/// Crée un compte d'exploitation avec sa seconde authentification prête.
async fn ops(pool: &PoolPg, role: &str, marqueur: &str) -> (CompteOps, Vec<u8>) {
    let email = Email::parse(&format!("ops-{marqueur}-{}@klaar.test", Uuid::new_v4())).unwrap();
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

/// Le code courant pour ce secret.
fn code(secret: &[u8]) -> String {
    calculer_totp(secret, Utc::now().timestamp().div_euclid(TOTP_PAS_SECONDES))
}

macro_rules! bac {
    ($pool:expr) => {
        test::init_service(app_de_test(etat_de_test($pool.clone(), None))).await
    };
}

fn connexion(email: &str, mot_de_passe: &str, code: &str) -> test::TestRequest {
    test::TestRequest::post()
        .uri("/api/v1/ops/login")
        .set_json(serde_json::json!({
            "email": email, "mot_de_passe": mot_de_passe, "code": code
        }))
}

/// Les identifiants voyagent en paramètres pour les routes authentifiées.
fn parametres(compte: &CompteOps, secret: &[u8]) -> String {
    format!(
        "email={}&mot_de_passe={}&code={}",
        urlencoding(compte.email.as_str()),
        urlencoding(MDP),
        code(secret)
    )
}

/// Encodage minimal, suffisant pour une adresse et un mot de passe de test.
fn urlencoding(brut: &str) -> String {
    brut.replace('@', "%40").replace('+', "%2B")
}

// === @happy ===

#[actix_web::test]
async fn happy_un_compte_avec_son_code_se_connecte() {
    let pool = pool().await;
    let app = bac!(pool);
    let (compte, secret) = ops(&pool, "SUPER_ADMIN", "connexion").await;

    let reponse = test::call_service(
        &app,
        connexion(compte.email.as_str(), MDP, &code(&secret)).to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "OPS_AUTHENTICATED");
    assert_eq!(corps["role"], "SUPER_ADMIN");
}

#[actix_web::test]
async fn happy_un_super_admin_cree_un_compte_et_recoit_son_secret() {
    let pool = pool().await;
    let app = bac!(pool);
    let (patron, secret) = ops(&pool, "SUPER_ADMIN", "createur").await;

    let nouvelle = format!("nouvel-ops-{}@klaar.test", Uuid::new_v4());
    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!(
                "/api/v1/ops/accounts?{}",
                parametres(&patron, &secret)
            ))
            .set_json(serde_json::json!({
                "email": nouvelle, "mot_de_passe": MDP, "role": "KYC_REVIEWER"
            }))
            .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::CREATED);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "OPS_ACCOUNT_CREATED");
    assert_eq!(corps["role"], "KYC_REVIEWER");
    // Le secret est rendu une fois, en base32 lisible par une application
    // d'authentification.
    let secret_lisible = corps["secret_totp"].as_str().expect("secret");
    assert!(secret_lisible.len() >= 32);
    assert!(secret_lisible
        .chars()
        .all(|c| c.is_ascii_uppercase() || ('2'..='7').contains(&c)));
}

// === @negative ===

#[actix_web::test]
async fn negative_sans_code_la_connexion_est_refusee() {
    // FR-041 `@security` : sans seconde authentification, accès bloqué.
    let pool = pool().await;
    let app = bac!(pool);
    let (compte, _) = ops(&pool, "SUPER_ADMIN", "sans-code").await;

    for mauvais in ["", "000000", "12345", "abcdef"] {
        let reponse = test::call_service(
            &app,
            connexion(compte.email.as_str(), MDP, mauvais).to_request(),
        )
        .await;
        assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED, "code {mauvais}");
    }
}

#[actix_web::test]
async fn negative_un_mauvais_mot_de_passe_est_refuse() {
    let pool = pool().await;
    let app = bac!(pool);
    let (compte, secret) = ops(&pool, "SUPER_ADMIN", "mdp-faux").await;

    let reponse = test::call_service(
        &app,
        connexion(compte.email.as_str(), "Autre@2026Secure", &code(&secret)).to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn negative_un_role_inconnu_est_refuse_a_la_creation() {
    // FR-041 `@negative` : 422.
    let pool = pool().await;
    let app = bac!(pool);
    let (patron, secret) = ops(&pool, "SUPER_ADMIN", "role-faux").await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!(
                "/api/v1/ops/accounts?{}",
                parametres(&patron, &secret)
            ))
            .set_json(serde_json::json!({
                "email": format!("x-{}@klaar.test", Uuid::new_v4()),
                "mot_de_passe": MDP,
                "role": "super_root"
            }))
            .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "ROLE_UNKNOWN");
}

// === @edge ===

#[actix_web::test]
async fn edge_un_compte_desactive_ne_se_connecte_plus() {
    let pool = pool().await;
    let app = bac!(pool);
    let (compte, secret) = ops(&pool, "SUPER_ADMIN", "revoque").await;
    sqlx::query("UPDATE compte_ops SET actif = FALSE WHERE id = $1")
        .bind(compte.id)
        .execute(&pool)
        .await
        .expect("désactivation");

    let reponse = test::call_service(
        &app,
        connexion(compte.email.as_str(), MDP, &code(&secret)).to_request(),
    )
    .await;

    // 403 et non 401 : les identifiants sont bons, c'est l'état du compte qui
    // refuse. Le distinguer n'apprend rien à qui n'a pas le mot de passe,
    // puisqu'il faut l'avoir pour arriver jusqu'ici.
    assert_eq!(reponse.status(), StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn edge_une_adresse_deja_prise_est_refusee() {
    let pool = pool().await;
    let app = bac!(pool);
    let (patron, secret) = ops(&pool, "SUPER_ADMIN", "doublon").await;
    let (existant, _) = ops(&pool, "READER", "existant").await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!(
                "/api/v1/ops/accounts?{}",
                parametres(&patron, &secret)
            ))
            .set_json(serde_json::json!({
                "email": existant.email.as_str(), "mot_de_passe": MDP, "role": "READER"
            }))
            .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::CONFLICT);
}

// === @security ===

#[actix_web::test]
async fn security_un_code_deja_utilise_ne_repasse_pas() {
    // **Sans cela, un code lu par-dessus une épaule reste utilisable une minute
    // et demie.** C'est la fenêtre de tolérance qui l'exige, et le
    // compare-and-swap en base qui la referme.
    let pool = pool().await;
    let app = bac!(pool);
    let (compte, secret) = ops(&pool, "SUPER_ADMIN", "rejeu").await;
    let unique = code(&secret);

    let premiere = test::call_service(
        &app,
        connexion(compte.email.as_str(), MDP, &unique).to_request(),
    )
    .await;
    assert_eq!(premiere.status(), StatusCode::OK);

    let seconde = test::call_service(
        &app,
        connexion(compte.email.as_str(), MDP, &unique).to_request(),
    )
    .await;
    assert_eq!(
        seconde.status(),
        StatusCode::UNAUTHORIZED,
        "un code consommé ne doit pas repasser"
    );
}

#[actix_web::test]
async fn security_un_lecteur_ne_cree_pas_de_compte() {
    // Qui peut créer un compte peut se créer un super-administrateur : ce droit
    // n'appartient qu'à un seul rôle.
    let pool = pool().await;
    let app = bac!(pool);
    let (lecteur, secret) = ops(&pool, "READER", "lecteur-createur").await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!(
                "/api/v1/ops/accounts?{}",
                parametres(&lecteur, &secret)
            ))
            .set_json(serde_json::json!({
                "email": format!("x-{}@klaar.test", Uuid::new_v4()),
                "mot_de_passe": MDP,
                "role": "SUPER_ADMIN"
            }))
            .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::FORBIDDEN);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "FORBIDDEN");
}

#[actix_web::test]
async fn security_un_refus_de_droit_est_consigne() {
    // Une tentative d'accès hors droits est précisément ce qu'un journal
    // d'exploitation doit montrer.
    let pool = pool().await;
    let app = bac!(pool);
    let (lecteur, secret) = ops(&pool, "READER", "refus-consigne").await;

    test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!(
                "/api/v1/ops/accounts?{}",
                parametres(&lecteur, &secret)
            ))
            .set_json(serde_json::json!({
                "email": format!("x-{}@klaar.test", Uuid::new_v4()),
                "mot_de_passe": MDP,
                "role": "READER"
            }))
            .to_request(),
    )
    .await;

    let gestes: Vec<String> = sqlx::query_scalar("SELECT geste FROM journal_ops WHERE ops_id = $1")
        .bind(lecteur.id)
        .fetch_all(&pool)
        .await
        .expect("journal");
    assert!(
        gestes.iter().any(|g| g == "OPS_MANAGE_DENIED"),
        "le refus doit être consigné : {gestes:?}"
    );
}

#[actix_web::test]
async fn security_la_lecture_du_journal_est_elle_meme_journalisee() {
    // Qui a consulté quoi est ce qu'un audit de sécurité vient chercher : un
    // journal qui ne consigne pas ses propres lectures ne dit qu'une moitié de
    // l'histoire.
    let pool = pool().await;
    let app = bac!(pool);
    let (lecteur, secret) = ops(&pool, "READER", "lecture-tracee").await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/v1/ops/audit?{}",
                parametres(&lecteur, &secret)
            ))
            .to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["par_page"], 50);

    let gestes: Vec<String> = sqlx::query_scalar("SELECT geste FROM journal_ops WHERE ops_id = $1")
        .bind(lecteur.id)
        .fetch_all(&pool)
        .await
        .expect("journal");
    assert!(gestes.iter().any(|g| g == "AUDIT_READ"), "{gestes:?}");
}

#[actix_web::test]
async fn security_le_journal_d_exploitation_ne_se_modifie_pas() {
    // FR-042 `@security` : « même un super-admin ne peut modifier ».
    let pool = pool().await;
    let app = bac!(pool);
    let (lecteur, secret) = ops(&pool, "READER", "journal-fige").await;
    test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/v1/ops/audit?{}",
                parametres(&lecteur, &secret)
            ))
            .to_request(),
    )
    .await;

    for tentative in [
        "UPDATE journal_ops SET geste = 'RIEN' WHERE ops_id = $1",
        "DELETE FROM journal_ops WHERE ops_id = $1",
    ] {
        let refus = sqlx::query(tentative)
            .bind(lecteur.id)
            .execute(&pool)
            .await
            .expect_err("le journal doit être insert-only");
        assert!(
            refus.to_string().contains("insert-only"),
            "déclencheur attendu, obtenu : {refus}"
        );
    }
}

#[actix_web::test]
async fn security_un_secret_deja_configure_ne_se_remplace_pas() {
    // Le remplacer permettrait à quelqu'un qui a volé une session de
    // reconfigurer la seconde authentification sur son propre téléphone.
    let pool = pool().await;
    let (compte, _) = ops(&pool, "SUPER_ADMIN", "secret-fige").await;
    let depot = PgOpsRepository::new(pool.clone());

    let remplace = depot
        .configurer_totp(compte.id, &[9u8; 20])
        .await
        .expect("appel abouti");
    assert!(!remplace, "un secret existant ne doit pas être écrasé");

    let secret: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT secret_totp FROM compte_ops WHERE id = $1")
            .bind(compte.id)
            .fetch_one(&pool)
            .await
            .expect("compte relu");
    assert_eq!(secret, Some(vec![7u8; 20]), "le secret d'origine tient");
}

#[actix_web::test]
async fn security_une_adresse_inconnue_donne_le_meme_refus() {
    // Distinguer « cette adresse n'existe pas » de « le mot de passe est faux »
    // donnerait la liste des comptes d'exploitation à qui essaie.
    let pool = pool().await;
    let app = bac!(pool);

    let reponse = test::call_service(
        &app,
        connexion("personne@klaar.test", MDP, "123456").to_request(),
    )
    .await;
    assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "OPS_CREDENTIALS_INVALID");
}

// === Exports réglementaires (Story 8.2, FR-039) ===

#[actix_web::test]
async fn happy_l_export_rgpd_rend_les_donnees_du_compte() {
    let pool = pool().await;
    let app = bac!(pool);
    let (lecteur, secret) = ops(&pool, "READER", "export-rgpd").await;

    // Un compte utilisateur ordinaire, avec une Demande.
    let utilisateur = Uuid::new_v4();
    let empreinte =
        EmpreinteMotDePasse::calculer(&MotDePasse::parse(MDP).unwrap(), ParametresArgon2::tests())
            .unwrap();
    sqlx::query(
        "INSERT INTO utilisateur (id, email, empreinte_mot_de_passe, statut, locale, cree_le)
         VALUES ($1, $2, $3, 'ACTIVE', 'fr', now())",
    )
    .bind(utilisateur)
    .bind(format!("exporte-{utilisateur}@example.eu"))
    .bind(empreinte.as_str())
    .execute(&pool)
    .await
    .expect("compte");

    let reponse = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/v1/ops/exports/gdpr?{}&utilisateur={utilisateur}",
                parametres(&lecteur, &secret)
            ))
            .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::OK);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "GDPR_EXPORT");
    assert_eq!(
        corps["donnees"]["utilisateur"]["id"],
        utilisateur.to_string()
    );
    // Les sections portent le nom exact des tables, et sont présentes même
    // vides : leur absence laisserait croire que la question n'a pas été posée.
    for section in ["session_refresh", "demande", "journal_audit", "message"] {
        assert!(
            corps["donnees"][section].is_array(),
            "section manquante : {section}"
        );
    }
}

#[actix_web::test]
async fn negative_un_compte_inconnu_n_est_pas_un_export_vide() {
    // Une autorité qui reçoit un export vide alors que le compte n'existe pas
    // en tirera la mauvaise conclusion.
    let pool = pool().await;
    let app = bac!(pool);
    let (lecteur, secret) = ops(&pool, "READER", "export-absent").await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/v1/ops/exports/gdpr?{}&utilisateur={}",
                parametres(&lecteur, &secret),
                Uuid::new_v4()
            ))
            .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn negative_une_periode_a_l_envers_est_refusee() {
    // FR-039 `@negative` : une période impossible est une erreur de saisie, pas
    // un export vide dont on conclurait « aucune activité ».
    let pool = pool().await;
    let app = bac!(pool);
    let (lecteur, secret) = ops(&pool, "READER", "periode-envers").await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/v1/ops/exports/vat?{}&debut=2026-12-31T00:00:00Z&fin=2026-01-01T00:00:00Z",
                parametres(&lecteur, &secret)
            ))
            .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let corps: Value = test::read_body_json(reponse).await;
    assert_eq!(corps["code"], "PERIOD_INVALID");
}

#[actix_web::test]
async fn happy_l_export_tva_rend_un_csv_en_centimes() {
    let pool = pool().await;
    let app = bac!(pool);
    let (lecteur, secret) = ops(&pool, "READER", "export-tva").await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/v1/ops/exports/vat?{}&debut=2026-01-01T00:00:00Z&fin=2027-01-01T00:00:00Z",
                parametres(&lecteur, &secret)
            ))
            .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::OK);
    let entete = reponse
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(entete.starts_with("text/csv"), "type rendu : {entete}");

    let corps = String::from_utf8(test::read_body(reponse).await.to_vec()).expect("utf-8");
    // Les colonnes disent « cents » : un tableur qui relit « 217,80 » selon sa
    // locale produit tantôt 217,8 tantôt 21780, et personne ne s'en aperçoit
    // avant le contrôle.
    assert!(corps.starts_with("devis_id;decidee_le;taux_tva_bp;montant_htva_cents"));
}

#[actix_web::test]
async fn security_l_export_rgpd_couvre_toutes_les_tables_qui_portent_un_compte() {
    // **C'est le test qui donne sa valeur à l'export.** L'article 15 donne
    // droit à *toutes* les données à caractère personnel, pas à celles qu'on a
    // pensé à inclure. La liste vient du schéma — `information_schema` — et non
    // d'une énumération écrite à la main, qui se désynchroniserait.
    //
    // Le jour où quelqu'un ajoute une table référençant un compte sans toucher
    // à l'export, ce test tombe.
    use klaar_application::ports::export_repository::ExportRepository;
    use klaar_sqlx_repos::PgExportRepository;

    let pool = pool().await;
    let depot = PgExportRepository::new(pool.clone());
    let tables = depot.tables_personnelles().await.expect("schéma lisible");
    assert!(!tables.is_empty(), "le schéma doit déclarer des références");

    let utilisateur = Uuid::new_v4();
    let empreinte =
        EmpreinteMotDePasse::calculer(&MotDePasse::parse(MDP).unwrap(), ParametresArgon2::tests())
            .unwrap();
    sqlx::query(
        "INSERT INTO utilisateur (id, email, empreinte_mot_de_passe, statut, locale, cree_le)
         VALUES ($1, $2, $3, 'ACTIVE', 'fr', now())",
    )
    .bind(utilisateur)
    .bind(format!("couverture-{utilisateur}@example.eu"))
    .bind(empreinte.as_str())
    .execute(&pool)
    .await
    .expect("compte");

    let export = depot
        .donnees_personnelles(utilisateur)
        .await
        .expect("export")
        .expect("compte présent");
    let rendu = export.to_string();

    // Chaque table du schéma doit apparaître quelque part dans l'export : soit
    // par son nom de section, soit parce que ses données y sont incluses par
    // une autre. `mission` et `devis` passent par `demandes` — un demandeur les
    // atteint par sa Demande — et sont donc listées ici avec leur raison.
    let par_ricochet = [
        // Rattachées à une Demande, donc atteintes par `demandes`.
        ("mission", "atteinte par la Demande"),
        ("devis", "rattaché à la Mission du demandeur"),
        ("liberation", "rattachée à la Mission"),
        ("annulation_mission", "rattachée à la Mission"),
        ("trace_matching", "rattachée à la Demande"),
        // Le prestataire est une fiche, exportée sous `fiche_prestataire`.
        ("provider", "exporté sous fiche_prestataire"),
        ("reputation_provider", "agrégat de la fiche prestataire"),
        // Les comptes d'exploitation ne référencent pas `utilisateur`.
        ("compte_ops", "espace de noms séparé"),
        (
            "journal_ops",
            "journal d'exploitation, pas de données du compte",
        ),
    ];

    let mut oubliees = Vec::new();
    for t in &tables {
        let couverte =
            rendu.contains(&t.table) || par_ricochet.iter().any(|(nom, _)| *nom == t.table);
        if !couverte {
            oubliees.push(format!("{}.{}", t.table, t.colonne));
        }
    }
    assert!(
        oubliees.is_empty(),
        "tables portant des données d'un compte et absentes de l'export : {oubliees:?}"
    );
}

#[actix_web::test]
async fn security_un_export_est_journalise_avant_d_etre_produit() {
    // FR-039 `@security` : sortir les données de quelqu'un est le geste le plus
    // lourd de cette console.
    let pool = pool().await;
    let app = bac!(pool);
    let (lecteur, secret) = ops(&pool, "READER", "export-trace").await;
    let cible = Uuid::new_v4();

    test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/v1/ops/exports/gdpr?{}&utilisateur={cible}",
                parametres(&lecteur, &secret)
            ))
            .to_request(),
    )
    .await;

    let traces: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT geste, cible FROM journal_ops WHERE ops_id = $1")
            .bind(lecteur.id)
            .fetch_all(&pool)
            .await
            .expect("journal");
    assert!(
        traces
            .iter()
            .any(|(g, c)| g == "AUDIT_EXPORT" && c.as_deref() == Some(cible.to_string().as_str())),
        "l'export doit être consigné avec sa cible : {traces:?}"
    );
}

#[actix_web::test]
async fn security_un_mediateur_n_exporte_pas() {
    // Trancher un litige et sortir toutes les données de quelqu'un sont deux
    // gestes de nature différente ; les confondre donnerait à qui arbitre un
    // pouvoir d'extraction qu'il n'a pas besoin d'avoir.
    let pool = pool().await;
    let app = bac!(pool);
    let (mediateur, secret) = ops(&pool, "MEDIATOR", "mediateur-export").await;

    let reponse = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/v1/ops/exports/gdpr?{}&utilisateur={}",
                parametres(&mediateur, &secret),
                Uuid::new_v4()
            ))
            .to_request(),
    )
    .await;

    assert_eq!(reponse.status(), StatusCode::FORBIDDEN);
}
