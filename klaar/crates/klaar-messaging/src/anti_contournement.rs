//! Détection des coordonnées dans un message (FR-032, Story 6.3).
//!
//! **Ce que cette détection est, et ce qu'elle n'est pas.** C'est un frein, pas
//! un mur. Quelqu'un de déterminé écrira « mon numéro se termine par les deux
//! derniers chiffres de l'année » et passera. Le but n'est pas de rendre le
//! contournement impossible — il ne peut pas l'être dans du texte libre — mais
//! de le rendre délibéré : personne ne peut plus prétendre avoir échangé un
//! numéro par mégarde, et la tentative est consignée.
//!
//! **La limite est assumée et écrite ici** plutôt que découverte par quelqu'un
//! qui croirait la barrière étanche.
//!
//! **Les faux positifs coûtent plus cher que les faux négatifs.** Un message
//! légitime bloqué est une conversation cassée entre deux personnes qui ont un
//! problème à régler ; un numéro qui passe est une commission perdue. Le seuil
//! penche donc du côté du passage : neuf chiffres au minimum pour un numéro
//! belge, ce qui laisse passer les dates, les montants et les âges.

use std::fmt;

/// Chiffres minimaux d'un numéro belge : `0x xxx xx xx`, soit neuf.
///
/// **Pas huit.** Une date écrite `24/12/2026` fait huit chiffres une fois les
/// séparateurs retirés, et la bloquer casserait la moitié des prises de
/// rendez-vous.
const CHIFFRES_MIN: usize = 9;

/// Au-delà, ce n'est plus un numéro de téléphone belge.
///
/// Quatorze : `0032` suivi d'un portable à dix chiffres sans son zéro initial,
/// soit la forme la plus longue qu'on rencontre.
const CHIFFRES_MAX: usize = 14;

/// Ce que la détection a trouvé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coordonnee {
    Telephone,
    Courriel,
}

impl Coordonnee {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Telephone => "PHONE",
            Self::Courriel => "EMAIL",
        }
    }
}

