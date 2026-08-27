//! Port de journal d'audit (FR-001, exigence NIS2 de traçabilité).

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::erreurs::RepositoryError;

/// Événement consigné. Le vocabulaire est figé et anglophone parce qu'il sort
/// du code : ces codes se retrouvent dans des exports, des tableaux de bord et
/// des réquisitions, où les renommer coûte plus que la cohérence gagnée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeAudit {
    UserSignup,
    UserSignupDuplicate,
    UserEmailVerified,
    UserLogin,
    UserLoginFailed,
    UserLogout,
    SessionRefreshed,
    /// Refresh rejoué : signature d'un vol, famille coupée.
    SessionReuseDetected,
    /// Refresh présenté depuis un autre contexte. Signalé, pas bloquant.
    SessionContextChanged,
    /// Compte verrouillé après échecs répétés (FR-007).
    AccountLocked,
    /// Effacement demandé, exécution différée (FR-005).
    UserErasureRequested,
    UserErasureCancelled,
    /// Effacement exécuté. Conservé : c'est la trace que le droit a été honoré.
    UserErased,
}

impl CodeAudit {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UserSignup => "USER_SIGNUP",
            Self::UserSignupDuplicate => "USER_SIGNUP_DUPLICATE",
            Self::UserEmailVerified => "USER_EMAIL_VERIFIED",
            Self::UserLogin => "USER_LOGIN",
            Self::UserLoginFailed => "USER_LOGIN_FAILED",
            Self::UserLogout => "USER_LOGOUT",
            Self::SessionRefreshed => "SESSION_REFRESHED",
            Self::SessionReuseDetected => "SESSION_REUSE_DETECTED",
            Self::SessionContextChanged => "SESSION_CONTEXT_CHANGED",
            Self::AccountLocked => "ACCOUNT_LOCKED",
            Self::UserErasureRequested => "USER_ERASURE_REQUESTED",
            Self::UserErasureCancelled => "USER_ERASURE_CANCELLED",
            Self::UserErased => "USER_ERASED",
        }
    }
}

/// Entrée du journal.
///
/// Volontairement sans adresse IP ni agent utilisateur. Le raisonnement est le
/// même que pour les journaux applicatifs (voir `COMPLIANCE.md`) : ces données
/// sont personnelles, leur conservation demande une finalité et une durée que
/// rien n'a encore établies ici. La limite est réelle et assumée — un audit de
/// sécurité complet voudra l'origine des tentatives. La borne d'abus est tenue
/// séparément, en mémoire, par la limitation de débit.
#[derive(Debug, Clone)]
pub struct EntreeAudit {
    pub code: CodeAudit,
    /// Compte concerné, quand il existe. Absent lorsqu'une tentative porte sur
    /// une adresse déjà prise : y mettre l'identifiant du titulaire ferait du
    /// journal d'audit l'oracle que le reste du code s'applique à éviter.
    pub sujet_id: Option<Uuid>,
    pub horodatage: DateTime<Utc>,
}

#[allow(async_fn_in_trait)]
pub trait JournalAudit {
    async fn consigner(&self, entree: EntreeAudit) -> Result<(), RepositoryError>;
}
