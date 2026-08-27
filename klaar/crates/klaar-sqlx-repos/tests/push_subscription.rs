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
use klaar_sqlx_repos::{creer_pool, PgPushSubscriptionRepository};
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

async fn depot() -> PgPushSubscriptionRepository {
    PgPushSubscriptionRepository::new(creer_pool(&url()).await.expect("connexion PostgreSQL"))
}

#[tokio::test]
async fn happy_enregistre_puis_retrouve_par_sujet() {
    let depot = depot().await;
    let sujet = Uuid::new_v4();
    let a = abonnement("happy");

    let enregistre = depot.enregistrer(&a, Some(sujet)).await.unwrap();
    assert_eq!(enregistre.abonnement, a);
    assert_eq!(enregistre.sujet_id, Some(sujet));

    let liste = depot.lister_par_sujet(sujet).await.unwrap();
    assert_eq!(liste.len(), 1);
    assert_eq!(liste[0].id, enregistre.id);

    depot.supprimer_par_endpoint(&a.endpoint).await.unwrap();
}

#[tokio::test]
async fn happy_reenregistrer_le_meme_endpoint_met_a_jour_sans_dupliquer() {
    // Un navigateur peut renouveler ses clés en gardant son endpoint. Un
    // second enregistrement doit remplacer, pas ajouter : sinon l'appareil
    // reçoit chaque notification en double.
    let depot = depot().await;
    let sujet = Uuid::new_v4();
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
    let depot = depot().await;
    let sujet = Uuid::new_v4();
    let a = abonnement("coalesce");

    depot.enregistrer(&a, Some(sujet)).await.unwrap();
    let apres = depot.enregistrer(&a, None).await.unwrap();

    assert_eq!(apres.sujet_id, Some(sujet));
    depot.supprimer_par_endpoint(&a.endpoint).await.unwrap();
}

#[tokio::test]
async fn security_la_purge_retire_reellement_la_donnee_personnelle() {
    // Un endpoint identifie un appareil. Quand le service de push le déclare
    // disparu (410), la ligne doit partir : la garder, c'est conserver une
    // donnée personnelle sans finalité.
    let depot = depot().await;
    let sujet = Uuid::new_v4();
    let a = abonnement("purge");

    depot.enregistrer(&a, Some(sujet)).await.unwrap();
    assert!(depot.supprimer_par_endpoint(&a.endpoint).await.unwrap());
    assert!(depot.lister_par_sujet(sujet).await.unwrap().is_empty());
    // Deuxième suppression : idempotente, pas d'erreur.
    assert!(!depot.supprimer_par_endpoint(&a.endpoint).await.unwrap());
}
