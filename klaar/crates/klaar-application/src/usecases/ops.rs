//! Connexion d'exploitation et journal consultable (FR-041, FR-042, Story 8.4).
//!
//! **Deux facteurs, et le second n'est pas optionnel.** FR-041 `@security` le
//! dit sans détour : sans seconde authentification, accès bloqué. Un compte
//! d'exploitation voit les Demandes, les litiges et les montants de tout le
//! monde ; un mot de passe volé ne doit pas suffire.
//!
//! **Le premier accès sert à configurer la MFA, et à rien d'autre.** Un compte
//! neuf reçoit son secret, le scanne, et ne peut rien faire avant de l'avoir
//! prouvé. C'est ce qui évite les comptes d'exploitation « qu'on sécurisera
//! plus tard ».

use chrono::{DateTime, Duration, Utc};
use klaar_identity::{
    verifier_totp, CompteOps, EmpreinteMotDePasse, JetonVerification, MotDePasse, Permission,
    INACTIVITE_MAX_JOURS, TOTP_SECRET_OCTETS,
};
use klaar_shared_kernel::Email;
use std::fmt;
use uuid::Uuid;

use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::ops_repository::{GesteOps, OpsRepository, SessionOps as SessionEnBase};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurOps {
    /// Adresse inconnue, mot de passe faux, ou code faux.
    ///
    /// **Un seul code pour les trois.** Distinguer « cette adresse n'existe
    /// pas » de « le mot de passe est faux » donnerait la liste des comptes
    /// d'exploitation à qui essaie, et c'est exactement la liste qu'on veut
    /// garder pour soi.
    Refuse,
    /// Le compte est désactivé, ou sa seconde authentification n'est pas
    /// configurée.
    Indisponible(String),
    /// Droit manquant pour ce geste.
    Interdit,
    ServiceIndisponible(String),
}

impl ErreurOps {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Refuse => "OPS_CREDENTIALS_INVALID",
            Self::Indisponible(_) => "OPS_UNAVAILABLE",
            Self::Interdit => "FORBIDDEN",
            Self::ServiceIndisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }
}

impl fmt::Display for ErreurOps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refuse => write!(f, "identifiants d'exploitation refusés"),
            Self::Indisponible(d) => write!(f, "{d}"),
            Self::Interdit => write!(f, "ce geste n'est pas dans vos droits"),
            Self::ServiceIndisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurOps {}

impl From<RepositoryError> for ErreurOps {
    fn from(e: RepositoryError) -> Self {
        Self::ServiceIndisponible(e.to_string())
    }
}

/// Ce qu'une connexion d'exploitation rend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionOps {
    pub compte: CompteOps,
}

/// Authentifie un compte d'exploitation : mot de passe **et** code.
///
/// Le code est vérifié après le mot de passe, et les deux échecs rendent le
/// même refus : sans cela, la réponse dirait à qui essaie s'il a trouvé le bon
/// mot de passe, ce qui transformerait la seconde authentification en simple
/// délai.
pub async fn connecter<O, H>(
    comptes: &O,
    horloge: &H,
    email: &Email,
    mot_de_passe: &MotDePasse,
    code_totp: &str,
    parametres: klaar_identity::ParametresArgon2,
) -> Result<SessionOps, ErreurOps>
where
    O: OpsRepository,
    H: Horloge,
{
    let maintenant = horloge.maintenant();

    let Some(compte) = comptes.par_email(email).await? else {
        // **Le calcul est fait quand même**, sur une empreinte factice : sans
        // cela, la durée de la réponse dirait si l'adresse existe.
        let _ = EmpreinteMotDePasse::calculer(mot_de_passe, parametres);
        return Err(ErreurOps::Refuse);
    };

    if !compte.empreinte_mot_de_passe.verifier(mot_de_passe) {
        return Err(ErreurOps::Refuse);
    }

    // L'état du compte est vérifié **après** le mot de passe : dire « ce compte
    // est désactivé » à qui n'a pas le mot de passe renseignerait sur
    // l'existence du compte.
    compte
        .peut_agir(maintenant)
        .map_err(|e| ErreurOps::Indisponible(e.to_string()))?;

    let secret = compte
        .secret_totp
        .as_deref()
        .ok_or_else(|| ErreurOps::Indisponible("MFA_REQUIRED".to_string()))?;
    let verification =
        verifier_totp(secret, code_totp, maintenant.timestamp(), None).ok_or(ErreurOps::Refuse)?;

    // Le pas est consommé en base, pas en mémoire : deux requêtes portant le
    // même code doivent voir la même ligne, et une seule doit passer.
    if !comptes
        .consommer_pas_totp(compte.id, verification.pas, maintenant)
        .await?
    {
        return Err(ErreurOps::Refuse);
    }

    Ok(SessionOps { compte })
}

