use serde::{Deserialize, Serialize};
use std::fmt;
use unicode_normalization::UnicodeNormalization;

/// Adresse email validée et normalisée (minuscules puis NFC).
///
/// La normalisation n'est pas cosmétique : sans elle, `jørgen@üniverse.eu`
/// saisi avec des accents combinants et le même email saisi précomposé sont
/// deux chaînes différentes, donc deux comptes pour une seule personne, et un
/// contrôle d'unicité qui ne contrôle rien (FR-001 `@edge`).
/// Validation volontairement minimale (présence d'un `@` avec local-part et
/// domaine non vides) : une regex RFC 5322 complète serait une fausse
/// précision ici, la vérification réelle passe par l'envoi du token (FR-001).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Email(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmailError {
    Empty,
    MissingAt,
    EmptyLocalPart,
    EmptyDomain,
}

impl fmt::Display for EmailError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EmailError::Empty => write!(f, "email vide"),
            EmailError::MissingAt => write!(f, "email sans '@'"),
            EmailError::EmptyLocalPart => write!(f, "partie locale vide avant '@'"),
            EmailError::EmptyDomain => write!(f, "domaine vide après '@'"),
        }
    }
}

impl std::error::Error for EmailError {}

impl Email {
    pub fn parse(input: &str) -> Result<Self, EmailError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(EmailError::Empty);
        }
        let (local, domain) = trimmed.split_once('@').ok_or(EmailError::MissingAt)?;
        if local.is_empty() {
            return Err(EmailError::EmptyLocalPart);
        }
        if domain.is_empty() {
            return Err(EmailError::EmptyDomain);
        }
        // Minuscules d'abord, NFC ensuite, et non l'inverse : le passage en
        // minuscules peut lui-même produire une forme décomposée, si bien que
        // normaliser avant laisserait passer le cas qu'on prétend traiter.
        Ok(Self(trimmed.to_lowercase().nfc().collect()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Email {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_parses_and_lowercases_valid_email() {
        let email = Email::parse("Gilles.Maury@Example.COM").unwrap();
        assert_eq!(email.as_str(), "gilles.maury@example.com");
    }

    #[test]
    fn negative_rejects_missing_at() {
        assert_eq!(Email::parse("pas-un-email"), Err(EmailError::MissingAt));
    }

    #[test]
    fn edge_rejects_empty_local_part() {
        assert_eq!(
            Email::parse("@example.com"),
            Err(EmailError::EmptyLocalPart)
        );
    }

    #[test]
    fn edge_normalises_combining_marks_to_precomposed_form() {
        // "JØrgen@Üniverse.eu" du PRD, le domaine étant saisi en décomposé :
        // U + U+0308 (tréma combinant) au lieu de Ü. Sans NFC, ces deux
        // écritures produisent deux comptes distincts pour une même adresse.
        let decompose = Email::parse("JØrgen@U\u{308}niverse.eu").unwrap();
        let precompose = Email::parse("JØrgen@Üniverse.eu").unwrap();
        assert_eq!(decompose, precompose);
        assert_eq!(precompose.as_str(), "jørgen@üniverse.eu");

        // La normalisation ne fait pas plus qu'elle ne promet. « ø » (U+00F8)
        // n'a aucune décomposition canonique : ce n'est pas un « o » porteur
        // d'une barre combinante, c'est une lettre à part entière. NFC ne
        // rapproche donc pas « o » + U+0338 de « ø », et c'est correct. Un
        // rapprochement des homoglyphes relève de la confusabilité (UTS #39),
        // pas de la normalisation, et n'est pas fourni ici.
        let barre_combinante = Email::parse("jo\u{338}rgen@üniverse.eu").unwrap();
        assert_ne!(barre_combinante, precompose);
    }

    #[test]
    fn security_rejects_empty_input_rather_than_panicking() {
        assert_eq!(Email::parse(""), Err(EmailError::Empty));
        assert_eq!(Email::parse("   "), Err(EmailError::Empty));
    }
}
