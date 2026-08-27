//! Port de persistance des comptes utilisateur (FR-001).

use chrono::{DateTime, Utc};
use klaar_identity::{EmpreinteJeton, Utilisateur};
use klaar_shared_kernel::Email;
use uuid::Uuid;

use super::erreurs::RepositoryError;

/// Jeton de vérification tel qu'il est conservé : jamais en clair, avec sa
/// date d'expiration et sa date de consommation.
#[derive(Debug, Clone)]
pub struct JetonAConserver {
    pub empreinte: EmpreinteJeton,
    pub expire_le: DateTime<Utc>,
}

#[allow(async_fn_in_trait)]
pub trait UtilisateurRepository {
    /// Crée le compte et son jeton, ou ne fait rien si l'adresse est déjà
    /// prise. Retourne `true` si la création a eu lieu.
    ///
    /// Un seul appel, et non « vérifier puis insérer », parce que deux
    /// requêtes concurrentes passeraient toutes les deux la vérification avant
    /// que l'une n'insère (FR-001 `@edge`, double soumission). L'unicité doit
    /// être tranchée par la base, à l'endroit où elle est réellement garantie ;
    /// tout contrôle applicatif préalable n'est qu'une optimisation.
    ///
    /// Le compte et son jeton sont écrits dans la même transaction : un compte
    /// sans jeton est un compte que personne ne peut activer.
    async fn creer_si_absent(
        &self,
        utilisateur: &Utilisateur,
        jeton: &JetonAConserver,
    ) -> Result<bool, RepositoryError>;

    async fn par_email(&self, email: &Email) -> Result<Option<Utilisateur>, RepositoryError>;

    async fn par_id(&self, id: Uuid) -> Result<Option<Utilisateur>, RepositoryError>;
}
