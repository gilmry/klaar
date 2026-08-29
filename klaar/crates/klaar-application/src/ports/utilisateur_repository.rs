//! Port de persistance des comptes utilisateur (FR-001).

use chrono::{DateTime, Utc};
use klaar_identity::{EmpreinteJeton, Utilisateur, Verrouillage};
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

    /// Écrit l'état de verrouillage (FR-007).
    ///
    /// Séparé du reste du compte : un échec d'authentification ne doit toucher
    /// que ce compteur. Réécrire l'agrégat entier risquerait d'écraser une
    /// modification concurrente du profil par un chemin qui n'a rien à y voir.
    async fn mettre_a_jour_verrouillage(
        &self,
        utilisateur_id: Uuid,
        verrouillage: &Verrouillage,
    ) -> Result<(), RepositoryError>;

    async fn par_email(&self, email: &Email) -> Result<Option<Utilisateur>, RepositoryError>;

    async fn par_id(&self, id: Uuid) -> Result<Option<Utilisateur>, RepositoryError>;

    /// Change la langue d'un compte (FR-043).
    ///
    /// Rend `false` si le compte n'existe pas. La langue est validée en amont :
    /// ce port reçoit un code que le service parle, pas une chaîne du réseau.
    /// Efface les comptes restés non vérifiés au-delà de `avant`, au plus
    /// `par_passage_max` par appel. Rend le nombre effacé.
    ///
    /// **Ce sont des comptes que personne n'a confirmés.** Une adresse suffit à
    /// en créer un, y compris l'adresse de quelqu'un qui n'a rien demandé et
    /// qui n'a fait que recevoir un courriel de vérification. Les garder
    /// indéfiniment constituerait une liste de personnes à partir de simples
    /// tentatives, ce que la minimisation interdit.
    ///
    /// Le plafond par passage borne la transaction : sans lui, un premier
    /// balayage sur une base ancienne verrouillerait la table le temps
    /// d'effacer tout l'arriéré. Le reliquat part au passage suivant.
    async fn purger_non_verifies(
        &self,
        avant: DateTime<Utc>,
        par_passage_max: i64,
    ) -> Result<u64, RepositoryError>;

    async fn definir_locale(
        &self,
        utilisateur_id: Uuid,
        locale: klaar_shared_kernel::Locale,
    ) -> Result<bool, RepositoryError>;
}

/// Port d'effacement (FR-005).
///
/// Séparé de `UtilisateurRepository` plutôt qu'ajouté dedans : trois cas
/// d'usage sur quatre n'ont rien à faire de l'effacement, et chaque méthode
/// ajoutée au port principal oblige tous leurs doubles de test à la
/// reproduire. La séparation n'est pas de la coquetterie — c'est ce qui garde
/// les tests lisibles à mesure que le domaine grandit.
#[allow(async_fn_in_trait)]
pub trait EffacementRepository {
    /// Programme, ou annule, l'échéance d'effacement.
    async fn programmer_effacement(
        &self,
        utilisateur_id: Uuid,
        efface_le: Option<DateTime<Utc>>,
    ) -> Result<(), RepositoryError>;

    async fn annuler_effacement(&self, utilisateur_id: Uuid) -> Result<(), RepositoryError>;

    /// Comptes dont l'échéance est atteinte.
    async fn effacements_echus(
        &self,
        maintenant: DateTime<Utc>,
    ) -> Result<Vec<Uuid>, RepositoryError>;

    /// Efface les données personnelles d'un compte, **sans supprimer sa ligne**.
    ///
    /// La ligne subsiste pour que le journal d'audit reste rattachable, comme
    /// l'exige le scénario `@security` de FR-005 : la supprimer emporterait par
    /// cascade des entrées qui doivent survivre à l'effacement.
    ///
    /// Rend `true` si **cet appel** a effacé le compte. Deux exécutions
    /// concurrentes du job — deux ordonnanceurs, ou une relance après une
    /// panne — lisent la même liste d'échéances : sans ce retour, elles
    /// écriraient chacune une entrée `USER_ERASED`, et le journal prétendrait
    /// que le droit a été exercé deux fois.
    async fn effacer(
        &self,
        utilisateur_id: Uuid,
        maintenant: DateTime<Utc>,
    ) -> Result<bool, RepositoryError>;
}
