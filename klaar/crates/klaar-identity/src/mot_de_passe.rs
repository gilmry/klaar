//! Mot de passe et son empreinte (FR-001, NIST SP 800-63B).

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Longueur minimale, en caractères. NIST SP 800-63B §5.1.1.2 recommande 8 au
/// minimum absolu et davantage dès que le service le permet ; le PRD retient 12.
pub const LONGUEUR_MIN: usize = 12;

/// Longueur maximale, en octets UTF-8.
///
/// Ce n'est pas une règle de composition — NIST les proscrit — mais une borne
/// de coût. Argon2 lit l'intégralité du mot de passe : sans plafond, un envoi
/// de plusieurs mégaoctets fait travailler le serveur pour rien, et une
/// poignée de requêtes suffit à l'occuper. 128 octets laissent largement place
/// à une phrase de passe.
pub const LONGUEUR_MAX_OCTETS: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MotDePasseError {
    Vide,
    TropCourt { longueur: usize },
    TropLong { octets: usize },
    Hachage(String),
}

impl MotDePasseError {
    /// Code stable exposé au client. Les libellés changent avec la traduction,
    /// pas ces codes : c'est sur eux que le frontend branche ses messages.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Vide => "PASSWORD_EMPTY",
            Self::TropCourt { .. } => "PASSWORD_TOO_SHORT",
            Self::TropLong { .. } => "PASSWORD_TOO_LONG",
            Self::Hachage(_) => "PASSWORD_HASH_FAILED",
        }
    }
}

impl fmt::Display for MotDePasseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vide => write!(f, "mot de passe vide"),
            Self::TropCourt { longueur } => write!(
                f,
                "mot de passe de {longueur} caractères, minimum {LONGUEUR_MIN}"
            ),
            Self::TropLong { octets } => write!(
                f,
                "mot de passe de {octets} octets, maximum {LONGUEUR_MAX_OCTETS}"
            ),
            Self::Hachage(d) => write!(f, "hachage impossible : {d}"),
        }
    }
}

impl std::error::Error for MotDePasseError {}

/// Mot de passe en clair, validé, qui ne traverse jamais une frontière de
/// journalisation ni de sérialisation.
///
/// Le type ne dérive ni `Debug` ni `Serialize` : c'est la seule protection qui
/// tienne dans la durée. Une consigne « ne pas journaliser le mot de passe »
/// tient jusqu'au premier `tracing::info!(?dto)` ajouté six mois plus tard ; un
/// type qui ne sait pas s'afficher rend ce `tracing` non compilable.
#[derive(Clone, PartialEq, Eq)]
pub struct MotDePasse(String);

impl MotDePasse {
    pub fn parse(input: &str) -> Result<Self, MotDePasseError> {
        if input.is_empty() {
            return Err(MotDePasseError::Vide);
        }
        // Pas de `trim` : les espaces de début et de fin font partie du secret.
        // Les retirer réduirait silencieusement l'entropie d'une phrase de
        // passe, et empêcherait l'utilisateur de se reconnecter avec ce qu'il a
        // réellement saisi.
        let longueur = input.chars().count();
        if longueur < LONGUEUR_MIN {
            return Err(MotDePasseError::TropCourt { longueur });
        }
        if input.len() > LONGUEUR_MAX_OCTETS {
            return Err(MotDePasseError::TropLong {
                octets: input.len(),
            });
        }
        Ok(Self(input.to_string()))
    }

    fn expose(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

/// `Debug` explicite, pour que `{:?}` sur une structure englobante n'imprime
/// jamais le secret.
impl fmt::Debug for MotDePasse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MotDePasse(***)")
    }
}

/// Paramètres argon2id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParametresArgon2 {
    pub memoire_kib: u32,
    pub iterations: u32,
    pub parallelisme: u32,
}

impl ParametresArgon2 {
    /// Paramètres du PRD : 64 MiB, 3 itérations.
    pub const fn production() -> Self {
        Self {
            memoire_kib: 64 * 1024,
            iterations: 3,
            parallelisme: 1,
        }
    }

