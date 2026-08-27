//! base64url sans remplissage, l'encodage employé partout par Web Push.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;

pub fn encode(octets: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(octets)
}

/// Décode en tolérant le remplissage : la spécification l'interdit, mais des
/// navigateurs et des bibliothèques en produisent quand même, et refuser une
/// clé par ailleurs valable ne protège de rien.
pub fn decode(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    let nettoye = s.trim_end_matches('=');
    URL_SAFE_NO_PAD.decode(nettoye)
}
