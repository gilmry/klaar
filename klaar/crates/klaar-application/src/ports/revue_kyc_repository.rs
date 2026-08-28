//! Port de la revue KYC (FR-038, Story 8.1).

use chrono::{DateTime, Utc};
use klaar_identity::{RevueKyc, StatutProvider};
use uuid::Uuid;

use super::erreurs::RepositoryError;

/// Une entreprise en attente de contrôle, telle que la console la voit.
///
/// **Elle ne porte que ce qui sert à décider.** Le numéro d'entreprise, la
/// raison sociale, l'ancienneté de la demande. Pas l'adresse du gérant : un
/// écran de revue n'a pas à exposer plus que la question posée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DossierKyc {
    pub provider_id: Uuid,
    pub numero_bce: String,
    pub raison_sociale: String,
    pub secteurs: Vec<String>,
    pub inscrit_le: DateTime<Utc>,
    /// Jours écoulés depuis l'inscription.
    pub attente_jours: i64,
    /// Un refus a été proposé et attend sa seconde paire d'yeux (FR-038
    /// `@edge`).
    ///
    /// **Exposé pour que la file ne fasse pas proposer deux fois.** Sans lui,
    /// un second examinateur croirait le dossier intact et rédigerait un motif
    /// qui ne servirait à rien.
    pub refus_en_attente: Option<RefusEnAttente>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefusEnAttente {
    pub revue_id: Uuid,
    pub motif: String,
    pub propose_par: Option<Uuid>,
    pub propose_le: DateTime<Utc>,
}

#[allow(async_fn_in_trait)]
pub trait RevueKycRepository {
    /// Les entreprises en attente, de la plus ancienne à la plus récente.
    async fn en_attente(&self, limite: i64) -> Result<Vec<DossierKyc>, RepositoryError>;

    /// Le dossier d'une entreprise, avec son statut courant.
    async fn dossier(
        &self,
        provider_id: Uuid,
    ) -> Result<Option<(DossierKyc, StatutProvider)>, RepositoryError>;

    /// Écrit une revue proposée.
    ///
    /// Rend `false` si une revue est déjà en attente pour cette entreprise :
    /// deux refus concurrents laisseraient un doute sur celui qui fait foi.
    async fn proposer(&self, revue: &RevueKyc) -> Result<bool, RepositoryError>;

    /// La revue en attente de confirmation, s'il y en a une.
    async fn en_attente_de_confirmation(
        &self,
        provider_id: Uuid,
    ) -> Result<Option<RevueKyc>, RepositoryError>;

    /// Clôt une revue et applique son effet au prestataire, **dans la même
    /// transaction**.
    ///
    /// Séparer les deux laisserait, en cas de panne entre elles, une revue
    /// confirmée sans effet ou une entreprise refusée sans dossier. Rend `false`
    /// si l'entreprise n'était plus en attente : c'est le compare-and-swap qui
    /// ferme la course entre deux examinateurs.
    async fn clore(
        &self,
        revue: &RevueKyc,
        statut: StatutProvider,
        maintenant: DateTime<Utc>,
    ) -> Result<bool, RepositoryError>;

    /// L'entreprise retire sa demande d'inscription (FR-038 `@edge`).
    ///
    /// Rend `false` si elle n'était plus en attente.
    async fn retirer(&self, provider_id: Uuid) -> Result<bool, RepositoryError>;
}
