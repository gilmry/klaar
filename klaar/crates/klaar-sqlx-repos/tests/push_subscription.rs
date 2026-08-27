//! Story 0.12 — dépôt des abonnements push, contre un vrai PostgreSQL.
//!
//! Ces cas ne sont pas exécutables sans base : `ON CONFLICT`, les contraintes
//! d'unicité et le comptage sont précisément ce qu'un double en mémoire ne
//! reproduirait pas. Ils échouent bruyamment quand `DATABASE_URL` est absente,
//! plutôt que d'être silencieusement ignorés — un test vert parce qu'il n'a
//! rien exécuté est pire que pas de test.
//!
//! Chaque cas nettoie ce qu'il a créé : la table est partagée, et laisser des
//! lignes derrière soi rend le comptage global inutilisable pour les suivants.

use klaar_application::ports::push::PushSubscription;
use klaar_application::ports::push_repository::PushSubscriptionRepository;
use klaar_sqlx_repos::{creer_pool, PgPushSubscriptionRepository, PoolPg};
use uuid::Uuid;

fn url() -> String {
    std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI")
}

/// Un endpoint unique par test : la table est partagée, les cas doivent
/// pouvoir tourner ensemble sans se marcher dessus.
fn abonnement(marqueur: &str) -> PushSubscription {
    PushSubscription {
        endpoint: format!("https://push.example.net/envoi/{marqueur}-{}", Uuid::new_v4()),
        p256dh: "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4".to_string(),
        auth: "BTBZMqHH6r4Tts7J_aSIgg".to_string(),
    }
}

async fn pool() -> PoolPg {
    creer_pool(&url()).await.expect("connexion PostgreSQL")
}

async fn depot() -> PgPushSubscriptionRepository {
    PgPushSubscriptionRepository::new(pool().await)
}

/// Crée un compte réel et rend son identifiant.
///
/// Un UUID tiré au hasard ne suffit plus : la migration V3 a posé la clé
/// étrangère que V2 annonçait, si bien qu'un `sujet_id` sans compte en face
/// est désormais rejeté par la base. C'est exactement ce qu'on veut — un
/// abonnement rattaché à un compte inexistant n'aurait jamais dû être
/// enregistrable.
async fn compte(pool: &PoolPg, marqueur: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO utilisateur (id, email, empreinte_mot_de_passe, statut, locale, cree_le)
         VALUES ($1, $2, '$argon2id$v=19$m=32,t=1,p=1$c2Vsc2Vsc2Vsc2VsMQ$0000000000000000000000000000000000000000000',
                 'PENDING_EMAIL_VERIFY', 'fr', now())",
    )
    .bind(id)
    .bind(format!("push-{marqueur}-{id}@example.eu"))
    .execute(pool)
    .await
    .expect("compte de test");
    id
}

