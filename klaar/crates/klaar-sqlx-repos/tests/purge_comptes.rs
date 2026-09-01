//! Story 1.1 — purge des comptes jamais vérifiés, contre un vrai PostgreSQL.
//!
//! Un double en mémoire ne dirait rien de ce qui compte ici : que le `DELETE`
//! ne prenne que le statut visé, qu'il respecte la date, qu'il s'arrête au
//! plafond, et que la cascade emporte bien les jetons. Ce sont quatre
//! propriétés de la base, pas du code Rust.
//!
//! Ces cas échouent bruyamment sans `DATABASE_URL` plutôt que d'être ignorés :
//! un test vert parce qu'il n'a rien exécuté est pire que pas de test.
//!
//! **Chaque cas ne compte que ses propres lignes.** La table est partagée et
//! d'autres cas y écrivent ; un comptage global serait faux dès qu'un test
//! tourne en parallèle. D'où le marqueur unique dans chaque adresse, et le
//! nettoyage systématique.

use chrono::{Duration, Utc};
use klaar_application::ports::utilisateur_repository::UtilisateurRepository;
use klaar_sqlx_repos::{creer_pool, PgUtilisateurRepository, PoolPg};
use tokio::sync::Mutex;
use uuid::Uuid;

/// Sérialise les cas de ce fichier.
///
/// **Ils agissent tous sur la même population.** `purger_non_verifies` efface
/// *tous* les comptes non vérifiés plus vieux que le seuil, pas seulement celui
/// que le cas vient de créer : deux cas qui tournent en même temps se
/// détruisent donc mutuellement leurs données. Le symptôme est arrivé en
/// intégration continue, sur une violation de clé étrangère — un jeton inséré
/// pour un compte qu'un cas voisin venait d'effacer entre sa création et son
/// usage.
///
/// Même idiome que `catalogue_routes.rs`, et pour la même raison : un verrou de
/// processus vaut mieux qu'un test qui échoue une fois sur vingt sans qu'on
/// sache pourquoi. Verrou **asynchrone**, le garde traversant des `await`.
static PURGE: Mutex<()> = Mutex::const_new(());

async fn verrou() -> tokio::sync::MutexGuard<'static, ()> {
    PURGE.lock().await
}

fn url() -> String {
    std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI")
}

async fn pool() -> PoolPg {
    creer_pool(&url()).await.expect("connexion PostgreSQL")
}

/// Crée un compte à une date et un statut choisis, et rend son identifiant.
///
/// `cree_le` est passé explicitement : la purge se juge sur l'âge, et attendre
/// soixante-douze heures pour le vérifier n'est pas une option.
async fn compte(pool: &PoolPg, marqueur: &str, statut: &str, age_heures: i64) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO utilisateur (id, email, empreinte_mot_de_passe, statut, locale, cree_le)
         VALUES ($1, $2, '$argon2id$v=19$m=32,t=1,p=1$c2Vsc2Vsc2Vsc2VsMQ$0000000000000000000000000000000000000000000',
                 $3, 'fr', $4)",
    )
    .bind(id)
    .bind(format!("purge-{marqueur}-{id}@example.eu"))
    .bind(statut)
    .bind(Utc::now() - Duration::hours(age_heures))
    .execute(pool)
    .await
    .expect("compte de test");
    id
}

async fn existe(pool: &PoolPg, id: Uuid) -> bool {
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM utilisateur WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("comptage")
        > 0
}

