//! Cas d'usage : orchestration des ports et du domaine, sans logique métier
//! propre. Chacun est tracé sur un FR du PRD.

pub mod accepter;
pub mod annuler;
pub mod annuler_mission;
pub mod connecter;
pub mod consulter;
pub mod converser;
pub mod disponibilite;
pub mod effacer;
pub mod elargir;
pub mod emettre_devis;
pub mod expirer;
pub mod expirer_devis;
pub mod inscrire_utilisateur;
pub mod langue;
pub mod matcher;
pub mod noter;
pub mod notifier;
pub mod ops;
pub mod ouvrir_litige;
pub mod rafraichir;
pub mod repondre_devis;
pub mod reprogrammer;
pub mod soumettre_demande;
pub mod transiter_mission;
pub mod valider_mission;
pub mod verifier_email;
