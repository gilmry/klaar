//! Port de persistance des abonnements push (Story 0.12).

use std::fmt;
use uuid::Uuid;

use super::push::PushSubscription;

#[derive(Debug)]
pub enum RepositoryError {
    Indisponible(String),
    Contrainte(String),
}

impl fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Indisponible(d) => write!(f, "dépôt indisponible : {d}"),
            Self::Contrainte(d) => write!(f, "contrainte violée : {d}"),
        }
    }
}

impl std::error::Error for RepositoryError {}

/// Abonnement tel qu'il est conservé, avec son identité propre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbonnementEnregistre {
    pub id: Uuid,
    pub abonnement: PushSubscription,
    pub sujet_id: Option<Uuid>,
}

/// Dépôt d'abonnements push.
///
/// `supprimer_par_endpoint` n'est pas un confort : quand un service de push
/// répond 410, garder la ligne signifie conserver une donnée personnelle
/// devenue sans finalité, ce que le RGPD interdit, et réessayer sans fin.
#[allow(async_fn_in_trait)]
pub trait PushSubscriptionRepository {
    /// Enregistre un abonnement, ou met à jour ses clés s'il existe déjà.
    ///
    /// Un navigateur peut renouveler ses clés en conservant son endpoint :
    /// insérer en double créerait deux notifications pour un seul appareil.
    async fn enregistrer(
        &self,
        abonnement: &PushSubscription,
        sujet_id: Option<Uuid>,
    ) -> Result<AbonnementEnregistre, RepositoryError>;

    async fn lister_par_sujet(
        &self,
        sujet_id: Uuid,
    ) -> Result<Vec<AbonnementEnregistre>, RepositoryError>;

    /// Supprime un abonnement. Retourne `true` s'il existait.
    async fn supprimer_par_endpoint(&self, endpoint: &str) -> Result<bool, RepositoryError>;

    async fn compter(&self) -> Result<i64, RepositoryError>;
}