impl fmt::Display for Coordonnee {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Cherche une coordonnée dans un message. Rend la première trouvée.
///
/// L'ordre — téléphone puis courriel — n'a pas d'importance métier : un message
/// qui contient les deux est bloqué de toute façon, et le code rendu ne sert
/// qu'à expliquer.
pub fn detecter(message: &str) -> Option<Coordonnee> {
    if contient_telephone(message) {
        return Some(Coordonnee::Telephone);
    }
    if contient_courriel(message) {
        return Some(Coordonnee::Courriel);
    }
    None
}

/// Vrai si le message contient une suite de chiffres qui ressemble à un numéro.
///
/// **La normalisation est le cœur du procédé.** Les séparateurs — espaces,
/// points, tirets, barres obliques, parenthèses — sont retirés *entre* les
/// chiffres, ce qui réunit `04/70/12/34/56` en `0470123456`. Les découper
/// autrement ne marcherait pas : c'est précisément ce que fait quelqu'un qui
/// veut passer.
fn contient_telephone(message: &str) -> bool {
    let mut chiffres = String::new();
    let mut caracteres = message.chars().peekable();

    while let Some(c) = caracteres.next() {
        if c.is_ascii_digit() {
            chiffres.push(c);
            continue;
        }
        if est_separateur(c) && !chiffres.is_empty() {
            // Un séparateur ne coupe la suite que s'il n'est pas suivi d'un
            // chiffre : « 04 70 12 » reste une suite, « 47 ans et 12 » non.
            if caracteres
                .peek()
                .is_some_and(|s| s.is_ascii_digit() || est_separateur(*s))
            {
                continue;
            }
        }
        if ressemble_a_un_numero(&chiffres) {
            return true;
        }
        chiffres.clear();
    }
    ressemble_a_un_numero(&chiffres)
}

/// Séparateurs qu'on trouve dans un numéro écrit à la main, ou déguisé.
fn est_separateur(c: char) -> bool {
    matches!(c, ' ' | '.' | '-' | '/' | '(' | ')' | '\u{a0}' | '_' | '+')
}

/// Vrai si cette suite de chiffres a la forme d'un numéro belge.
///
/// Deux formes. Le **national** commence par zéro et fait neuf ou dix chiffres :
/// `02 123 45 67`, `0470 12 34 56`. L'**international** remplace ce zéro par un
/// indicatif — `+32` ou `0032` — et le reste fait donc un chiffre de moins.
///
/// L'indicatif est retiré avant de mesurer, plutôt que de multiplier les
/// longueurs admises : `0032470123456` et `32470123456` désignent le même
/// numéro, et les compter séparément se serait vu à la première erreur.
fn ressemble_a_un_numero(chiffres: &str) -> bool {
    if !(CHIFFRES_MIN..=CHIFFRES_MAX).contains(&chiffres.len()) {
        return false;
    }

    // National : le zéro initial est déjà là.
    if chiffres.starts_with('0') && !chiffres.starts_with("0032") {
        return (CHIFFRES_MIN..=10).contains(&chiffres.len());
    }

    // International : l'indicatif remplace le zéro, donc un chiffre de moins.
    for indicatif in ["0032", "32"] {
        if let Some(national) = chiffres.strip_prefix(indicatif) {
            return (CHIFFRES_MIN - 1..=9).contains(&national.len());
        }
    }
    false
}

/// Vrai si le message contient une adresse, écrite ou déguisée.
///
/// Les déguisements courants — `(at)`, `[at]`, ` at ` — sont ramenés à `@`
/// avant l'analyse. Ce n'est pas exhaustif, et ça ne peut pas l'être.
fn contient_courriel(message: &str) -> bool {
    // **Les variantes espacées d'abord, et c'est important.** Remplacer
    // « (dot) » par « . » sans consommer les espaces laisserait
    // « exemple . eu », qu'il faudrait recoller ensuite — et un recollage
    // aveugle des espaces autour des points transformerait « @Camille. Je suis
    // là » en une adresse. Consommer les espaces dans la substitution évite ce
    // faux positif au lieu d'avoir à le rattraper.
    let mut normalise = message.to_lowercase();
    for (deguisement, vrai) in [
        (" (at) ", "@"),
        (" [at] ", "@"),
        (" {at} ", "@"),
        (" at ", "@"),
        (" arobase ", "@"),
        ("(at)", "@"),
        ("[at]", "@"),
        ("{at}", "@"),
        (" (dot) ", "."),
        (" [dot] ", "."),
        (" {dot} ", "."),
        (" dot ", "."),
        (" point ", "."),
        ("(dot)", "."),
        ("[dot]", "."),
        ("{dot}", "."),
    ] {
        normalise = normalise.replace(deguisement, vrai);
    }

    // Une adresse minimale : quelque chose, un arobase, un domaine avec un
    // point et une extension d'au moins deux lettres.
    normalise.split('@').skip(1).any(|apres| {
        let avant_espace = apres.split_whitespace().next().unwrap_or("");
        let Some((domaine, extension)) = avant_espace.rsplit_once('.') else {
            return false;
        };
        !domaine.is_empty()
            && domaine.chars().any(|c| c.is_alphanumeric())
            && extension.len() >= 2
            && extension.chars().take(2).all(|c| c.is_alphabetic())
    }) && normalise
        .split('@')
        .next()
        .is_some_and(|avant| avant.chars().any(|c| c.is_alphanumeric()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // === @happy ===

    #[test]
    fn happy_un_numero_de_portable_est_detecte() {
        // FR-032 `@happy`.
        assert_eq!(
            detecter("appelez 0470 12 34 56"),
            Some(Coordonnee::Telephone)
        );
    }

    #[test]
    fn happy_une_adresse_est_detectee() {
        assert_eq!(
            detecter("contacte moi@exemple.eu"),
            Some(Coordonnee::Courriel)
        );
    }

    #[test]
    fn happy_un_message_ordinaire_passe() {
        for message in [
            "Bonjour, où êtes-vous ?",
            "Je serai là dans vingt minutes.",
            "La fuite vient du joint sous l'évier.",
        ] {
            assert_eq!(detecter(message), None, "{message}");
        }
    }

    // === @negative ===

    #[test]
    fn negative_un_age_n_est_pas_un_numero() {
        // FR-032 `@negative` : « j'ai 47 ans » doit passer.
        assert_eq!(detecter("j'ai 47 ans"), None);
        assert_eq!(detecter("il y a 3 enfants et 2 chats"), None);
    }

    #[test]
    fn negative_une_date_n_est_pas_un_numero() {
        // Huit chiffres une fois les séparateurs retirés : c'est pour cela que
        // le seuil est à neuf. Bloquer les dates casserait les prises de
        // rendez-vous, c'est-à-dire l'usage principal de la messagerie.
        for date in [
            "on dit le 24/12/2026 ?",
            "rendez-vous le 01-02-2026",
            "le 3 mars 2026 à 14h30",
        ] {
            assert_eq!(detecter(date), None, "{date}");
        }
    }

    #[test]
    fn negative_un_montant_n_est_pas_un_numero() {
        assert_eq!(detecter("ça fera 180,50 € au total"), None);
        assert_eq!(detecter("le devis est à 1 250 euros"), None);
    }

    #[test]
    fn negative_un_arobase_sans_domaine_passe() {
        // « @ » s'utilise aussi pour interpeller quelqu'un.
        assert_eq!(detecter("@Camille vous êtes là ?"), None);
    }

    #[test]
    fn negative_une_interpellation_suivie_d_un_point_passe() {
        // **Le faux positif qu'un recollage aveugle des espaces produirait.**
        // « @Camille. Je suis là » deviendrait « @camille.je suis là », et
        // « camille.je » ressemblerait à un domaine avec une extension de deux
        // lettres. C'est pour cela que les espaces sont consommés dans la
        // substitution plutôt que retirés après coup.
        for innocent in [
            "@Camille. Je suis là",
            "@Sacha. On se voit demain",
            "merci @Camille. À tout de suite",
        ] {
            assert_eq!(detecter(innocent), None, "{innocent}");
        }
    }

    #[test]
    fn negative_une_phrase_ordinaire_avec_des_points_passe() {
        assert_eq!(detecter("J'ai fini. Le joint est changé."), None);
        assert_eq!(detecter("C'est réparé. Bonne journée."), None);
    }

    // === @edge ===

    #[test]
    fn edge_un_numero_decoupe_est_detecte() {
        // FR-032 `@edge` : c'est exactement ce que fait quelqu'un qui veut
        // passer.
        for deguise in [
            "04/70/12/34/56",
            "0470.12.34.56",
            "0470-12-34-56",
            "0470 12 34 56",
            "04 70 12 34 56",
        ] {
            assert_eq!(
                detecter(deguise),
                Some(Coordonnee::Telephone),
                "déguisement : {deguise}"
            );
        }
    }

    #[test]
    fn edge_le_format_international_est_detecte() {
        for international in ["+32 470 12 34 56", "0032 470 12 34 56", "+32470123456"] {
            assert_eq!(
                detecter(international),
                Some(Coordonnee::Telephone),
                "{international}"
            );
        }
    }

    #[test]
    fn edge_un_fixe_a_neuf_chiffres_est_detecte() {
        assert_eq!(detecter("le 02 123 45 67"), Some(Coordonnee::Telephone));
    }

    #[test]
    fn edge_une_adresse_deguisee_est_detectee() {
        for deguise in [
            "moi (at) exemple.eu",
            "moi[at]exemple.eu",
            "moi at exemple.eu",
            "moi@exemple (dot) eu",
        ] {
            assert_eq!(detecter(deguise), Some(Coordonnee::Courriel), "{deguise}");
        }
    }

    #[test]
    fn edge_un_message_vide_ne_declenche_rien() {
        assert_eq!(detecter(""), None);
        assert_eq!(detecter("   "), None);
    }

    // === @security ===

    #[test]
    fn security_un_numero_noye_dans_du_texte_est_detecte() {
        assert_eq!(
            detecter("bonjour, si ça coupe rappelez le 0470123456 merci"),
            Some(Coordonnee::Telephone)
        );
    }

    #[test]
    fn security_la_detection_ne_depend_pas_de_la_casse() {
        assert_eq!(detecter("MOI(AT)EXEMPLE.EU"), Some(Coordonnee::Courriel));
    }

    #[test]
    fn security_le_vocabulaire_rendu_est_stable() {
        // Ces codes sortent du service et se retrouvent dans des messages
        // affichés et des journaux.
        assert_eq!(Coordonnee::Telephone.as_str(), "PHONE");
        assert_eq!(Coordonnee::Courriel.as_str(), "EMAIL");
    }

    #[test]
    fn security_la_detection_ne_plante_sur_aucune_entree() {
        // Un message est du texte libre venu du réseau : il contiendra des
        // choses qu'on n'a pas imaginées.
        for hostile in [
            "\u{0}\u{1}\u{2}",
            "🙂🙂🙂",
            "0000000000000000000000000000",
            &"9".repeat(10_000),
            "@@@@@@@",
            "....",
            "+++",
        ] {
            let _ = detecter(hostile);
        }
    }
}