async fn effacer(pool: &PoolPg, ids: &[Uuid]) {
    for id in ids {
        sqlx::query("DELETE FROM utilisateur WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await
            .expect("nettoyage");
    }
}

/// Le seuil utilisé par tous les cas : quarante-huit heures. Les comptes créés
/// « il y a 96 h » sont donc au-delà, ceux « il y a 1 h » en deçà.
fn seuil() -> chrono::DateTime<Utc> {
    Utc::now() - Duration::hours(48)
}

#[tokio::test]
async fn happy_efface_un_compte_non_verifie_plus_vieux_que_le_seuil() {
    let _verrou = verrou().await;
    let pool = pool().await;
    let depot = PgUtilisateurRepository::new(pool.clone());
    let vieux = compte(&pool, "happy", "PENDING_EMAIL_VERIFY", 96).await;

    depot.purger_non_verifies(seuil(), 500).await.unwrap();

    // On juge sur le sort de *ce* compte, jamais sur le nombre total effacé :
    // les cas de ce fichier tournent en parallèle sur une table partagée, et la
    // purge de l'un emporte les lignes de l'autre. Un compteur global serait
    // vert ou rouge selon l'ordonnancement, ce qui n'apprend rien.
    assert!(
        !existe(&pool, vieux).await,
        "le compte visé doit être parti"
    );
}

#[tokio::test]
async fn negative_epargne_un_compte_actif_quel_que_soit_son_age() {
    let _verrou = verrou().await;
    // C'est la propriété qui compte le plus : une erreur de condition ici
    // effacerait des comptes en service, avec leurs demandes en cascade.
    let pool = pool().await;
    let depot = PgUtilisateurRepository::new(pool.clone());
    let actif = compte(&pool, "negative", "ACTIVE", 8760).await;

    depot.purger_non_verifies(seuil(), 500).await.unwrap();

    assert!(
        existe(&pool, actif).await,
        "un compte actif d'un an ne doit jamais être touché par cette purge"
    );
    effacer(&pool, &[actif]).await;
}

#[tokio::test]
async fn edge_epargne_un_compte_non_verifie_plus_recent_que_le_seuil() {
    let _verrou = verrou().await;
    // Quelqu'un qui vient de s'inscrire et n'a pas encore ouvert sa boîte.
    let pool = pool().await;
    let depot = PgUtilisateurRepository::new(pool.clone());
    let recent = compte(&pool, "edge-recent", "PENDING_EMAIL_VERIFY", 1).await;

    depot.purger_non_verifies(seuil(), 500).await.unwrap();

    assert!(
        existe(&pool, recent).await,
        "une inscription d'il y a une heure est en cours, pas abandonnée"
    );
    effacer(&pool, &[recent]).await;
}

#[tokio::test]
async fn edge_le_plafond_arrete_le_passage_et_laisse_le_reliquat() {
    let _verrou = verrou().await;
    let pool = pool().await;
    let depot = PgUtilisateurRepository::new(pool.clone());
    let a = compte(&pool, "edge-plafond-a", "PENDING_EMAIL_VERIFY", 96).await;
    let b = compte(&pool, "edge-plafond-b", "PENDING_EMAIL_VERIFY", 96).await;

    let premier = depot.purger_non_verifies(seuil(), 1).await.unwrap();

    // La propriété garantie par le `LIMIT`, et la seule qui tienne quand
    // d'autres cas purgent la même table en parallèle : jamais plus que le
    // plafond. Sans `LIMIT`, cet appel rendrait le nombre de tous les comptes
    // périmés de la table, ses deux lignes comprises, donc au moins deux.
    assert!(
        premier <= 1,
        "le plafond doit borner le passage, {premier} effacés pour un plafond de 1"
    );

    // Et sans plafond contraignant, les deux partent : le `LIMIT` borne, il ne
    // fait pas disparaître le reliquat.
    depot.purger_non_verifies(seuil(), 500).await.unwrap();
    assert!(!existe(&pool, a).await && !existe(&pool, b).await);
    effacer(&pool, &[a, b]).await;
}

#[tokio::test]
async fn security_la_cascade_emporte_le_jeton_de_verification() {
    let _verrou = verrou().await;
    // Le jeton est l'empreinte d'un secret envoyé par courriel. Effacer le
    // compte en laissant le jeton conserverait la trace d'une inscription que
    // la purge est censée avoir fait disparaître.
    let pool = pool().await;
    let depot = PgUtilisateurRepository::new(pool.clone());
    let id = compte(&pool, "security", "PENDING_EMAIL_VERIFY", 96).await;
    let empreinte = format!("{:0<64}", Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO jeton_verification_email (empreinte, utilisateur_id, expire_le)
         VALUES ($1, $2, now() + interval '1 hour')",
    )
    .bind(&empreinte)
    .bind(id)
    .execute(&pool)
    .await
    .expect("jeton de test");

    depot.purger_non_verifies(seuil(), 500).await.unwrap();

    let jetons: i64 =
        sqlx::query_scalar("SELECT count(*) FROM jeton_verification_email WHERE empreinte = $1")
            .bind(&empreinte)
            .fetch_one(&pool)
            .await
            .expect("comptage");
    assert_eq!(jetons, 0, "le jeton doit partir avec le compte");
}
