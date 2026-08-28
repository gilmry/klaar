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
