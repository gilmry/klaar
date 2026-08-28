//! Port du journal des webhooks Stripe (FR-028, Story 5.5).

use chrono::{DateTime, Utc};
use klaar_stripe_adapter::{Evenement, Suite};

use super::erreurs::RepositoryError;

/// Ce que la base répond quand on tente de consigner un événement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consignation {
    /// Écrit : c'est la première fois. L'effet peut être appliqué.
    Neuf,
    /// Cet identifiant existait déjà.
    ///
    /// **C'est la base qui le dit, pas une lecture préalable.** Deux réceptions
    /// simultanées du même événement s'écraseraient l'une l'autre sur un
    /// « lire puis décider » ; l'insertion tranche.
    DejaVu,
}

#[allow(async_fn_in_trait)]
pub trait EvenementStripeRepository {
    /// Horodatage Stripe du dernier événement **appliqué** à cet objet.
    ///
    /// `None` s'il n'y en a pas : c'est alors le premier, et rien ne le dépasse.
    async fn dernier_applique(
        &self,
        objet_id: &str,
    ) -> Result<Option<DateTime<Utc>>, RepositoryError>;

    /// Consigne l'événement et la suite qui lui a été donnée.
    async fn consigner(
        &self,
        evenement: &Evenement,
        suite: Suite,
        recu_le: DateTime<Utc>,
    ) -> Result<Consignation, RepositoryError>;
}
