//! Agrégat `Utilisateur` (FR-001, FR-004).

use chrono::{DateTime, Duration, Utc};
use klaar_shared_kernel::{Email, Locale};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use crate::jeton_verification::{EmpreinteJeton, JetonVerification, VALIDITE_HEURES};
use crate::mot_de_passe::EmpreinteMotDePasse;
use crate::verrouillage::Verrouillage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatutUtilisateur {
    /// Compte créé, adresse non encore prouvée. Ne donne accès à rien.
    EnAttenteVerificationEmail,
    Actif,
}

impl StatutUtilisateur {
    /// Valeur écrite en base et exposée dans l'API. Figée : la renommer
    /// invalide les lignes déjà enregistrées.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::EnAttenteVerificationEmail => "PENDING_EMAIL_VERIFY",
            Self::Actif => "ACTIVE",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "PENDING_EMAIL_VERIFY" => Some(Self::EnAttenteVerificationEmail),
            "ACTIVE" => Some(Self::Actif),
            _ => None,
        }
    }
}

impl fmt::Display for StatutUtilisateur {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Utilisateur {
    pub id: Uuid,
    pub email: Email,
    pub empreinte_mot_de_passe: EmpreinteMotDePasse,
    pub statut: StatutUtilisateur,
    pub locale: Locale,
    pub cree_le: DateTime<Utc>,
    /// Compteur d'échecs et verrou éventuel (FR-007).
    pub verrouillage: Verrouillage,
}

/// Jeton fraîchement émis : la valeur en clair à envoyer, et ce qu'il faut
/// conserver. Les deux ne partent pas au même endroit, d'où le type qui les
/// sépare.
#[derive(Debug)]
pub struct JetonEmis {
    /// À placer dans le lien de l'email. Jamais conservé.
    pub en_clair: JetonVerification,
    /// À écrire en base.
    pub empreinte: EmpreinteJeton,
    pub expire_le: DateTime<Utc>,
}

impl Utilisateur {
    /// Crée un compte non vérifié.
    ///
    /// L'horodatage est passé en argument plutôt que lu par `Utc::now()` : un
    /// domaine qui lit l'horloge lui-même ne se teste plus sur ses propres
    /// expirations sans attendre réellement.
    pub fn inscrire(
        email: Email,
        empreinte_mot_de_passe: EmpreinteMotDePasse,
        locale: Locale,
        maintenant: DateTime<Utc>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            email,
            empreinte_mot_de_passe,
            // Aucun paramètre ne permet de créer un compte directement actif :
            // c'est l'invariant de FR-001, et le seul chemin vers `Actif` passe
            // par `verifier_email`.
            statut: StatutUtilisateur::EnAttenteVerificationEmail,
            locale,
            cree_le: maintenant,
            verrouillage: Verrouillage::default(),
        }
    }

    /// Émet un jeton de vérification valable `VALIDITE_HEURES`.
    pub fn emettre_jeton_verification(maintenant: DateTime<Utc>) -> JetonEmis {
        let en_clair = JetonVerification::tirer();
        JetonEmis {
            empreinte: en_clair.empreinte(),
            en_clair,
            expire_le: maintenant + Duration::hours(VALIDITE_HEURES),
        }
    }

    pub fn est_actif(&self) -> bool {
        self.statut == StatutUtilisateur::Actif
    }

    /// Passe le compte en `ACTIVE`. Idempotent : re-vérifier un compte déjà
    /// actif n'est pas une erreur, c'est un double clic sur le lien.
    pub fn verifier_email(&mut self) {
        self.statut = StatutUtilisateur::Actif;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mot_de_passe::{MotDePasse, ParametresArgon2};

    fn empreinte() -> EmpreinteMotDePasse {
        let mdp = MotDePasse::parse("Marie@2026Secure").unwrap();
        EmpreinteMotDePasse::calculer(&mdp, ParametresArgon2::tests()).unwrap()
    }

    fn instant() -> DateTime<Utc> {
        DateTime::from_timestamp(1_780_000_000, 0).unwrap()
    }

    #[test]
    fn happy_un_compte_neuf_est_en_attente_de_verification() {
        let u = Utilisateur::inscrire(
            Email::parse("marie@example.eu").unwrap(),
            empreinte(),
            Locale::Fr,
            instant(),
        );
        assert_eq!(u.statut, StatutUtilisateur::EnAttenteVerificationEmail);
        assert!(!u.est_actif());
        assert_eq!(u.cree_le, instant());
    }

    #[test]
    fn happy_la_verification_active_le_compte() {
        let mut u = Utilisateur::inscrire(
            Email::parse("marie@example.eu").unwrap(),
            empreinte(),
            Locale::Fr,
            instant(),
        );
        u.verifier_email();
        assert!(u.est_actif());
        assert_eq!(u.statut.as_str(), "ACTIVE");
    }

    #[test]
    fn negative_un_statut_inconnu_ne_se_relit_pas() {
        assert_eq!(StatutUtilisateur::parse("SUPPRIME"), None);
        assert_eq!(StatutUtilisateur::parse("active"), None);
    }

    #[test]
    fn edge_le_jeton_expire_une_heure_apres_son_emission() {
        let emis = Utilisateur::emettre_jeton_verification(instant());
        assert_eq!(emis.expire_le, instant() + Duration::hours(1));
        assert_eq!(emis.empreinte, emis.en_clair.empreinte());
    }

    #[test]
    fn edge_reverifier_un_compte_actif_reste_sans_effet() {
        let mut u = Utilisateur::inscrire(
            Email::parse("marie@example.eu").unwrap(),
            empreinte(),
            Locale::Fr,
            instant(),
        );
        u.verifier_email();
        u.verifier_email();
        assert!(u.est_actif());
    }

    #[test]
    fn security_deux_inscriptions_ne_partagent_pas_d_identifiant() {
        let a = Utilisateur::inscrire(
            Email::parse("a@example.eu").unwrap(),
            empreinte(),
            Locale::Fr,
            instant(),
        );
        let b = Utilisateur::inscrire(
            Email::parse("b@example.eu").unwrap(),
            empreinte(),
            Locale::Nl,
            instant(),
        );
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn security_aucun_chemin_ne_cree_un_compte_deja_actif() {
        // Le seul constructeur public force `PENDING_EMAIL_VERIFY`. Si un jour
        // quelqu'un ajoute un `Utilisateur { .. }` littéral ailleurs, ce test
        // ne l'attrapera pas — mais il attrape le cas bien plus probable où
        // `inscrire` gagne un paramètre `statut`.
        let statuts: Vec<_> = [Locale::Fr, Locale::Nl, Locale::En]
            .into_iter()
            .map(|l| {
                Utilisateur::inscrire(
                    Email::parse("marie@example.eu").unwrap(),
                    empreinte(),
                    l,
                    instant(),
                )
                .statut
            })
            .collect();
        assert!(statuts
            .iter()
            .all(|s| *s == StatutUtilisateur::EnAttenteVerificationEmail));
    }

    #[test]
    fn security_le_debug_de_l_agregat_ne_revele_pas_l_empreinte() {
        let u = Utilisateur::inscrire(
            Email::parse("marie@example.eu").unwrap(),
            empreinte(),
            Locale::Fr,
            instant(),
        );
        let trace = format!("{u:?}");
        assert!(trace.contains("EmpreinteMotDePasse(***)"));
        assert!(!trace.contains("$argon2id$"));
    }
}