/// Vérifie un droit et consigne le geste, ensemble.
///
/// **Le journal précède l'action, pas l'inverse.** Une action réussie dont la
/// trace n'a pas pu s'écrire est exactement ce qu'un audit ne peut pas
/// reconstituer ; mieux vaut refuser que d'agir en silence.
pub async fn autoriser_et_consigner<O, H>(
    comptes: &O,
    horloge: &H,
    ops_id: Uuid,
    permission: Permission,
    cible: Option<&str>,
) -> Result<CompteOps, ErreurOps>
where
    O: OpsRepository,
    H: Horloge,
{
    let maintenant = horloge.maintenant();
    let compte = comptes.par_id(ops_id).await?.ok_or(ErreurOps::Refuse)?;

    if !compte.autorise(permission, maintenant) {
        // Le refus est consigné aussi : une tentative d'accès hors droits est
        // précisément ce qu'un journal d'exploitation doit montrer.
        let _ = comptes
            .consigner(&GesteOps {
                ops_id: Some(ops_id),
                geste: format!("{permission}_DENIED"),
                cible: cible.map(str::to_string),
                fait_le: maintenant,
            })
            .await;
        return Err(ErreurOps::Interdit);
    }

    comptes
        .consigner(&GesteOps {
            ops_id: Some(ops_id),
            geste: permission.as_str().to_string(),
            cible: cible.map(str::to_string),
            fait_le: maintenant,
        })
        .await?;

    Ok(compte)
}

/// Secret TOTP tiré au sort, pour un compte qui n'en a pas encore.
///
/// Rend le secret **et** sa forme base32, celle que l'application
/// d'authentification lit. Le secret brut ne sort qu'ici et n'est jamais rendu
/// deux fois : `configurer_totp` refuse d'écraser un secret existant.
pub fn secret_totp_neuf() -> (Vec<u8>, String) {
    use rand::RngCore;
    let mut secret = vec![0u8; TOTP_SECRET_OCTETS];
    rand::rng().fill_bytes(&mut secret);
    let lisible = klaar_identity::base32_totp(&secret);
    (secret, lisible)
}

/// Révoque les comptes d'exploitation inactifs (FR-041 `@edge`).
pub async fn revoquer_les_inactifs<O, H>(comptes: &O, horloge: &H) -> Result<u64, RepositoryError>
where
    O: OpsRepository,
    H: Horloge,
{
    let avant = horloge.maintenant() - Duration::days(INACTIVITE_MAX_JOURS);
    comptes.revoquer_les_inactifs(avant).await
}

/// Page du journal d'exploitation (FR-042 `@happy` : cinquante par page).
pub const JOURNAL_PAR_PAGE: i64 = 50;

