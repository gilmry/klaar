//! Port des annulations de Mission (FR-022, Story 4.7).
//!
//! **L'atomicité, encore.** La bascule en `CANCELLED`, l'entrée d'historique et
//! la ligne d'annulation forment une seule transaction : une Mission annulée
//! sans ligne laisserait un remboursement dû dont plus rien ne porte la trace,
//! et une ligne sans bascule ferait rembourser deux fois au passage suivant.

use chrono::{DateTime, Utc};
use klaar_intervention::AnnulationMission;
use uuid::Uuid;

use super::erreurs::RepositoryError;

/// Issue d'une tentative d'annulation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultatAnnulation {
    Prononcee(AnnulationMission),
    /// La Mission avait déjà changé d'état entre la lecture et l'écriture.
    MissionDejaClose,
}

#[allow(async_fn_in_trait)]
pub trait AnnulationRepository {
    /// Annule la Mission **et** consigne la décision.
    ///
    /// Le statut de départ sert de garde : deux annulations concurrentes — le
    /// demandeur et le prestataire dans la même seconde — ne doivent pas toutes
    /// deux aboutir.
    async fn prononcer(
        &self,
        annulation: &AnnulationMission,
    ) -> Result<ResultatAnnulation, RepositoryError>;

    /// Désistements de ce prestataire depuis un instant donné (FR-022 `@edge`).
    ///
    /// Compte les annulations qui lui sont imputées, et elles seules : celles
    /// que le demandeur a décidées ne lui coûtent rien.
    async fn desistements_depuis(
        &self,
        provider_id: Uuid,
        depuis: DateTime<Utc>,
    ) -> Result<i64, RepositoryError>;

    /// Annulations décidées par ce demandeur depuis un instant donné.
    async fn annulations_du_demandeur_depuis(
        &self,
        demandeur_id: Uuid,
        depuis: DateTime<Utc>,
    ) -> Result<i64, RepositoryError>;

    /// Annulation d'une Mission, s'il y en a une.
    async fn par_mission(
        &self,
        mission_id: Uuid,
    ) -> Result<Option<AnnulationMission>, RepositoryError>;
}
