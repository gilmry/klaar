//! Port de la trace de matching (FR-012, AI Act art. 12).
//!
//! **Pas d'API de lecture générale.** La trace se consulte par requête directe
//! quand quelqu'un demande des comptes ; lui donner une API de lecture avant
//! qu'un besoin réel n'existe reviendrait à exposer, sans protection définie,
//! qui a été notifié et pourquoi.
//!
//! La seule lecture ouverte ici sert à prévenir les candidats qu'une Demande
//! vient d'être prise (FR-013 `@happy`). Elle ne rend que des identifiants de
//! comptes déjà notifiés, jamais les scores ni les motifs d'écart, et elle sert
//! précisément les personnes concernées : celles qui attendent une réponse.

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

    /// Comptes des prestataires retenus pour cette Demande, sauf un.
    ///
    /// Ce sont les gens à prévenir quand la Demande est prise par un autre. Le
    /// prestataire exclu est celui qui vient de l'accepter : lui envoyer
    /// « déjà prise » serait absurde.
    ///
    /// Rend des identifiants de **comptes** et non de prestataires : ce sont
    /// les comptes qui portent les abonnements push.
    async fn comptes_retenus_sauf(
        &self,
        demande_id: Uuid,
        sauf_provider_id: Uuid,
    ) -> Result<Vec<Uuid>, RepositoryError>;
}
