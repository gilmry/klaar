//! Secteurs et Skills du catalogue (FR-008).

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::libelles::Libelles;

/// Code stable d'une entrée de catalogue.
///
/// Ces codes voyagent dans les URL, les statistiques, les exports et les
/// intégrations. Ils ne se renomment donc pas : le libellé change avec la
/// traduction, le code jamais. D'où le format restreint — minuscules ASCII,
/// chiffres et traits d'union — qui survit à une URL, à un nom de colonne et à
/// un fichier CSV sans échappement.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CodeCatalogue(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeError {
    Vide,
    TropLong { longueur: usize },
    CaractereInterdit { caractere: char },
    BordInvalide,
}

impl fmt::Display for CodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vide => write!(f, "code vide"),
            Self::TropLong { longueur } => write!(f, "code de {longueur} caractères, maximum 48"),
            Self::CaractereInterdit { caractere } => {
                write!(f, "caractère interdit dans un code : {caractere:?}")
            }
            Self::BordInvalide => write!(f, "un code ne peut ni commencer ni finir par un tiret"),
        }
    }
}

impl std::error::Error for CodeError {}

/// Longueur maximale d'un code. Assez pour être lisible, assez court pour
/// tenir dans une URL sans la couper.
pub const LONGUEUR_MAX_CODE: usize = 48;

