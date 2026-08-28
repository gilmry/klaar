//! Port de la conversation (FR-030, FR-032, Story 6.1).

use chrono::{DateTime, Utc};
use klaar_messaging::Message;
use uuid::Uuid;

use super::erreurs::RepositoryError;

/// Ce qu'il faut savoir d'une conversation avant d'y écrire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EtatConversation {
    /// Messages déjà écrits, pour la limite de cent.
    pub deja_ecrits: i64,
    /// Instant de fin de l'intervention, quand elle est close. `None` tant
    /// qu'elle est en cours : la conversation reste alors ouverte.
    pub close_depuis: Option<DateTime<Utc>>,
}

#[allow(async_fn_in_trait)]
pub trait MessageRepository {
    async fn ecrire(&self, message: &Message) -> Result<(), RepositoryError>;

    /// Le fil, du plus ancien au plus récent.
    ///
    /// Pas de pagination : la conversation est bornée à cent messages, et
    /// paginer cent lignes serait de la cérémonie. La borne est ce qui rend ce
    /// choix tenable, et c'est écrit dans le domaine.
    async fn fil(&self, mission_id: Uuid) -> Result<Vec<Message>, RepositoryError>;

    async fn etat(&self, mission_id: Uuid) -> Result<EtatConversation, RepositoryError>;

    /// Consigne une tentative d'échange de coordonnées (FR-032 `@security`).
    ///
    /// **Le message refusé n'est pas passé.** Garder le texte reviendrait à
    /// constituer un fichier de ce que les gens ont essayé de s'écrire, pour
    /// une finalité qui n'en a pas besoin.
    async fn consigner_tentative(
        &self,
        mission_id: Uuid,
        auteur_id: Uuid,
        genre: &str,
        tentee_le: DateTime<Utc>,
    ) -> Result<(), RepositoryError>;

    /// Tentatives de ce compte depuis un instant donné.
    async fn tentatives_depuis(
        &self,
        auteur_id: Uuid,
        depuis: DateTime<Utc>,
    ) -> Result<i64, RepositoryError>;
}