    /// Paramètres délibérément faibles, réservés aux tests.
    ///
    /// Sans eux, une suite qui hache quelques dizaines de mots de passe passe
    /// plusieurs secondes dans argon2, et finit par être désactivée. Ce n'est
    /// pas un affaiblissement de la production : le test
    /// `security_le_binaire_utilise_bien_les_parametres_du_prd` verrouille
    /// `production()` sur les valeurs du PRD.
    pub const fn tests() -> Self {
        Self {
            memoire_kib: 32,
            iterations: 1,
            parallelisme: 1,
        }
    }
}

impl Default for ParametresArgon2 {
    fn default() -> Self {
        Self::production()
    }
}

/// Empreinte argon2id au format PHC (`$argon2id$v=19$m=...,t=...,p=...$sel$hash`).
///
/// Le format embarque les paramètres employés : une empreinte calculée avec
/// d'anciens paramètres reste vérifiable après leur durcissement, ce qui rend
/// la migration progressive possible sans invalider les comptes existants.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmpreinteMotDePasse(String);

impl EmpreinteMotDePasse {
    pub fn calculer(
        mot_de_passe: &MotDePasse,
        parametres: ParametresArgon2,
    ) -> Result<Self, MotDePasseError> {
        let params = Params::new(
            parametres.memoire_kib,
            parametres.iterations,
            parametres.parallelisme,
            None,
        )
        .map_err(|e| MotDePasseError::Hachage(e.to_string()))?;
        let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let sel = SaltString::generate(&mut OsRng);
        let empreinte = argon
            .hash_password(mot_de_passe.expose(), &sel)
            .map_err(|e| MotDePasseError::Hachage(e.to_string()))?;
        Ok(Self(empreinte.to_string()))
    }

