//! Port du suivi de position (FR-019, Story 4.4).

use chrono::{DateTime, Utc};
use klaar_intervention::PositionSuivie;
use uuid::Uuid;

use super::erreurs::RepositoryError;

/// Le trajet, une fois les positions purgées.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrajetAgrege {
    pub distance_metres: f64,
    pub duree_secondes: i64,
    pub releves: i32,
}

#[allow(async_fn_in_trait)]
pub trait SuiviRepository {
    /// Enregistre le consentement au partage pour cette intervention.
    async fn consentir(
        &self,
        mission_id: Uuid,
        maintenant: DateTime<Utc>,
    ) -> Result<(), RepositoryError>;

    /// Retire le consentement. Rend `false` s'il n'y en avait pas.
    async fn retirer_consentement(
        &self,
        mission_id: Uuid,
        maintenant: DateTime<Utc>,
    ) -> Result<bool, RepositoryError>;

    /// Vrai si le partage est consenti et non retiré.
    async fn consenti(&self, mission_id: Uuid) -> Result<bool, RepositoryError>;

    /// Écrit un relevé. La position est **déjà dégradée** par le domaine.
    async fn relever(&self, position: &PositionSuivie) -> Result<(), RepositoryError>;

    /// Dernière position connue, s'il y en a une.
    async fn derniere(&self, mission_id: Uuid) -> Result<Option<PositionSuivie>, RepositoryError>;

    /// Calcule le trajet agrégé **puis** supprime les positions (FR-019).
    ///
    /// Les deux dans la même transaction : agréger sans supprimer laisserait la
    /// trace fine, supprimer sans agréger perdrait la mesure. Rend le nombre
    /// d'interventions purgées.
    async fn purger_les_echues(
        &self,
        avant: DateTime<Utc>,
        limite: i64,
    ) -> Result<u64, RepositoryError>;

    /// Trajet agrégé d'une intervention, une fois purgée.
    async fn trajet(&self, mission_id: Uuid) -> Result<Option<TrajetAgrege>, RepositoryError>;
}
