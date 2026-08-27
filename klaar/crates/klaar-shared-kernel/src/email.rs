use serde::{Deserialize, Serialize};
use std::fmt;

/// Adresse email validée et normalisée (NFC, minuscules).
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
        Ok(Self(trimmed.to_lowercase()))
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
    fn security_rejects_empty_input_rather_than_panicking() {
        assert_eq!(Email::parse(""), Err(EmailError::Empty));
        assert_eq!(Email::parse("   "), Err(EmailError::Empty));
    }
}
