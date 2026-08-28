//! État d'un échange OIDC : `state`, `nonce`, PKCE (FR-002, Story 1.5).
//!
//! **Rien ici ne parle à itsme.** Ce module tient ce qui protège l'échange —
//! le jeton anti-falsification de requête, le nombre à usage unique contre le
//! rejeu, et le vérificateur PKCE — et ces trois-là sont à nous. Le contrat
//! itsme manque ; les garanties, non, et ce sont elles qu'on ne voudrait pas
//! écrire dans l'urgence le jour où les identifiants arrivent.
//!
//! **`state` et `nonce` ne font pas le même travail, et les confondre est une
//! erreur courante.** Le `state` revient dans l'URL de retour : il prouve que
//! c'est bien nous qui avons lancé l'échange, contre une requête forgée depuis
//! un autre site. Le `nonce` revient **dans le jeton d'identité signé** : il
//! prouve que ce jeton-là a été émis pour cet échange-ci, contre le rejeu d'un
//! jeton authentique capté ailleurs. N'en avoir qu'un laisse l'autre attaque
//! ouverte.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::fmt;

/// Durée de vie d'un échange, en secondes.
///
/// Dix minutes. Le temps de déverrouiller son téléphone, d'ouvrir itsme et de
/// confirmer — pas davantage : un échange ouvert est une fenêtre pendant
/// laquelle un `state` capté reste utilisable.
pub const VALIDITE_SECONDES: i64 = 600;

/// Octets tirés pour `state`, `nonce` et le vérificateur PKCE.
///
/// Trente-deux, soit 256 bits du générateur du système. La spécification PKCE
/// autorise un vérificateur de 43 à 128 caractères ; 32 octets en base64url
/// sans remplissage en font 43, le minimum admis, et c'est déjà hors de portée
/// d'une énumération.
pub const OCTETS_ALEA: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EchangeError {
    /// Le `state` rendu ne correspond à aucun échange en cours.
    ///
    /// **Même refus qu'un `state` faux.** Distinguer « inconnu » de « expiré »
    /// dirait à qui essaie s'il a mis la main sur un vrai.
    EtatInconnu,
    /// L'échange a dépassé sa fenêtre.
    Expire,
    /// Le `nonce` du jeton d'identité ne correspond pas à celui de l'échange.
    NonceDiscordant,
}

impl EchangeError {
    pub fn code(&self) -> &'static str {
        // Un seul code : les trois causes sont indistinguables de l'extérieur,
        // et le contraire renseignerait qui essaie.
        "ITSME_STATE_INVALID"
    }
}

impl fmt::Display for EchangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EtatInconnu => write!(f, "échange inconnu"),
            Self::Expire => write!(f, "échange expiré après {VALIDITE_SECONDES} secondes"),
            Self::NonceDiscordant => write!(f, "nonce du jeton discordant"),
        }
    }
}

impl std::error::Error for EchangeError {}

/// Un échange OIDC ouvert, côté service.
///
/// **Le vérificateur PKCE ne sort jamais d'ici avant l'échange de code.** C'est
/// tout son intérêt : il ne circule pas dans le navigateur, contrairement au
/// défi qui en est le condensé.
#[derive(Clone, PartialEq, Eq)]
pub struct Echange {
    state: String,
    nonce: String,
    verificateur: String,
    pub ouvert_le: DateTime<Utc>,
}

// `Debug` écrit à la main : dériver imprimerait le vérificateur PKCE dans le
// premier journal venu, ce qui annulerait la protection qu'il apporte.
impl fmt::Debug for Echange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Echange")
            .field("state", &"***")
            .field("nonce", &"***")
            .field("verificateur", &"***")
            .field("ouvert_le", &self.ouvert_le)
            .finish()
    }
}

impl Echange {
    /// Ouvre un échange : trois valeurs tirées du générateur du système.
    pub fn ouvrir(maintenant: DateTime<Utc>) -> Self {
        Self {
            state: alea(),
            nonce: alea(),
            verificateur: alea(),
            ouvert_le: maintenant,
        }
    }

