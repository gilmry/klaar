//! Port de persistance des sessions de rafraîchissement (FR-004).

use chrono::{DateTime, Utc};
use klaar_identity::EmpreinteJeton;
use uuid::Uuid;

use super::erreurs::RepositoryError;

/// Durée de vie du refresh, fixée par FR-004.
pub const VALIDITE_REFRESH_JOURS: i64 = 30;

/// Session à écrire.
///
/// `famille_id` relie tous les refresh issus d'une même authentification. La
/// rotation en crée un nouveau à chaque usage ; c'est la famille qui permet, à
/// la détection d'un rejeu, de couper la chaîne entière plutôt que le seul
/// jeton rejoué — sans quoi le voleur garde le sien.
#[derive(Debug, Clone)]
pub struct SessionAConserver {
    pub empreinte: EmpreinteJeton,
    pub utilisateur_id: Uuid,
    pub famille_id: Uuid,
    pub expire_le: DateTime<Utc>,
    /// Empreinte du contexte d'obtention (agent utilisateur), quand il est
    /// connu. `None` n'est pas une anomalie : une session sans contexte
    /// enregistré ne peut simplement pas être comparée.
    pub empreinte_contexte: Option<EmpreinteJeton>,
}

/// Issue d'une présentation de refresh.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultatRotation {
    Rotationne {
        utilisateur_id: Uuid,
        famille_id: Uuid,
        /// Le refresh est présenté depuis un contexte différent de celui qui
        /// l'a obtenu. Signalé, pas bloquant : voir `rafraichir`.
        contexte_change: bool,
    },
    /// Refresh déjà consommé. C'est la signature d'un vol : le porteur
    /// légitime a reçu son remplaçant à la rotation précédente et n'a aucune
    /// raison de présenter l'ancien.
    Rejeu {
        famille_id: Uuid,
        utilisateur_id: Uuid,
    },
    Expire,
    Revoque,
    Inconnu,
}

#[allow(async_fn_in_trait)]
pub trait SessionRepository {
    async fn ouvrir(&self, session: &SessionAConserver) -> Result<(), RepositoryError>;

    /// Consomme le refresh présenté et en ouvre un nouveau dans la même
    /// famille, en une seule transaction.
    ///
    /// Les deux écritures sont indissociables : consommer sans remplacer
    /// déconnecte quelqu'un qui n'a rien fait de mal, remplacer sans consommer
    /// laisse deux refresh valables et rend le rejeu indétectable.
    async fn rotationner(
        &self,
        presentee: &EmpreinteJeton,
        nouvelle: &EmpreinteJeton,
        contexte: Option<&EmpreinteJeton>,
        expire_le: DateTime<Utc>,
        maintenant: DateTime<Utc>,
    ) -> Result<ResultatRotation, RepositoryError>;

    /// Famille d'un refresh, consommé ou non. Sert à couper la chaîne après un
    /// rejeu, et à déconnecter.
    async fn famille_de(&self, empreinte: &EmpreinteJeton)
        -> Result<Option<Uuid>, RepositoryError>;

    /// Révoque toutes les sessions d'un compte. Rend le nombre de sessions
    /// effectivement coupées.
    async fn revoquer_famille(
        &self,
        famille_id: Uuid,
        maintenant: DateTime<Utc>,
    ) -> Result<u64, RepositoryError>;
}
