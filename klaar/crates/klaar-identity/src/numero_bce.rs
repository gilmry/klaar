//! Numéro d'entreprise à la Banque-Carrefour des Entreprises (FR-003).
//!
//! **Ce qui est vérifié ici, et ce qui ne peut pas l'être.** Un numéro BCE
//! porte une clé de contrôle : les deux derniers chiffres valent
//! `97 - (les huit premiers modulo 97)`. Cette vérification est **hors ligne**
//! et attrape ce qui compte le plus souvent — une faute de frappe, un chiffre
//! interverti, un numéro inventé au hasard.
//!
//! Elle ne dit rien de l'existence de l'entreprise, de son état de faillite ni
//! de son activité déclarée. Cela demande l'API de la BCE, hors du périmètre
//! vitrine, et c'est écrit dans `COMPLIANCE.md`. Un numéro bien formé n'est
//! donc pas un numéro valide : c'est un numéro qui mérite d'être soumis à la
//! BCE le jour où on pourra l'interroger.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Numéro d'entreprise belge, dix chiffres, clé de contrôle vérifiée.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NumeroBce(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NumeroBceError {
    Vide,
    /// Autre chose que dix chiffres, une fois les séparateurs retirés.
    LongueurInvalide {
        chiffres: usize,
    },
    /// Un numéro d'entreprise commence par 0 ou 1 ; 2 à 9 sont réservés à
    /// d'autres registres.
    PrefixeInvalide,
    /// La clé de contrôle ne correspond pas au corps du numéro.
    CleInvalide,
}

impl NumeroBceError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Vide => "BCE_EMPTY",
            Self::LongueurInvalide { .. } => "BCE_MALFORMED",
            Self::PrefixeInvalide => "BCE_MALFORMED",
            Self::CleInvalide => "BCE_CHECKSUM_FAILED",
        }
    }
}

impl fmt::Display for NumeroBceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vide => write!(f, "numéro d'entreprise vide"),
            Self::LongueurInvalide { chiffres } => {
                write!(f, "{chiffres} chiffres, un numéro BCE en compte dix")
            }
            Self::PrefixeInvalide => write!(f, "un numéro d'entreprise commence par 0 ou 1"),
            Self::CleInvalide => write!(f, "clé de contrôle incorrecte"),
        }
    }
}

impl std::error::Error for NumeroBceError {}

impl NumeroBce {
    /// Analyse un numéro saisi, séparateurs tolérés.
    ///
    /// `0123.456.749`, `0123456749` et `BE 0123 456 749` désignent le même
    /// numéro : refuser une mise en forme habituelle ferait buter l'inscription
    /// sur un détail typographique, et le titulaire recopie ce qui figure sur
    /// ses documents.
    pub fn parse(saisie: &str) -> Result<Self, NumeroBceError> {
        let brut = saisie.trim();
        if brut.is_empty() {
            return Err(NumeroBceError::Vide);
        }
        // Le préfixe pays est retiré avant tout : « BE0123456749 » est la forme
        // du numéro de TVA, que beaucoup donnent pour le numéro d'entreprise —
        // ce qu'il est, au préfixe près.
        let sans_pays = brut
            .strip_prefix("BE")
            .or_else(|| brut.strip_prefix("be"))
            .unwrap_or(brut);

        let chiffres: String = sans_pays.chars().filter(char::is_ascii_digit).collect();
        if chiffres.len() != 10 {
            return Err(NumeroBceError::LongueurInvalide {
                chiffres: chiffres.len(),
            });
        }
        if !matches!(chiffres.as_bytes()[0], b'0' | b'1') {
            return Err(NumeroBceError::PrefixeInvalide);
        }

        let corps: u64 = chiffres[..8]
            .parse()
            .map_err(|_| NumeroBceError::CleInvalide)?;
        let cle: u64 = chiffres[8..]
            .parse()
            .map_err(|_| NumeroBceError::CleInvalide)?;
        if cle != 97 - (corps % 97) {
            return Err(NumeroBceError::CleInvalide);
        }

        Ok(Self(chiffres))
    }

    /// Dix chiffres, sans séparateur. Forme de stockage et de comparaison.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Forme lisible `0123.456.749`, telle qu'elle figure sur les documents.
    pub fn formate(&self) -> String {
        format!("{}.{}.{}", &self.0[..4], &self.0[4..7], &self.0[7..])
    }
}

