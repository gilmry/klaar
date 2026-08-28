//! Port de persistance des prestataires (FR-003, FR-012).

use klaar_catalog::CodeCatalogue;
use klaar_identity::{NumeroBce, Provider};
use klaar_shared_kernel::Geo;
use uuid::Uuid;

use super::erreurs::RepositoryError;

/// Prestataire trouvé par une recherche géolocalisée, avec sa distance.
///
/// La distance est calculée par la base et rendue telle quelle : la recalculer
/// côté application donnerait deux valeurs légèrement différentes pour la même
/// paire de points, et l'ordre affiché cesserait de correspondre au tri.
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderProche {
    pub provider: Provider,
    pub distance_metres: f64,
}

#[allow(async_fn_in_trait)]
pub trait ProviderRepository {
    async fn creer(&self, provider: &Provider) -> Result<(), RepositoryError>;

    async fn par_id(&self, id: Uuid) -> Result<Option<Provider>, RepositoryError>;

    async fn par_numero_bce(&self, numero: &NumeroBce)
        -> Result<Option<Provider>, RepositoryError>;

    /// Écrit statut, origine du contrôle et disponibilité.
    ///
    /// Séparé de la création pour la même raison que le verrouillage l'est du
    /// compte : activer un prestataire ne doit pas pouvoir écraser sa fiche.
    async fn mettre_a_jour_etat(&self, provider: &Provider) -> Result<(), RepositoryError>;

    async fn definir_disponibilite(
        &self,
        provider_id: Uuid,
        disponible: bool,
    ) -> Result<(), RepositoryError>;

    /// Prestataires actifs, disponibles, couvrant le secteur, dans le rayon.
    ///
    /// Triés par distance croissante. Le tri est fait par la base, qui dispose
    /// de l'index spatial : le faire après coup obligerait à tout rapatrier.
    async fn proches(
        &self,
        secteur: &CodeCatalogue,
        position: Geo,
        rayon_metres: f64,
        limite: i64,
    ) -> Result<Vec<ProviderProche>, RepositoryError>;
}
