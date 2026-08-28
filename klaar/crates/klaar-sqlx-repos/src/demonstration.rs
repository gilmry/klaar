//! Écritures réservées aux jeux de démonstration.
//!
//! **Rien ici n'est destiné à la production.** Ce module existe parce que
//! peupler une base de démonstration demande de créer des comptes déjà actifs,
//! ce qu'aucun chemin normal ne permet — et c'est délibéré : un compte naît
//! `PENDING_EMAIL_VERIFY` et n'en sort qu'en suivant le lien reçu par courriel.
//!
//! Le contournement est donc isolé dans un module qui porte son nom, plutôt que
//! dissimulé sous un paramètre optionnel d'une fonction ordinaire. Il n'est
//! employé que par le binaire `klaar-prestataires-demo`.

use chrono::Utc;
use uuid::Uuid;

use klaar_application::ports::erreurs::RepositoryError;
use klaar_identity::{EmpreinteMotDePasse, StatutUtilisateur};
use klaar_shared_kernel::Email;

use crate::erreur;
use crate::pool::PoolPg;

/// Crée un compte **déjà actif**, ou rend celui qui porte déjà cette adresse.
///
/// Idempotent : relancer la commande de peuplement ne doit ni échouer ni créer
/// de doublon.
pub async fn compte_actif_de_demonstration(
    pool: &PoolPg,
    email: &Email,
    empreinte: &EmpreinteMotDePasse,
) -> Result<Uuid, RepositoryError> {
    sqlx::query(
        "INSERT INTO utilisateur (id, email, empreinte_mot_de_passe, statut, locale, cree_le)
         VALUES ($1, $2, $3, $4, 'fr', $5)
         ON CONFLICT (email) DO NOTHING",
    )
    .bind(Uuid::new_v4())
    .bind(email.as_str())
    .bind(empreinte.as_str())
    .bind(StatutUtilisateur::Actif.as_str())
    .bind(Utc::now())
    .execute(pool)
    .await
    .map_err(erreur)?;

    // Relu plutôt que rendu depuis l'insertion : avec `DO NOTHING`, la ligne
    // conservée peut être celle d'un passage précédent.
    sqlx::query_scalar("SELECT id FROM utilisateur WHERE email = $1")
        .bind(email.as_str())
        .fetch_one(pool)
        .await
        .map_err(erreur)
}

/// Domaine réservé aux comptes de démonstration.
///
/// `.invalid` est réservé par la RFC 2606 : rien ne peut y être livré, et
/// **aucun compte réel ne peut en porter une adresse**. C'est ce qui rend la
/// remise à zéro ci-dessous sûre : son filtre ne peut pas déborder sur des
/// données véritables.
const DOMAINE_DEMONSTRATION: &str = "%@demo.klaar.invalid";

/// Clôt les Missions et les Demandes ouvertes des comptes de démonstration.
///
/// Sert avant un enregistrement de parcours : sans elle, un prestataire resté
/// « occupé » d'une exécution précédente ne recevrait plus rien, et la
/// démonstration s'arrêterait à la première étape sans que la cause soit
/// visible à l'écran.
///
/// **L'historique des transitions n'est pas touché.** Il est append-only, et le
/// réécrire irait contre ce que la Story 4.3 garantit. Une Mission close reste
/// donc en base avec sa trace ; elle ne bloque simplement plus personne.
pub async fn remettre_a_zero(pool: &PoolPg) -> Result<(), RepositoryError> {
    sqlx::query(
        "UPDATE mission SET statut = 'CANCELLED'
         WHERE statut IN ('ACCEPTED', 'PROVIDER_EN_ROUTE', 'ON_SITE')
           AND provider_id IN (
               SELECT p.id FROM provider p
               JOIN utilisateur u ON u.id = p.utilisateur_id
               WHERE u.email LIKE $1
           )",
    )
    .bind(DOMAINE_DEMONSTRATION)
    .execute(pool)
    .await
    .map_err(erreur)?;

    sqlx::query(
        "UPDATE demande SET statut = 'CANCELLED'
         WHERE statut IN ('BROADCASTING', 'NO_MATCH')
           AND demandeur_id IN (SELECT id FROM utilisateur WHERE email LIKE $1)",
    )
    .bind(DOMAINE_DEMONSTRATION)
    .execute(pool)
    .await
    .map_err(erreur)?;

    // Les prestataires reprennent le service : une démonstration précédente a
    // pu en mettre un en pause ou lui donner un rayon serré.
    sqlx::query(
        "UPDATE provider SET disponible = TRUE, rayon_intervention_metres = 20000
         WHERE utilisateur_id IN (SELECT id FROM utilisateur WHERE email LIKE $1)",
    )
    .bind(DOMAINE_DEMONSTRATION)
    .execute(pool)
    .await
    .map_err(erreur)?;

    Ok(())
}