    /// Le `state`, à placer dans l'URL d'autorisation.
    pub fn state(&self) -> &str {
        &self.state
    }

    /// Le `nonce`, à placer dans l'URL et à retrouver dans le jeton signé.
    pub fn nonce(&self) -> &str {
        &self.nonce
    }

    /// Le **défi** PKCE : condensé SHA-256 du vérificateur, en base64url.
    ///
    /// C'est lui qui part dans le navigateur. Le vérificateur reste ici, et
    /// c'est ce qui empêche celui qui intercepte le code d'autorisation de
    /// l'échanger contre un jeton.
    pub fn defi(&self) -> String {
        let condense = Sha256::digest(self.verificateur.as_bytes());
        URL_SAFE_NO_PAD.encode(condense)
    }

    /// La méthode de défi. `S256` et jamais `plain` : `plain` envoie le
    /// vérificateur lui-même, ce qui revient à ne pas faire de PKCE.
    pub fn methode_defi(&self) -> &'static str {
        "S256"
    }

    /// Le vérificateur, pour l'échange du code contre un jeton.
    ///
    /// Nommé `expose` comme ailleurs dans ce dépôt : le nom rappelle à
    /// l'appelant qu'il tient un secret.
    pub fn expose_verificateur(&self) -> &str {
        &self.verificateur
    }

    pub fn est_valide(&self, maintenant: DateTime<Utc>) -> bool {
        maintenant < self.ouvert_le + Duration::seconds(VALIDITE_SECONDES)
    }

    /// Vérifie le retour d'itsme.
    ///
    /// **Les deux vérifications, jamais une seule.** Le `state` prouve que
    /// l'échange vient de nous ; le `nonce` prouve que le jeton a été émis pour
    /// cet échange. Se contenter du premier laisse rejouer un jeton authentique
    /// capté ailleurs.
    pub fn verifier(
        &self,
        state_rendu: &str,
        nonce_du_jeton: &str,
        maintenant: DateTime<Utc>,
    ) -> Result<(), EchangeError> {
        if !egal_en_temps_constant(state_rendu.as_bytes(), self.state.as_bytes()) {
            return Err(EchangeError::EtatInconnu);
        }
        if !self.est_valide(maintenant) {
            return Err(EchangeError::Expire);
        }
        if !egal_en_temps_constant(nonce_du_jeton.as_bytes(), self.nonce.as_bytes()) {
            return Err(EchangeError::NonceDiscordant);
        }
        Ok(())
    }
}

/// Trente-deux octets du générateur du système, en base64url sans remplissage.
fn alea() -> String {
    let mut octets = [0u8; OCTETS_ALEA];
    OsRng.fill_bytes(&mut octets);
    URL_SAFE_NO_PAD.encode(octets)
}

