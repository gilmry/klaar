//! Ports de persistance des Demandes (FR-011).

use chrono::{DateTime, Utc};
use klaar_catalog::CodeCatalogue;
use klaar_matching::{Demande, StatutDemande};
use klaar_shared_kernel::Geo;
use uuid::Uuid;

use super::erreurs::RepositoryError;

#[allow(async_fn_in_trait)]
pub trait DemandeRepository {
    async fn creer(&self, demande: &Demande) -> Result<(), RepositoryError>;

    async fn par_id(&self, id: Uuid) -> Result<Option<Demande>, RepositoryError>;

    /// Demande identique encore en diffusion, s'il en existe une.
    ///
    /// La recherche vit ici et non dans le domaine parce qu'elle porte sur
    /// l'ensemble des Demandes, que le domaine ne détient pas. La règle du
    /// doublon, elle, reste dans l'agrégat.
    async fn doublon_recent(
        &self,
        demandeur_id: Uuid,
        secteur: &CodeCatalogue,
        position: Geo,
        maintenant: DateTime<Utc>,
    ) -> Result<Option<Demande>, RepositoryError>;

    /// Fait passer une Demande d'un statut à un autre.
    ///
    /// Le statut est décidé par le domaine et écrit ici : ce port ne connaît
    /// pas les transitions permises, il les applique.
    async fn changer_statut(
        &self,
        id: Uuid,
        statut: StatutDemande,
        maintenant: DateTime<Utc>,
    ) -> Result<(), RepositoryError>;

    /// Demandes du compte sur la dernière heure (FR-011 `@edge`).
    async fn compter_depuis_une_heure(
        &self,
        demandeur_id: Uuid,
        maintenant: DateTime<Utc>,
    ) -> Result<i64, RepositoryError>;
}

/// Port de vérification de la méthode de paiement (FR-011, précondition).
///
/// Séparé du dépôt de Demandes : il sera implémenté par l'adaptateur Stripe
/// (Story 1.7), qui n'a rien à voir avec la persistance des Demandes. Le garder
/// distinct évite qu'un dépôt PostgreSQL se retrouve à parler à Stripe.
#[allow(async_fn_in_trait)]
pub trait PaiementRepository {
    async fn possede_methode(&self, utilisateur_id: Uuid) -> Result<bool, RepositoryError>;
}
