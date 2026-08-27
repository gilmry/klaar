//! Port de persistance des comptes utilisateur (FR-001).

use chrono::{DateTime, Utc};
use klaar_identity::{EmpreinteJeton, Utilisateur};
use klaar_shared_kernel::Email;
use uuid::Uuid;

use super::erreurs::RepositoryError;

/// Jeton de vérification tel qu'il est conservé : jamais en clair, avec sa
/// date d'expiration et sa date de consommation.
#[derive(Debug, Clone)]
pub struct JetonAConserver {
    pub empreinte: EmpreinteJeton,
    pub expire_le: DateTime<Utc>,
}

/// Issue d'une présentation de jeton.
///
/// `DejaConsomme` est distingué d'`Inconnu` sans que cela crée un oracle : les
/// deux réponses ne se distinguent qu'en présentant un jeton réel, que seul le
/// titulaire de la boîte a reçu. Les confondre obligerait en revanche à
/// afficher une erreur à quelqu'un dont le compte vient d'être activé et qui
/// recharge la page — le cas le plus banal du parcours.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultatJeton {
    Consomme { utilisateur_id: Uuid },
    DejaConsomme { utilisateur_id: Uuid },
    Expire,
    Inconnu,
}

#[allow(async_fn_in_trait)]
pub trait UtilisateurRepository {
    /// Crée le compte et son jeton, ou ne fait rien si l'adresse est déjà
    /// prise. Retourne `true` si la création a eu lieu.
    ///
    /// Un seul appel, et non « vérifier puis insérer », parce que deux
    /// requêtes concurrentes passeraient toutes les deux la vérification avant
    /// que l'une n'insère (FR-001 `@edge`, double soumission). L'unicité doit
    /// être tranchée par la base, à l'endroit où elle est réellement garantie ;
    /// tout contrôle applicatif préalable n'est qu'une optimisation.
    ///
    /// Le compte et son jeton sont écrits dans la même transaction : un compte
    /// sans jeton est un compte que personne ne peut activer.
    async fn creer_si_absent(
        &self,
        utilisateur: &Utilisateur,
        jeton: &JetonAConserver,
    ) -> Result<bool, RepositoryError>;

    /// Consomme un jeton de vérification et active le compte, en une seule
    /// transaction.
    ///
    /// Les deux écritures sont indissociables : marquer le jeton sans activer
    /// laisserait un compte définitivement inactivable, activer sans marquer
    /// rendrait le lien rejouable, ce que FR-001 interdit.
    ///
    /// Le verrou est pris sur la ligne du jeton, et non sur le compte : deux
    /// clics simultanés sur le même lien doivent aboutir à une seule
    /// consommation, et c'est le jeton qui porte cette unicité.
    async fn consommer_jeton_verification(
        &self,
        empreinte: &EmpreinteJeton,
        maintenant: DateTime<Utc>,
    ) -> Result<ResultatJeton, RepositoryError>;

    async fn par_email(&self, email: &Email) -> Result<Option<Utilisateur>, RepositoryError>;

    async fn par_id(&self, id: Uuid) -> Result<Option<Utilisateur>, RepositoryError>;
}
