//! La langue de chacun (FR-043, Story 9.1).
//!
//! **Le défaut que ce module corrige.** La locale vit sur le compte depuis la
//! Story 1.1, mais chaque avis partait en français : les appelants passaient
//! `Locale::Fr` en dur, avec un commentaire disant que lire la vraie langue
//! « demanderait un dépôt de plus ». C'était vrai, et c'était un mauvais
//! arbitrage — un prestataire néerlandophone recevait des notifications en
//! français, dans un pays où c'est précisément le genre de détail qui décide de
//! l'usage.
//!
//! **Le repli est le français, et il est silencieux.** Une langue illisible ou
//! un compte introuvable ne doivent pas empêcher un avis de partir : mieux vaut
//! une notification dans la mauvaise langue que pas de notification du tout.

use klaar_shared_kernel::Locale;
use uuid::Uuid;

use crate::ports::langue::LecteurLangue;

pub use crate::ports::langue::LANGUE_PAR_DEFAUT;

/// Langue d'un compte, avec repli silencieux.
///
/// Simple passe-plat vers le port : il existe pour que les appelants n'aient
/// pas à importer le trait, et pour que ce module reste le seul endroit où l'on
/// se demande « dans quelle langue ».
pub async fn langue_de<L>(lecteur: &L, compte_id: Uuid) -> Locale
where
    L: LecteurLangue,
{
    lecteur.langue_de(compte_id).await
}

/// Interprète une langue demandée, avec repli (FR-043 `@negative`).
///
/// Rend `None` pour une langue que le service ne parle pas — l'appelant décide
/// alors s'il replie ou s'il refuse. FR-043 `@negative` demande un repli en
/// français plutôt qu'un refus : quelqu'un qui demande l'allemand ne doit pas se
/// retrouver devant une erreur, mais devant une application qui marche.
pub fn interpreter(demandee: &str) -> Option<Locale> {
    Locale::parse(demandee).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_les_trois_langues_du_service_sont_reconnues() {
        for code in ["fr", "nl", "en"] {
            assert!(interpreter(code).is_some(), "{code}");
        }
    }

    #[test]
    fn negative_une_langue_non_parlee_n_est_pas_reconnue() {
        // FR-043 `@negative` : c'est l'appelant qui replie, et le dire ici
        // permet de choisir entre repli et refus selon le contexte.
        for code in ["de", "es", "", "français", "FR-BE"] {
            assert_eq!(interpreter(code), None, "{code}");
        }
    }

    #[test]
    fn security_le_repli_est_une_langue_du_service() {
        // Un repli sur une langue inconnue ferait planter la composition des
        // messages, qui indexe des tables par locale.
        assert!(interpreter(LANGUE_PAR_DEFAUT.as_str()).is_some());
    }
}