/// Supprime le compte, et par cascade ses abonnements.
async fn effacer_compte(pool: &PoolPg, id: Uuid) {
    sqlx::query("DELETE FROM utilisateur WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .expect("nettoyage");
}

#[tokio::test]
async fn happy_enregistre_puis_retrouve_par_sujet() {
    let pool = pool().await;
    let depot = PgPushSubscriptionRepository::new(pool.clone());
    let sujet = compte(&pool, "happy").await;
    let a = abonnement("happy");

    let enregistre = depot.enregistrer(&a, Some(sujet)).await.unwrap();
    assert_eq!(enregistre.abonnement, a);
    assert_eq!(enregistre.sujet_id, Some(sujet));

    let liste = depot.lister_par_sujet(sujet).await.unwrap();
    assert_eq!(liste.len(), 1);
    assert_eq!(liste[0].id, enregistre.id);

    depot.supprimer_par_endpoint(&a.endpoint).await.unwrap();
    effacer_compte(&pool, sujet).await;
}

#[tokio::test]
async fn happy_reenregistrer_le_meme_endpoint_met_a_jour_sans_dupliquer() {
    // Un navigateur peut renouveler ses clés en gardant son endpoint. Un
    // second enregistrement doit remplacer, pas ajouter : sinon l'appareil
    // reçoit chaque notification en double.
    let pool = pool().await;
    let depot = PgPushSubscriptionRepository::new(pool.clone());
    let sujet = compte(&pool, "maj").await;
    let mut a = abonnement("maj");

    let premier = depot.enregistrer(&a, Some(sujet)).await.unwrap();
    a.p256dh =
        "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8"
            .to_string();
    let second = depot.enregistrer(&a, Some(sujet)).await.unwrap();

    assert_eq!(premier.id, second.id, "la même ligne doit être réutilisée");
    let liste = depot.lister_par_sujet(sujet).await.unwrap();
    assert_eq!(liste.len(), 1, "un seul abonnement pour un endpoint");
    assert_eq!(liste[0].abonnement.p256dh, a.p256dh);

    depot.supprimer_par_endpoint(&a.endpoint).await.unwrap();
    effacer_compte(&pool, sujet).await;
}

#[tokio::test]
async fn negative_supprimer_un_endpoint_inconnu_ne_ment_pas() {
    let depot = depot().await;
    assert!(!depot
        .supprimer_par_endpoint("https://push.example.net/envoi/jamais-vu")
        .await
        .unwrap());
}

#[tokio::test]
async fn edge_un_abonnement_sans_sujet_est_accepte() {
    // Un visiteur peut accepter les notifications avant de se connecter : le
    // rattachement au compte viendra avec l'Epic 1.
    let depot = depot().await;
    let a = abonnement("anonyme");
    let enregistre = depot.enregistrer(&a, None).await.unwrap();
    assert_eq!(enregistre.sujet_id, None);
    assert!(depot.supprimer_par_endpoint(&a.endpoint).await.unwrap());
}

#[tokio::test]
async fn edge_rattacher_un_sujet_ne_l_efface_pas_par_un_enregistrement_anonyme() {
    // COALESCE dans le ON CONFLICT : un ré-enregistrement sans sujet ne doit
    // pas détacher un abonnement déjà rattaché à un compte.
    let pool = pool().await;
    let depot = PgPushSubscriptionRepository::new(pool.clone());
    let sujet = compte(&pool, "coalesce").await;
    let a = abonnement("coalesce");

    depot.enregistrer(&a, Some(sujet)).await.unwrap();
    let apres = depot.enregistrer(&a, None).await.unwrap();

    assert_eq!(apres.sujet_id, Some(sujet));
    depot.supprimer_par_endpoint(&a.endpoint).await.unwrap();
    effacer_compte(&pool, sujet).await;
}

#[tokio::test]
async fn security_la_purge_retire_reellement_la_donnee_personnelle() {
    // Un endpoint identifie un appareil. Quand le service de push le déclare
    // disparu (410), la ligne doit partir : la garder, c'est conserver une
    // donnée personnelle sans finalité.
    let pool = pool().await;
    let depot = PgPushSubscriptionRepository::new(pool.clone());
    let sujet = compte(&pool, "purge").await;
    let a = abonnement("purge");

    depot.enregistrer(&a, Some(sujet)).await.unwrap();
    assert!(depot.supprimer_par_endpoint(&a.endpoint).await.unwrap());
    assert!(depot.lister_par_sujet(sujet).await.unwrap().is_empty());
    // Deuxième suppression : idempotente, pas d'erreur.
    assert!(!depot.supprimer_par_endpoint(&a.endpoint).await.unwrap());
    effacer_compte(&pool, sujet).await;
}

#[tokio::test]
async fn negative_un_sujet_sans_compte_est_refuse() {
    // La contrainte posée par V3 : sans elle, un abonnement pouvait pointer un
    // compte inexistant, et personne ne l'apprenait avant le premier envoi.
    let depot = depot().await;
    let a = abonnement("orphelin");
    let erreur = depot
        .enregistrer(&a, Some(Uuid::new_v4()))
        .await
        .expect_err("un sujet inconnu doit être rejeté");
    assert!(
        matches!(
            erreur,
            klaar_application::ports::erreurs::RepositoryError::Contrainte(_)
        ),
        "une violation de clé étrangère est une contrainte, pas une indisponibilité : {erreur}"
    );
}

#[tokio::test]
async fn security_effacer_le_compte_efface_ses_abonnements() {
    // ON DELETE CASCADE, et non SET NULL : un abonnement orphelin continuerait
    // de recevoir les notifications d'un compte supprimé, ce que le droit à
    // l'effacement interdit.
    let pool = pool().await;
    let depot = PgPushSubscriptionRepository::new(pool.clone());
    let sujet = compte(&pool, "cascade").await;
    let a = abonnement("cascade");

    depot.enregistrer(&a, Some(sujet)).await.unwrap();
    effacer_compte(&pool, sujet).await;

    assert!(
        !depot.supprimer_par_endpoint(&a.endpoint).await.unwrap(),
        "l'abonnement aurait dû disparaître avec le compte"
    );
}
