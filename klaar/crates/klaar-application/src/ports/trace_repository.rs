//! Port de la trace de matching (FR-012, AI Act art. 12).
//!
//! **Écriture seule.** La trace se consigne et se consulte par requête directe
//! quand quelqu'un demande des comptes ; lui donner une API de lecture avant
//! qu'un besoin réel n'existe reviendrait à exposer, sans protection définie,
//! qui a été notifié et pourquoi.

use chrono::{DateTime, Utc};
use klaar_matching::Score;
use uuid::Uuid;

use super::erreurs::RepositoryError;

/// Pourquoi un candidat examiné n'a pas été notifié.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotifEcart {
    /// Classé au-delà des dix retenus.
    HorsTop,
    /// Trouvé hors du rayon du tour en cours.
    HorsRayon,
}

impl MotifEcart {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HorsTop => "HORS_TOP",
            Self::HorsRayon => "HORS_RAYON",
        }
    }
}

/// Une ligne de trace : un candidat examiné pour une Demande.
#[derive(Debug, Clone, PartialEq)]
pub struct LigneTrace {
    pub demande_id: Uuid,
    pub provider_id: Uuid,
    pub score: Score,
    pub distance_metres: f64,
    pub retenu: bool,
    /// Renseigné si et seulement si le candidat n'a pas été retenu.
    pub motif_ecart: Option<MotifEcart>,
    pub tracee_le: DateTime<Utc>,
}

#[allow(async_fn_in_trait)]
pub trait TraceRepository {
    /// Écrit les lignes d'un tour de matching, toutes ou aucune.
    ///
    /// Une trace partielle est pire qu'absente : elle laisse croire que les
    /// candidats manquants n'ont jamais été examinés.
    async fn consigner(&self, lignes: &[LigneTrace]) -> Result<(), RepositoryError>;
}
