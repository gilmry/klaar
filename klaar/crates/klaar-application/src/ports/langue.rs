//! Port étroit : la langue d'un compte (FR-043, Story 9.1).
//!
//! **Pourquoi un port pour une seule question.** Composer un avis a besoin de
//! savoir dans quelle langue l'écrire, et de rien d'autre du compte. Faire
//! dépendre le notifieur du dépôt d'utilisateurs entier l'aurait couplé à
//! l'inscription, à la vérification d'email et au verrouillage — et aurait
//! obligé chaque test de notification à doubler neuf méthodes pour en utiliser
//! une.
//!
//! L'implémentation générale ci-dessous fait qu'un `UtilisateurRepository`
//! satisfait ce port sans rien écrire : le service branche son dépôt, les
//! tests branchent une table.

use klaar_shared_kernel::Locale;
use uuid::Uuid;

use super::utilisateur_repository::UtilisateurRepository;

/// Langue par défaut quand rien ne peut être lu.
///
/// Le français plutôt que le néerlandais : la Région bruxelloise est
/// majoritairement francophone, et un repli doit se tromper le moins souvent
/// possible. Ce n'est pas une préférence, c'est une statistique.
pub const LANGUE_PAR_DEFAUT: Locale = Locale::Fr;

#[allow(async_fn_in_trait)]
pub trait LecteurLangue {
    /// **Ne rend jamais d'erreur.** L'appelant est un chemin de notification, et
    /// faire échouer l'envoi parce que la langue n'a pas pu être lue serait
    /// remplacer un désagrément par une perte.
    async fn langue_de(&self, compte_id: Uuid) -> Locale;
}

impl<U> LecteurLangue for U
where
    U: UtilisateurRepository,
{
    async fn langue_de(&self, compte_id: Uuid) -> Locale {
        match self.par_id(compte_id).await {
            Ok(Some(u)) => u.locale,
            Ok(None) => LANGUE_PAR_DEFAUT,
            Err(e) => {
                tracing::warn!(erreur = %e, "langue du compte illisible, repli sur le défaut");
                LANGUE_PAR_DEFAUT
            }
        }
    }
}