/// Lit une page du journal.
pub async fn lire_journal<O>(
    comptes: &O,
    acteur: Option<Uuid>,
    page: i64,
) -> Result<Vec<GesteOps>, ErreurOps>
where
    O: OpsRepository,
{
    // Une page négative ne peut venir que d'un client cassé ; la ramener à la
    // première vaut mieux qu'un décalage négatif que la base refuserait.
    let page = page.max(0);
    Ok(comptes
        .journal(acteur, JOURNAL_PAR_PAGE, page * JOURNAL_PAR_PAGE)
        .await?)
}

/// Instant limite d'inactivité, pour l'affichage.
pub fn echeance_inactivite(derniere_activite: DateTime<Utc>) -> DateTime<Utc> {
    derniere_activite + Duration::days(INACTIVITE_MAX_JOURS)
}

/// Durée d'une session d'exploitation.
///
/// **Trente minutes, sans prolongation.** Une session d'exploitation ouvre des
/// dossiers nominatifs et des décisions sur l'argent d'autrui ; celle qui se
/// renouvelle à chaque clic finit ouverte toute la journée sur un poste
/// partagé. Repasser par le code TOTP toutes les demi-heures est le prix de ces
/// droits-là.
pub const SESSION_MINUTES: i64 = 30;

/// Une session fraîchement ouverte : le jeton en clair, et son échéance.
///
/// Le jeton n'existe qu'ici et dans la réponse HTTP ; rien ne le conserve.
pub struct SessionOuverte {
    pub jeton: String,
    pub expire_le: DateTime<Utc>,
    pub compte: CompteOps,
}

/// Ouvre une session pour un compte déjà authentifié.
///
/// **L'authentification reste ailleurs.** Ce cas d'usage ne vérifie ni mot de
/// passe ni code : il en dépend, et les mêler donnerait deux endroits d'où une
/// session peut naître.
pub async fn ouvrir_session<O, H>(
    comptes: &O,
    horloge: &H,
    compte: CompteOps,
) -> Result<SessionOuverte, ErreurOps>
where
    O: OpsRepository,
    H: Horloge,
{
    let maintenant = horloge.maintenant();
    let expire_le = maintenant + Duration::minutes(SESSION_MINUTES);
    // Trente-deux octets du générateur du système : hors de portée d'une
    // énumération, y compris hors ligne. Seule l'empreinte est écrite.
    let jeton = JetonVerification::tirer();
    comptes
        .ouvrir_session(jeton.empreinte().as_str(), compte.id, maintenant, expire_le)
        .await?;
    Ok(SessionOuverte {
        jeton: jeton.expose().to_string(),
        expire_le,
        compte,
    })
}

/// Retrouve le compte derrière un jeton de session.
///
/// **Un jeton inconnu, expiré ou révoqué donne le même refus qu'un mot de passe
/// faux.** Distinguer « ce jeton n'existe pas » de « ce jeton a expiré »
/// apprendrait à qui essaie qu'il a mis la main sur quelque chose de réel.
pub async fn compte_de_session<O, H>(
    comptes: &O,
    horloge: &H,
    jeton: &str,
) -> Result<CompteOps, ErreurOps>
where
    O: OpsRepository,
    H: Horloge,
{
    let maintenant = horloge.maintenant();
    let empreinte = JetonVerification::depuis_chaine(jeton).empreinte();
    let SessionEnBase { ops_id, .. } = comptes
        .session(empreinte.as_str(), maintenant)
        .await?
        .ok_or(ErreurOps::Refuse)?;
    comptes.par_id(ops_id).await?.ok_or(ErreurOps::Refuse)
}

/// Ferme une session. Idempotent : refermer une session close n'est pas une
/// erreur, c'est le résultat attendu.
pub async fn fermer_session<O, H>(comptes: &O, horloge: &H, jeton: &str) -> Result<(), ErreurOps>
where
    O: OpsRepository,
    H: Horloge,
{
    let empreinte = JetonVerification::depuis_chaine(jeton).empreinte();
    comptes
        .revoquer_session(empreinte.as_str(), horloge.maintenant())
        .await?;
    Ok(())
}