impl CodeCatalogue {
    pub fn parse(input: &str) -> Result<Self, CodeError> {
        if input.is_empty() {
            return Err(CodeError::Vide);
        }
        if input.len() > LONGUEUR_MAX_CODE {
            return Err(CodeError::TropLong {
                longueur: input.len(),
            });
        }
        if let Some(c) = input
            .chars()
            .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-'))
        {
            return Err(CodeError::CaractereInterdit { caractere: c });
        }
        // Ni tiret de bord ni tiret double : ce sont les formes qui produisent
        // deux codes visuellement identiques mais distincts, et donc deux
        // entrées de catalogue là où on en voulait une.
        if input.starts_with('-') || input.ends_with('-') || input.contains("--") {
            return Err(CodeError::BordInvalide);
        }
        Ok(Self(input.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CodeCatalogue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Compétence proposée dans un Secteur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skill {
    pub code: CodeCatalogue,
    pub libelles: Libelles,
}

/// Secteur d'activité et les Skills qu'il regroupe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Secteur {
    pub code: CodeCatalogue,
    pub libelles: Libelles,
    pub skills: Vec<Skill>,
}

impl Secteur {
    /// Vrai si le secteur et tous ses Skills sont traduits dans les trois
    /// langues, et si aucun code de Skill n'apparaît deux fois.
    ///
    /// Un doublon de code produirait deux entrées que l'interface affiche à
    /// l'identique et que les statistiques comptent séparément.
    pub fn coherent(&self) -> bool {
        if !self.libelles.complet() {
            return false;
        }
        if !self.skills.iter().all(|s| s.libelles.complet()) {
            return false;
        }
        let mut codes: Vec<&str> = self.skills.iter().map(|s| s.code.as_str()).collect();
        codes.sort_unstable();
        let avant = codes.len();
        codes.dedup();
        codes.len() == avant
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use klaar_shared_kernel::Locale;

    fn skill(code: &str) -> Skill {
        Skill {
            code: CodeCatalogue::parse(code).unwrap(),
            libelles: Libelles::new("Fuite", "Lek", "Leak"),
        }
    }

    fn secteur(skills: Vec<Skill>) -> Secteur {
        Secteur {
            code: CodeCatalogue::parse("plomberie").unwrap(),
            libelles: Libelles::new("Plomberie", "Loodgieterij", "Plumbing"),
            skills,
        }
    }

    #[test]
    fn happy_accepte_un_code_bien_forme() {
        for code in ["plomberie", "serrurerie", "fuite-eau", "b2v-br", "auto2"] {
            assert!(CodeCatalogue::parse(code).is_ok(), "code {code}");
        }
    }

    #[test]
    fn happy_un_secteur_complet_est_coherent() {
        assert!(secteur(vec![skill("fuite-eau"), skill("debouchage")]).coherent());
    }

    #[test]
    fn negative_refuse_les_majuscules_et_les_espaces() {
        // Un code en majuscules produit deux entrées distinctes selon la casse
        // employée à l'écriture, et un espace casse toute URL qui le porte.
        assert!(matches!(
            CodeCatalogue::parse("Plomberie"),
            Err(CodeError::CaractereInterdit { caractere: 'P' })
        ));
        assert!(matches!(
            CodeCatalogue::parse("fuite eau"),
            Err(CodeError::CaractereInterdit { caractere: ' ' })
        ));
    }

    #[test]
    fn negative_refuse_les_accents_et_la_ponctuation() {
        // « électricité » se translittère en « electricite » : le libellé porte
        // l'accent, le code non.
        assert!(CodeCatalogue::parse("électricité").is_err());
        assert!(CodeCatalogue::parse("plomberie/fuite").is_err());
        assert!(CodeCatalogue::parse("plomberie_fuite").is_err());
    }

    #[test]
    fn negative_refuse_un_code_vide_ou_trop_long() {
        assert_eq!(CodeCatalogue::parse(""), Err(CodeError::Vide));
        assert!(matches!(
            CodeCatalogue::parse(&"a".repeat(LONGUEUR_MAX_CODE + 1)),
            Err(CodeError::TropLong { .. })
        ));
        assert!(CodeCatalogue::parse(&"a".repeat(LONGUEUR_MAX_CODE)).is_ok());
    }

    #[test]
    fn edge_refuse_les_tirets_de_bord_et_doubles() {
        // Ce sont les formes qui produisent deux codes visuellement identiques
        // mais distincts, donc deux entrées là où on en voulait une.
        for code in ["-plomberie", "plomberie-", "fuite--eau"] {
            assert_eq!(
                CodeCatalogue::parse(code),
                Err(CodeError::BordInvalide),
                "code {code}"
            );
        }
    }

    #[test]
    fn edge_un_secteur_dont_un_skill_n_est_pas_traduit_est_incoherent() {
        let bancal = Skill {
            code: CodeCatalogue::parse("debouchage").unwrap(),
            libelles: Libelles::new("Débouchage", "", "Unblocking"),
        };
        assert!(!secteur(vec![skill("fuite-eau"), bancal]).coherent());
    }

    #[test]
    fn edge_un_secteur_sans_skill_reste_coherent() {
        // Un secteur ouvert avant que ses compétences ne soient décrites : rien
        // d'anormal, l'interface affichera une liste vide.
        assert!(secteur(vec![]).coherent());
    }

    #[test]
    fn security_un_code_de_skill_en_double_rend_le_secteur_incoherent() {
        // Deux entrées identiques à l'affichage, comptées séparément dans les
        // statistiques : le genre de défaut qu'on ne voit qu'au rapport annuel.
        assert!(!secteur(vec![skill("fuite-eau"), skill("fuite-eau")]).coherent());
    }

    #[test]
    fn security_un_code_ne_porte_rien_qu_une_url_devrait_echapper() {
        for hostile in [
            "../../etc/passwd",
            "plomberie?x=1",
            "plomberie#a",
            "plomberie%20",
            "<script>",
            "plomberie;drop",
        ] {
            assert!(CodeCatalogue::parse(hostile).is_err(), "entrée {hostile:?}");
        }
    }

    #[test]
    fn security_le_libelle_reste_lisible_dans_les_trois_langues() {
        let s = secteur(vec![skill("fuite-eau")]);
        for locale in [Locale::Fr, Locale::Nl, Locale::En] {
            assert!(!s.libelles.pour(locale).is_empty());
            assert!(!s.skills[0].libelles.pour(locale).is_empty());
        }
    }
}
