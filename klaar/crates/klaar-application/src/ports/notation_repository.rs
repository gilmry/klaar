//! Port de la notation double sens (FR-033, FR-037, Story 7.1).
//!
//! **L'unicité est dans la base, pas ici.** FR-033 `@security` le demande
//! explicitement : « la contrainte unique est en base, la tentative de double
//! est techniquement impossible ». Lire puis écrire laisserait deux clics
//! rapides poser deux notes.

use chrono::{DateTime, Utc};
use klaar_trust::{Cible, Notation};
use uuid::Uuid;

use super::erreurs::RepositoryError;

/// Issue d'une tentative de notation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultatNotation {
    Ecrite(Notation),
    /// Ce côté a déjà noté cette intervention (FR-033 `@edge`).
    DejaNotee,
}

/// Ce qu'une intervention porte comme notes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NotesDeMission {
    pub sur_le_prestataire: Option<Notation>,
    pub sur_le_demandeur: Option<Notation>,
}

impl NotesDeMission {
    pub fn les_deux_presentes(&self) -> bool {
        self.sur_le_prestataire.is_some() && self.sur_le_demandeur.is_some()
    }

    pub fn pour(&self, cible: Cible) -> Option<&Notation> {
        match cible {
            Cible::Prestataire => self.sur_le_prestataire.as_ref(),
            Cible::Demandeur => self.sur_le_demandeur.as_ref(),
        }
    }
}

/// La réputation agrégée d'un prestataire, telle qu'elle est conservée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Reputation {
    pub somme_notes: u32,
    pub nombre_notes: u32,
}

#[allow(async_fn_in_trait)]
pub trait NotationRepository {
    /// Écrit la note **et** met à jour l'agrégat du prestataire, ensemble.
    ///
    /// Une seule transaction : une note écrite sans agrégat à jour ferait
    /// diverger la réputation affichée de celle qui classe, et personne ne s'en
    /// apercevrait avant le prochain recalcul complet.
    async fn noter(&self, notation: &Notation) -> Result<ResultatNotation, RepositoryError>;

    async fn notes_de_mission(&self, mission_id: Uuid) -> Result<NotesDeMission, RepositoryError>;

    /// Réputation agrégée. Rend zéro partout quand personne n'a encore noté.
    async fn reputation(&self, provider_id: Uuid) -> Result<Reputation, RepositoryError>;

    /// Réputations de plusieurs prestataires en une requête.
    ///
    /// **Une requête et pas une par candidat.** Le matching classe jusqu'à dix
    /// prestataires par tour, et un aller-retour chacun mettrait dix latences
    /// réseau dans un chemin censé répondre en quelques millisecondes. Les
    /// absents ne figurent pas dans la réponse : personne ne les a notés.
    async fn reputations_de(
        &self,
        provider_ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, Reputation>, RepositoryError>;

    /// Instant de validation de la Mission, d'où court la fenêtre de notation.
    ///
    /// Rend `None` si la Mission n'est pas validée : on ne note pas une
    /// intervention dont personne n'a dit qu'elle était finie.
    async fn validee_le(&self, mission_id: Uuid) -> Result<Option<DateTime<Utc>>, RepositoryError>;
}
