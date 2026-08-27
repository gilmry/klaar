//! Erreurs communes aux ports de persistance.

use std::fmt;

#[derive(Debug)]
pub enum RepositoryError {
    Indisponible(String),
    Contrainte(String),
}

impl fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Indisponible(d) => write!(f, "dépôt indisponible : {d}"),
            Self::Contrainte(d) => write!(f, "contrainte violée : {d}"),
        }
    }
}

impl std::error::Error for RepositoryError {}
