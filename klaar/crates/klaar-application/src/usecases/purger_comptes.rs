//! Purge des comptes jamais vérifiés (FR-001 `@security`, minimisation).
//!
//! **Une adresse suffit à créer un compte.** L'inscription n'exige rien
//! d'autre, et c'est voulu : demander davantage avant la vérification
//! reviendrait à collecter avant d'avoir la moindre preuve que la personne
//! existe. La contrepartie est que la table se remplit de comptes que personne
//! n'a confirmés, y compris ceux créés avec l'adresse de gens qui n'ont rien
//! demandé et n'ont fait que recevoir un courriel.
//!
//! Les garder indéfiniment transformerait des tentatives en liste de
//! personnes. C'est exactement ce que la minimisation interdit, et ce n'est pas
//! rattrapé par le fait que le compte soit inactif : une adresse conservée
//! reste une donnée à caractère personnel, active ou non.
//!
//! **Ce balayage efface, il ne désactive pas.** Un compte non vérifié ne porte
//! rien qu'on doive conserver : ni intervention, ni paiement, ni litige. Le
//! marquer « supprimé » garderait l'adresse, donc ne réglerait rien.

use chrono::Duration;

use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::utilisateur_repository::UtilisateurRepository;

/// Délai au terme duquel un compte non vérifié est effacé.
///
/// Soixante-douze heures. Le jeton de vérification, lui, ne vaut qu'une heure :
/// passé ce délai le compte est déjà inutilisable en l'état. Les trois jours ne
/// protègent donc pas le parcours normal, ils laissent la place à un renvoi de
/// lien (Story 1.2) demandé le lendemain ou après un week-end.
///
/// Plus court rendrait ce renvoi impossible pour qui ne relève sa boîte que le
/// lundi. Plus long ne sert personne : au-delà, une réinscription refait le
/// même travail en une requête.
pub const RETENTION_HEURES: i64 = 72;

/// Plafond d'effacements par passage.
///
/// Borne la transaction. Sans lui, le premier balayage sur une base ancienne
/// verrouillerait la table le temps d'écouler tout l'arriéré, et le reste de
/// l'application attendrait. Le reliquat part au passage suivant : le balayage
/// tourne souvent, un retard de quelques minutes sur une purge à trois jours
/// n'a pas de conséquence.
pub const PAR_PASSAGE_MAX: i64 = 500;

/// Efface les comptes restés non vérifiés au-delà du délai de rétention.
///
/// Idempotent : un second passage immédiat ne retrouve rien, la sélection
/// portant sur une date désormais dépassée par les seules lignes déjà parties.
pub async fn purger_les_comptes_non_verifies<U, H>(
    utilisateurs: &U,
    horloge: &H,
) -> Result<u64, RepositoryError>
where
    U: UtilisateurRepository,
    H: Horloge,
{
    let avant = horloge.maintenant() - Duration::hours(RETENTION_HEURES);
    utilisateurs
        .purger_non_verifies(avant, PAR_PASSAGE_MAX)
        .await
}

// Les bornes ci-dessous sont vérifiées à la compilation, et non par des tests.
// Elles ne portent que sur des constantes : un `assert!` d'exécution serait
// optimisé et donnerait un test vert qui n'exécute rien. Ainsi, changer une
// constante hors de ses bornes ne compile plus, ce qui oblige à venir lire les
// raisons écrites plus haut.
//
// Le comportement, lui, est couvert par `klaar-sqlx-repos/tests/purge_comptes`,
// contre un vrai PostgreSQL : c'est là que se jouent le statut épargné, la
// date, le plafond et la cascade.

// Le jeton vaut une heure. Purger avant qu'il ne soit périmé effacerait des
// comptes en cours de vérification, pendant que la personne clique sur son lien.
const _: () = assert!(RETENTION_HEURES > 1);

// Trois jours est une purge. Trois mois serait une conservation sous un autre
// nom, et les raisons écrites plus haut cesseraient d'être vraies.
const _: () = assert!(RETENTION_HEURES <= 7 * 24);

// Assez grand pour écouler un arriéré en quelques passages, assez petit pour
// qu'un passage ne tienne pas la table.
const _: () = assert!(PAR_PASSAGE_MAX >= 100);
const _: () = assert!(PAR_PASSAGE_MAX <= 5_000);
