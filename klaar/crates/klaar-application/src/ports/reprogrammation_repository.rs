//! Port de la reprogrammation (FR-023, Story 4.8).

use chrono::{DateTime, Utc};
use klaar_intervention::{AuteurAnnulation, Reprogrammation};
use uuid::Uuid;

use super::erreurs::RepositoryError;

/// Ce qu'il faut savoir avant de proposer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContexteReprogrammation {
    pub demandeur_id: Uuid,
    pub provider_id: Uuid,
    /// Qui a annulé, et quand. `None` si la Mission n'a pas été annulée.
    pub annulation: Option<(AuteurAnnulation, DateTime<Utc>)>,
    /// Le devis accepté dont le prix serait repris.
    pub devis_accepte: Option<Uuid>,
}

/// Issue d'une acceptation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultatAcceptation {
    /// Une nouvelle intervention est née, avec son devis repris.
    Reprise { nouvelle_mission: Uuid },
    /// La proposition avait déjà bougé.
    DejaClose,
    /// Le prestataire porte déjà une autre intervention en cours.
    ProviderOccupe,
}

#[allow(async_fn_in_trait)]
pub trait ReprogrammationRepository {
    async fn contexte(
        &self,
        mission_id: Uuid,
    ) -> Result<Option<ContexteReprogrammation>, RepositoryError>;

    /// Écrit la proposition. Rend `false` s'il en existe déjà une.
    async fn proposer(&self, proposition: &Reprogrammation) -> Result<bool, RepositoryError>;

    async fn par_mission(
        &self,
        mission_id: Uuid,
    ) -> Result<Option<Reprogrammation>, RepositoryError>;

    /// Accepte : crée la nouvelle intervention **et** recopie le devis, dans
    /// une seule transaction.
    ///
    /// Une intervention née sans son devis laisserait le prestataire travailler
    /// sans que rien ne dise à quel prix, et la validation ultérieure n'aurait
    /// rien à libérer.
    async fn accepter(
        &self,
        mission_id: Uuid,
        maintenant: DateTime<Utc>,
    ) -> Result<ResultatAcceptation, RepositoryError>;

    /// Décline. Rend `false` si la proposition avait déjà bougé.
    async fn refuser(&self, mission_id: Uuid) -> Result<bool, RepositoryError>;
}
