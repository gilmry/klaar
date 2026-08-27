use serde::{Deserialize, Serialize};
use std::fmt;

/// Empreinte SHA-256 d'un document justificatif (anti-falsification,
/// Architecture §1.5 `SkillAttestation.document_hash`). Le calcul du hash
/// lui-même vit dans un adapter (IO) ; ce type ne fait que porter la valeur.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HashSha256([u8; 32]);

impl HashSha256 {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl fmt::Debug for HashSha256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HashSha256({})", self.to_hex())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_round_trips_bytes() {
        let bytes = [1u8; 32];
        let hash = HashSha256::from_bytes(bytes);
        assert_eq!(hash.as_bytes(), &bytes);
    }

    #[test]
    fn edge_all_zero_hash_formats_as_64_hex_zeros() {
        let hash = HashSha256::from_bytes([0u8; 32]);
        assert_eq!(hash.to_hex(), "0".repeat(64));
    }

    #[test]
    fn security_debug_output_never_panics_on_arbitrary_bytes() {
        let hash = HashSha256::from_bytes([255u8; 32]);
        let debug = format!("{hash:?}");
        assert!(debug.starts_with("HashSha256("));
    }
}
