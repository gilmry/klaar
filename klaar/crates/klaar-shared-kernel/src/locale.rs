use serde::{Deserialize, Serialize};
use std::fmt;

/// Langue supportée par Klaar : FR, NL, EN uniquement (Invariant §10.9, FR-043).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Locale {
    Fr,
    Nl,
    En,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleError(pub String);

impl fmt::Display for LocaleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "locale non supportée : '{}' (attendu fr, nl ou en)",
            self.0
        )
    }
}

impl std::error::Error for LocaleError {}

impl Locale {
    pub fn parse(input: &str) -> Result<Self, LocaleError> {
        match input.to_lowercase().as_str() {
            "fr" => Ok(Locale::Fr),
            "nl" => Ok(Locale::Nl),
            "en" => Ok(Locale::En),
            other => Err(LocaleError(other.to_string())),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Locale::Fr => "fr",
            Locale::Nl => "nl",
            Locale::En => "en",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_parses_the_three_supported_locales() {
        assert_eq!(Locale::parse("fr"), Ok(Locale::Fr));
        assert_eq!(Locale::parse("NL"), Ok(Locale::Nl));
        assert_eq!(Locale::parse("en"), Ok(Locale::En));
    }

    #[test]
    fn negative_rejects_unsupported_locale() {
        assert_eq!(Locale::parse("de"), Err(LocaleError("de".to_string())));
    }

    #[test]
    fn edge_rejects_empty_string() {
        assert!(Locale::parse("").is_err());
    }
}
