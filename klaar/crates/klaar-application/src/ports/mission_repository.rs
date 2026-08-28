//! Port d'attribution d'une Demande (FR-013, Story 3.4).
//!
//! **L'atomicité est la raison d'être de ce port.** Cinq prestataires reçoivent
//! la même notification et peuvent toucher « accepter » dans la même seconde.
//! Lire le statut puis l'écrire laisserait deux d'entre eux passer, et deux
//! camionnettes partiraient pour une seule fuite. La garantie n'est donc pas
//! dans le cas d'usage mais ici, dans une opération que la base sérialise :
//! `UPDATE … WHERE statut = 'BROADCASTING' RETURNING …`, dont au plus un
//! appelant voit une ligne.
//!
//! Même chose pour « une Mission à la fois » : un contrôle préalable serait
//! contournable par deux acceptations simultanées. C'est un index unique
//! partiel qui le tient, et l'erreur qu'il produit qui remonte ici.

use chrono::{DateTime, Utc};
use klaar_intervention::{Mission, StatutMission, TransitionMission};
use uuid::Uuid;

use super::erreurs::RepositoryError;

/// Issue d'une tentative d'attribution.
///
/// Trois cas distincts et non un `Option` : « la Demande n'est plus à prendre »
/// et « ce prestataire est déjà occupé » appellent des réponses différentes, et
/// les confondre dirait au prestataire d'aller voir ailleurs alors que c'est sa
/// propre Mission en cours qui le bloque.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultatAttribution {
    Attribuee(Mission),
    /// La Demande n'était plus en diffusion : un autre l'a prise, elle a été
    /// annulée, ou aucun candidat n'avait répondu.
    DemandeNonDiffusee,
    /// Le prestataire porte déjà une Mission en cours (FR-013 `@edge`).
    ProviderOccupe,
}

#[allow(async_fn_in_trait)]
pub trait MissionRepository {
    /// Attribue la Demande au prestataire, ou n'en fait rien.
    ///
    /// Le passage de la Demande en `MATCHED` et la création de la Mission
    /// forment une seule transaction : une Demande attribuée sans Mission
    /// laisserait le demandeur devant un statut qui promet une intervention
    /// dont personne ne porte la trace.
    async fn attribuer(
        &self,
        demande_id: Uuid,
        provider_id: Uuid,
        maintenant: DateTime<Utc>,
    ) -> Result<ResultatAttribution, RepositoryError>;

    /// Mission en cours du prestataire, s'il en a une.
    async fn en_cours_pour(&self, provider_id: Uuid) -> Result<Option<Mission>, RepositoryError>;

    async fn par_id(&self, id: Uuid) -> Result<Option<Mission>, RepositoryError>;

    /// Mission née d'une Demande, s'il y en a une.
    ///
    /// Une Demande donne au plus une Mission : c'est ce que garantit
    /// l'attribution atomique, et une contrainte d'unicité le grave.
    async fn par_demande(&self, demande_id: Uuid) -> Result<Option<Mission>, RepositoryError>;

    /// Écrit le nouveau statut **et** consigne la transition, ensemble.
    ///
    /// Une seule transaction : un statut changé sans entrée d'historique
    /// laisserait une Mission avancée dont plus rien ne dit quand ni d'où, et
    /// c'est exactement ce que FR-018 `@security` demande de pouvoir produire.
    ///
    /// Le statut de départ est passé en garde : deux transitions concurrentes
    /// depuis le même état ne doivent pas toutes deux aboutir, sinon
    /// l'historique porterait deux entrées pour un seul changement. Rend
    /// `false` quand la Mission avait déjà bougé.
    async fn transiter(
        &self,
        mission_id: Uuid,
        depuis: StatutMission,
        entree: &TransitionMission,
    ) -> Result<bool, RepositoryError>;
}