impl fmt::Display for NumeroBce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.formate())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fabrique un numéro dont la clé est correcte, à partir de huit chiffres.
    ///
    /// Les numéros de test sont **construits** plutôt que copiés d'entreprises
    /// réelles : un numéro BCE identifie une personne morale, et en figer un
    /// dans une suite de tests publiée le rattache durablement à ce dépôt.
    fn valide(corps: u64) -> String {
        let cle = 97 - (corps % 97);
        format!("{corps:08}{cle:02}")
    }

    #[test]
    fn happy_accepte_un_numero_dont_la_cle_est_correcte() {
        for corps in [1234567u64, 12345678, 1000000, 9999999] {
            let numero = valide(corps);
            assert!(
                NumeroBce::parse(&numero).is_ok(),
                "numéro construit {numero}"
            );
        }
    }

    #[test]
    fn happy_tolere_les_separateurs_habituels() {
        // Le titulaire recopie ce qui figure sur ses documents : point, espace
        // ou rien, et parfois le préfixe TVA.
        let numero = valide(1234567);
        let formes = [
            numero.clone(),
            format!("{}.{}.{}", &numero[..4], &numero[4..7], &numero[7..]),
            format!("{} {} {}", &numero[..4], &numero[4..7], &numero[7..]),
            format!("BE{numero}"),
            format!("BE {numero}"),
            format!("  {numero}  "),
        ];
        let attendu = NumeroBce::parse(&numero).unwrap();
        for forme in formes {
            assert_eq!(
                NumeroBce::parse(&forme),
                Ok(attendu.clone()),
                "forme {forme}"
            );
        }
    }

    #[test]
    fn happy_la_forme_lisible_est_celle_des_documents() {
        let numero = NumeroBce::parse(&valide(1234567)).unwrap();
        let lisible = numero.formate();
        assert_eq!(lisible.len(), 12);
        assert_eq!(lisible.matches('.').count(), 2);
        assert_eq!(&lisible[4..5], ".");
        assert_eq!(&lisible[8..9], ".");
    }

    #[test]
    fn negative_refuse_une_saisie_vide() {
        assert_eq!(NumeroBce::parse(""), Err(NumeroBceError::Vide));
        assert_eq!(NumeroBce::parse("   "), Err(NumeroBceError::Vide));
    }

    #[test]
    fn negative_refuse_un_nombre_de_chiffres_incorrect() {
        for saisie in ["012345", "01234567890", "0"] {
            assert!(
                matches!(
                    NumeroBce::parse(saisie),
                    Err(NumeroBceError::LongueurInvalide { .. })
                ),
                "saisie {saisie}"
            );
        }
    }

    #[test]
    fn negative_refuse_un_prefixe_reserve_a_d_autres_registres() {
        // 2 à 9 ne sont pas des numéros d'entreprise.
        let corps = 92345678u64;
        let cle = 97 - (corps % 97);
        assert_eq!(
            NumeroBce::parse(&format!("{corps:08}{cle:02}")),
            Err(NumeroBceError::PrefixeInvalide)
        );
    }

    #[test]
    fn negative_refuse_une_cle_de_controle_fausse() {
        let numero = valide(1234567);
        let cle: u64 = numero[8..].parse().unwrap();
        let fausse = format!("{}{:02}", &numero[..8], (cle + 1) % 97);
        assert_eq!(
            NumeroBce::parse(&fausse),
            Err(NumeroBceError::CleInvalide),
            "numéro {fausse}"
        );
    }

    #[test]
    fn edge_deux_chiffres_intervertis_sont_attrapes_par_la_cle() {
        // C'est la faute la plus fréquente à la saisie, et c'est exactement ce
        // que la clé de contrôle sert à détecter.
        let numero = valide(12345678);
        let mut octets: Vec<u8> = numero.clone().into_bytes();
        octets.swap(2, 3);
        let interverti = String::from_utf8(octets).unwrap();
        // L'inversion ne produit un numéro différent que si les deux chiffres
        // diffèrent ; ici, 2 et 3.
        assert_ne!(interverti, numero);
        assert_eq!(
            NumeroBce::parse(&interverti),
            Err(NumeroBceError::CleInvalide)
        );
    }

    #[test]
    fn edge_la_cle_peut_valoir_97_sans_casser_le_format() {
        // `97 - (corps % 97)` vaut 97 quand le corps est un multiple de 97.
        // Deux chiffres, donc, mais la valeur extrême du calcul : elle doit
        // repasser la vérification sans cas particulier.
        let corps = 97u64 * 12_345;
        let numero = format!("{corps:08}{:02}", 97 - (corps % 97));
        assert_eq!(&numero[8..], "97");
        assert!(NumeroBce::parse(&numero).is_ok(), "numéro {numero}");
    }

    #[test]
    fn security_aucune_saisie_hostile_ne_fait_paniquer() {
        // Ce contrôle est en frontière : il reçoit ce que le formulaire envoie.
        for hostile in [
            "../../etc/passwd",
            "'; DROP TABLE provider; --",
            "0123456749\u{0}",
            "٠١٢٣٤٥٦٧٤٩",
            &"9".repeat(1_000),
            "BE",
            "BEBEBE0123456749",
        ] {
            let _ = NumeroBce::parse(hostile);
        }
    }

    #[test]
    fn security_un_numero_bien_forme_n_est_pas_un_numero_valide() {
        // La clé de contrôle ne dit rien de l'existence de l'entreprise, de sa
        // faillite ni de son activité : cela demande l'API de la BCE, hors
        // périmètre. Ce test fixe l'intention pour que personne ne prenne
        // `parse` pour une validation d'entreprise.
        let invente = NumeroBce::parse(&valide(1111111));
        assert!(
            invente.is_ok(),
            "un numéro inventé mais bien formé passe : c'est la limite du contrôle hors ligne"
        );
    }
}
