//! Ports de persistance des Demandes (FR-011).

use chrono::{DateTime, Utc};
use klaar_catalog::CodeCatalogue;
use klaar_matching::{Demande, MotifAnnulation, StatutDemande};
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

    /// Éteint les Demandes dont le tour de diffusion est écoulé (FR-015).
    ///
    /// Rend celles qui viennent d'être éteintes, et elles seules : c'est ce qui
    /// permet à plusieurs balayages concurrents de tourner sans notifier deux
    /// fois le même demandeur. La sélection et l'écriture sont une seule
    /// opération, pour la même raison que l'attribution l'est.
    ///
    /// `limite` borne un passage : sans elle, un rattrapage après une longue
    /// interruption tenterait de tout traiter d'un coup.
    async fn expirer_echues(
        &self,
        avant: DateTime<Utc>,
        limite: i64,
    ) -> Result<Vec<Demande>, RepositoryError>;

    /// Réécrit rayon, compteur d'élargissements, statut et début de tour.
    ///
    /// Rend `false` si la Demande n'était plus dans l'état depuis lequel
    /// l'élargissement a été calculé. C'est un compare-and-swap sur le
    /// **compteur d'élargissements**, et non sur le statut : le statut ne
    /// suffirait pas, puisqu'une Demande échue mais pas encore balayée est
    /// encore `BROADCASTING` et doit pouvoir être relancée. Le compteur, lui,
    /// distingue toujours deux clics successifs sur « élargir ».
    async fn relancer(&self, demande: &Demande) -> Result<bool, RepositoryError>;

    /// Annule une Demande à la demande de son auteur (FR-014, FR-015).
    ///
    /// Distinct de `changer_statut`, qui ne quitte que `BROADCASTING` : une
    /// annulation part aussi de `NO_MATCH`, et c'est précisément le cas du
    /// quatrième élargissement refusé.
    ///
    /// Rend `false` si la Demande était déjà attribuée : à ce stade, c'est la
    /// Mission qu'il faut annuler (FR-023).
    ///
    /// Le motif est facultatif et pris dans un vocabulaire fermé (FR-014
    /// `@security`) : c'est une information que le demandeur offre, pas une
    /// qu'on lui réclame pour lui rendre un droit.
    async fn annuler(
        &self,
        id: Uuid,
        motif: Option<MotifAnnulation>,
    ) -> Result<bool, RepositoryError>;

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