/// Comparaison sans court-circuit.
///
/// Le `state` et le `nonce` sont des secrets de courte vie : les comparer
/// octet par octet avec sortie anticipée laisserait le temps de réponse en
/// révéler le préfixe.
fn egal_en_temps_constant(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut difference = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        difference |= x ^ y;
    }
    difference == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 1, 9, 0, 0).unwrap()
    }

    #[test]
    fn happy_un_retour_conforme_est_accepte() {
        let e = Echange::ouvrir(t0());
        assert_eq!(e.verifier(e.state(), e.nonce(), t0()), Ok(()));
    }

    #[test]
    fn security_le_state_et_le_nonce_sont_deux_protections_distinctes() {
        // Le cœur du module. Le `state` seul laisse rejouer un jeton
        // authentique capté ailleurs ; le `nonce` seul laisse forger la requête
        // de retour depuis un autre site.
        let e = Echange::ouvrir(t0());
        let autre = Echange::ouvrir(t0());
        assert_eq!(
            e.verifier(autre.state(), e.nonce(), t0()),
            Err(EchangeError::EtatInconnu)
        );
        assert_eq!(
            e.verifier(e.state(), autre.nonce(), t0()),
            Err(EchangeError::NonceDiscordant)
        );
    }

    #[test]
    fn security_le_state_et_le_nonce_ne_sont_pas_la_meme_valeur() {
        // Les tirer une seule fois pour les deux usages ferait d'une fuite du
        // `state` — qui voyage dans une URL, donc dans l'historique et les
        // journaux — une fuite du `nonce`.
        let e = Echange::ouvrir(t0());
        assert_ne!(e.state(), e.nonce());
        assert_ne!(e.state(), e.expose_verificateur());
        assert_ne!(e.nonce(), e.expose_verificateur());
    }

    #[test]
    fn security_deux_echanges_ne_partagent_rien() {
        let a = Echange::ouvrir(t0());
        let b = Echange::ouvrir(t0());
        assert_ne!(a.state(), b.state());
        assert_ne!(a.nonce(), b.nonce());
        assert_ne!(a.expose_verificateur(), b.expose_verificateur());
    }

    #[test]
    fn security_le_defi_est_le_condense_du_verificateur_et_non_lui_meme() {
        // `plain` enverrait le vérificateur dans le navigateur, ce qui revient
        // à ne pas faire de PKCE du tout.
        let e = Echange::ouvrir(t0());
        assert_eq!(e.methode_defi(), "S256");
        assert_ne!(e.defi(), e.expose_verificateur());

        // Et c'est bien SHA-256, vérifiable de l'extérieur.
        let attendu = URL_SAFE_NO_PAD.encode(Sha256::digest(e.expose_verificateur().as_bytes()));
        assert_eq!(e.defi(), attendu);
    }

    #[test]
    fn security_le_verificateur_n_apparait_pas_dans_le_debug() {
        // Dériver `Debug` l'imprimerait dans le premier journal venu, ce qui
        // annulerait la protection qu'il apporte.
        let e = Echange::ouvrir(t0());
        let rendu = format!("{e:?}");
        assert!(!rendu.contains(e.expose_verificateur()));
        assert!(!rendu.contains(e.state()));
        assert!(!rendu.contains(e.nonce()));
    }

    #[test]
    fn negative_un_echange_expire_est_refuse() {
        let e = Echange::ouvrir(t0());
        let tard = t0() + Duration::seconds(VALIDITE_SECONDES);
        assert_eq!(
            e.verifier(e.state(), e.nonce(), tard),
            Err(EchangeError::Expire)
        );
    }

    #[test]
    fn edge_la_borne_exacte_de_validite_passe_encore() {
        let e = Echange::ouvrir(t0());
        let limite = t0() + Duration::seconds(VALIDITE_SECONDES) - Duration::seconds(1);
        assert_eq!(e.verifier(e.state(), e.nonce(), limite), Ok(()));
    }

    #[test]
    fn security_tous_les_refus_portent_le_meme_code() {
        for erreur in [
            EchangeError::EtatInconnu,
            EchangeError::Expire,
            EchangeError::NonceDiscordant,
        ] {
            assert_eq!(erreur.code(), "ITSME_STATE_INVALID");
        }
    }

    #[test]
    fn edge_les_valeurs_tirees_font_la_longueur_attendue_de_pkce() {
        // 32 octets en base64url sans remplissage font 43 caractères, le
        // minimum que la spécification PKCE admet pour un vérificateur.
        let e = Echange::ouvrir(t0());
        assert_eq!(e.expose_verificateur().len(), 43);
        // Et le jeu de caractères est celui que la spécification autorise.
        assert!(e
            .expose_verificateur()
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn negative_une_valeur_vide_ne_passe_pas() {
        let e = Echange::ouvrir(t0());
        assert!(e.verifier("", e.nonce(), t0()).is_err());
        assert!(e.verifier(e.state(), "", t0()).is_err());
    }
}
