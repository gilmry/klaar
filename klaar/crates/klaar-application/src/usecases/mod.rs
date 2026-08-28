//! Cas d'usage : orchestration des ports et du domaine, sans logique métier
//! propre. Chacun est tracé sur un FR du PRD.

pub mod accepter;
pub mod connecter;
pub mod effacer;
pub mod elargir;
pub mod expirer;
pub mod inscrire_utilisateur;
pub mod matcher;
pub mod notifier;
pub mod rafraichir;
pub mod soumettre_demande;
pub mod verifier_email;
