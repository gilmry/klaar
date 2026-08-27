//! Port de lecture du catalogue (FR-008).
//!
//! Lecture seule. L'administration du catalogue est une story distincte
//! (FR-010, Story 2.4) : lui ouvrir une porte ici avant qu'elle n'existe
//! reviendrait à écrire une API d'écriture que rien n'appelle et que personne
//! ne protège.

use klaar_catalog::Secteur;

use super::erreurs::RepositoryError;

#[allow(async_fn_in_trait)]
pub trait CatalogueRepository {
    /// Tous les secteurs, avec leurs Skills, dans l'ordre d'affichage.
    ///
    /// Rend une liste vide plutôt qu'une erreur si le catalogue n'est pas
    /// amorcé : un catalogue vide est un état de démarrage légitime, pas une
    /// panne (FR-008 `@edge`).
    async fn secteurs(&self) -> Result<Vec<Secteur>, RepositoryError>;
}