    /// Reconstruit une empreinte lue en base, sans la recalculer.
    pub fn depuis_phc(phc: &str) -> Result<Self, MotDePasseError> {
        PasswordHash::new(phc).map_err(|e| MotDePasseError::Hachage(e.to_string()))?;
        Ok(Self(phc.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn verifier(&self, mot_de_passe: &MotDePasse) -> bool {
        let Ok(attendu) = PasswordHash::new(&self.0) else {
            return false;
        };
        // `Argon2::default()` ne fixe pas les paramètres de vérification : ils
        // sont lus dans la chaîne PHC ci-dessus. Vérifier avec les paramètres
        // courants ferait échouer toute empreinte plus ancienne.
        Argon2::default()
            .verify_password(mot_de_passe.expose(), &attendu)
            .is_ok()
    }
}

impl fmt::Debug for EmpreinteMotDePasse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EmpreinteMotDePasse(***)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: ParametresArgon2 = ParametresArgon2::tests();

    #[test]
    fn happy_accepte_un_mot_de_passe_conforme_et_le_verifie() {
        let mdp = MotDePasse::parse("Marie@2026Secure").unwrap();
        let empreinte = EmpreinteMotDePasse::calculer(&mdp, P).unwrap();
        assert!(empreinte.verifier(&mdp));
    }

    #[test]
    fn negative_refuse_un_mot_de_passe_trop_court() {
        assert_eq!(
            MotDePasse::parse("court").unwrap_err().code(),
            "PASSWORD_TOO_SHORT"
        );
    }

    #[test]
    fn negative_refuse_un_mot_de_passe_vide() {
        assert_eq!(MotDePasse::parse("").unwrap_err().code(), "PASSWORD_EMPTY");
    }

    #[test]
    fn negative_une_empreinte_ne_valide_pas_un_autre_mot_de_passe() {
        let mdp = MotDePasse::parse("Marie@2026Secure").unwrap();
        let autre = MotDePasse::parse("Marie@2026Secur3").unwrap();
        let empreinte = EmpreinteMotDePasse::calculer(&mdp, P).unwrap();
        assert!(!empreinte.verifier(&autre));
    }

    #[test]
    fn edge_accepte_exactement_douze_caracteres_et_refuse_onze() {
        assert!(MotDePasse::parse(&"a".repeat(12)).is_ok());
        assert!(MotDePasse::parse(&"a".repeat(11)).is_err());
    }

    #[test]
    fn edge_compte_les_caracteres_et_non_les_octets_pour_le_minimum() {
        // Douze caractères non-ASCII font plus de douze octets. Compter les
        // octets ferait passer pour conforme un mot de passe de trois signes.
        let douze = "🔐".repeat(12);
        assert_eq!(douze.len(), 48);
        assert!(MotDePasse::parse(&douze).is_ok());
        assert_eq!(
            MotDePasse::parse(&"🔐".repeat(11)).unwrap_err(),
            MotDePasseError::TropCourt { longueur: 11 }
        );
    }

    #[test]
    fn edge_conserve_les_espaces_de_bord() {
        let avec = MotDePasse::parse("  phrase de passe  ").unwrap();
        let sans = MotDePasse::parse("phrase de passe").unwrap();
        let empreinte = EmpreinteMotDePasse::calculer(&avec, P).unwrap();
        assert!(empreinte.verifier(&avec));
        assert!(!empreinte.verifier(&sans));
    }

    #[test]
    fn edge_refuse_au_dela_de_la_borne_de_cout() {
        assert_eq!(
            MotDePasse::parse(&"a".repeat(LONGUEUR_MAX_OCTETS + 1))
                .unwrap_err()
                .code(),
            "PASSWORD_TOO_LONG"
        );
    }

    #[test]
    fn security_le_meme_mot_de_passe_donne_deux_empreintes_differentes() {
        // Un sel aléatoire par empreinte : sinon, deux comptes partageant un
        // mot de passe sont repérables comme tels dans un vidage de base, et
        // une table pré-calculée les casse tous les deux d'un coup.
        let mdp = MotDePasse::parse("Marie@2026Secure").unwrap();
        let a = EmpreinteMotDePasse::calculer(&mdp, P).unwrap();
        let b = EmpreinteMotDePasse::calculer(&mdp, P).unwrap();
        assert_ne!(a.as_str(), b.as_str());
        assert!(a.verifier(&mdp) && b.verifier(&mdp));
    }

    #[test]
    fn security_ni_le_mot_de_passe_ni_l_empreinte_ne_s_impriment() {
        let mdp = MotDePasse::parse("Marie@2026Secure").unwrap();
        let empreinte = EmpreinteMotDePasse::calculer(&mdp, P).unwrap();
        assert_eq!(format!("{mdp:?}"), "MotDePasse(***)");
        assert!(!format!("{mdp:?}").contains("Marie"));
        assert_eq!(format!("{empreinte:?}"), "EmpreinteMotDePasse(***)");
        assert!(!format!("{empreinte:?}").contains("argon2"));
    }

    #[test]
    fn security_l_empreinte_est_bien_de_l_argon2id() {
        let mdp = MotDePasse::parse("Marie@2026Secure").unwrap();
        let empreinte = EmpreinteMotDePasse::calculer(&mdp, P).unwrap();
        assert!(empreinte.as_str().starts_with("$argon2id$v=19$"));
    }

    #[test]
    fn security_le_binaire_utilise_bien_les_parametres_du_prd() {
        // FR-001 `@security` chiffre ces paramètres. Ce test échoue si
        // quelqu'un abaisse `production()` pour accélérer une suite de tests,
        // ce qui est exactement la façon dont ce type de régression arrive.
        let p = ParametresArgon2::production();
        assert_eq!(p.memoire_kib, 64 * 1024, "PRD : mémoire = 64 MiB");
        assert_eq!(p.iterations, 3, "PRD : 3 itérations");
    }

    #[test]
    fn security_une_empreinte_relue_reste_verifiable() {
        // Les paramètres vivent dans la chaîne PHC : durcir `production()`
        // plus tard ne doit pas déconnecter les comptes existants.
        let mdp = MotDePasse::parse("Marie@2026Secure").unwrap();
        let ancienne = EmpreinteMotDePasse::calculer(&mdp, P).unwrap();
        let relue = EmpreinteMotDePasse::depuis_phc(ancienne.as_str()).unwrap();
        assert!(relue.verifier(&mdp));
    }

    #[test]
    fn security_refuse_une_chaine_phc_invalide() {
        assert!(EmpreinteMotDePasse::depuis_phc("pas-une-empreinte").is_err());
    }
}
