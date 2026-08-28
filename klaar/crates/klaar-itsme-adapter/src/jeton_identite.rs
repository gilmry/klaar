//! Contrôle du jeton d'identité itsme (FR-002 `@security`, Story 1.5).
//!
//! **Un jeton signé n'est pas un jeton valable.** La signature dit qui l'a
//! émis ; elle ne dit ni pour qui, ni pour quand, ni pour quel échange. Les
//! revendications le disent, et les omettre est la façon la plus commune de
//! rater une intégration OIDC — un jeton parfaitement authentique, émis pour un
//! autre service, est alors accepté.
//!
//! **Quatre contrôles, et aucun n'est superflu :**
//! l'**émetteur**, sans quoi un fournisseur d'identité quelconque fait
//! l'affaire ; l'**audience**, sans quoi un jeton émis pour un autre client
//! passe ; l'**échéance**, sans quoi un jeton fuité vaut à vie ; et le
//! **nonce**, sans quoi un jeton authentique capté ailleurs se rejoue.
//!
//! **Le niveau eIDAS est vérifié aussi.** itsme peut authentifier à plusieurs
//! niveaux ; accepter n'importe lequel reviendrait à annoncer une garantie
//! « substantial » qu'on n'a pas obtenue.

use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use std::fmt;

/// Durée de vie maximale admise pour un jeton d'identité (FR-002
/// `@security` : « ≤ 60 s »).
///
/// **Contrôlée par nous, et pas seulement par l'émetteur.** Un fournisseur qui
/// émettrait des jetons d'une heure ouvrirait une fenêtre de rejeu d'une heure
/// sans que rien ne le signale de notre côté.
pub const DUREE_MAX_SECONDES: i64 = 60;

/// Tolérance de décalage d'horloge, en secondes.
///
/// Trente. Sans elle, une horloge en avance de deux secondes chez itsme ferait
/// refuser des jetons parfaitement valables ; plus large, elle rallongerait
/// d'autant la fenêtre de rejeu.
pub const DERIVE_TOLEREE_SECONDES: i64 = 30;

/// Niveau eIDAS exigé.
///
/// `substantial` est celui que FR-002 annonce. `low` ne vaut pas la
/// vérification d'identité promise ; `high` conviendrait aussi, mais itsme ne
/// le délivre pas et l'accepter serait écrire une branche qu'on ne peut pas
/// éprouver.
pub const ACR_ATTENDU: &str = "urn:be:fedict:iam:fas:citizen:eid";

/// Les revendications qu'on lit d'un jeton d'identité.
///
/// **Seulement celles-là.** itsme en renvoie davantage ; n'en désérialiser que
/// ce dont on se sert évite de conserver, par mégarde, des données d'identité
/// dont on n'a pas l'usage.
#[derive(Debug, Clone, Deserialize)]
pub struct Revendications {
    /// Identifiant stable de la personne chez itsme.
    ///
    /// **Jamais conservé en clair** (FR-002 `@security`) : c'est un
    /// identifiant national par un autre nom. Seule son empreinte est écrite.
    pub sub: String,
    /// Émetteur.
    pub iss: String,
    /// Audience : notre identifiant client.
    pub aud: String,
    /// Émis à.
    pub iat: i64,
    /// Expire à.
    pub exp: i64,
    /// Le nombre à usage unique de l'échange.
    pub nonce: Option<String>,
    /// Niveau d'authentification atteint.
    pub acr: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JetonError {
    /// Émetteur inattendu.
    EmetteurInconnu,
    /// Le jeton a été émis pour un autre client.
    AudienceEtrangere,
    /// Expiré, ou émis dans le futur.
    Echu,
    /// Durée de vie au-delà de ce que FR-002 admet.
    DureeExcessive { secondes: i64 },
    /// Le jeton ne porte pas de nonce : il ne peut être rattaché à aucun
    /// échange, donc il est rejouable.
    NonceAbsent,
    /// Niveau d'authentification insuffisant.
    NiveauInsuffisant,
    /// Identifiant de personne vide.
    SujetAbsent,
}

impl JetonError {
    pub fn code(&self) -> &'static str {
        // **Un seul code, comme pour la signature.** Dire laquelle des
        // vérifications a échoué renseignerait qui fabrique des jetons sur ce
        // qui lui reste à corriger.
        "ITSME_TOKEN_INVALID"
    }
}

impl fmt::Display for JetonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmetteurInconnu => write!(f, "émetteur inattendu"),
            Self::AudienceEtrangere => write!(f, "jeton émis pour un autre client"),
            Self::Echu => write!(f, "jeton expiré ou émis dans le futur"),
            Self::DureeExcessive { secondes } => write!(
                f,
                "durée de vie de {secondes} s, maximum {DUREE_MAX_SECONDES}"
            ),
            Self::NonceAbsent => write!(f, "jeton sans nonce : rejouable"),
            Self::NiveauInsuffisant => write!(f, "niveau d'authentification insuffisant"),
            Self::SujetAbsent => write!(f, "identifiant de personne absent"),
        }
    }
}

impl std::error::Error for JetonError {}

/// Ce qu'on attend du jeton, connu du service seul.
#[derive(Debug, Clone)]
pub struct Attendu {
    pub emetteur: String,
    /// Notre identifiant client chez itsme.
    pub audience: String,
}

/// Vérifie les revendications d'un jeton **dont la signature a déjà été
/// validée**.
///
/// La séparation est délibérée : la signature se vérifie contre une clé
/// obtenue du JWKS d'itsme — donc au réseau — tandis que tout ce qui suit se
/// vérifie hors ligne. Les mêler rendrait la seconde moitié impossible à
/// éprouver sans contrat.
pub fn verifier_revendications(
    revendications: &Revendications,
    attendu: &Attendu,
    nonce_de_l_echange: &str,
    maintenant: DateTime<Utc>,
) -> Result<(), JetonError> {
    if revendications.sub.trim().is_empty() {
        return Err(JetonError::SujetAbsent);
    }
    if revendications.iss != attendu.emetteur {
        return Err(JetonError::EmetteurInconnu);
    }
    // Comparaison stricte : une audience qui *contient* la nôtre ne suffit pas.
    if revendications.aud != attendu.audience {
        return Err(JetonError::AudienceEtrangere);
    }

    let (Some(emis), Some(expire)) = (
        DateTime::from_timestamp(revendications.iat, 0),
        DateTime::from_timestamp(revendications.exp, 0),
    ) else {
        return Err(JetonError::Echu);
    };

    let derive = Duration::seconds(DERIVE_TOLEREE_SECONDES);
    if maintenant >= expire + derive || maintenant + derive < emis {
        return Err(JetonError::Echu);
    }

    // La durée annoncée par l'émetteur lui-même. Un jeton d'une heure ouvrirait
    // une fenêtre de rejeu d'une heure ; FR-002 en admet soixante secondes.
    let duree = (expire - emis).num_seconds();
    if duree > DUREE_MAX_SECONDES {
        return Err(JetonError::DureeExcessive { secondes: duree });
    }

    let Some(nonce) = revendications.nonce.as_deref() else {
        return Err(JetonError::NonceAbsent);
    };
    if !egal_en_temps_constant(nonce.as_bytes(), nonce_de_l_echange.as_bytes()) {
        return Err(JetonError::NonceAbsent);
    }

    // Le niveau eIDAS. `None` est refusé : un jeton qui ne dit pas comment la
    // personne s'est authentifiée ne permet pas d'annoncer « identité
    // vérifiée ».
    match revendications.acr.as_deref() {
        Some(acr) if acr == ACR_ATTENDU => {}
        _ => return Err(JetonError::NiveauInsuffisant),
    }

    Ok(())
}

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

/// Les erreurs qu'itsme renvoie, traduites en codes du service (FR-002
/// `@negative`).
///
/// **Une liste fermée, et un repli explicite.** Un code inconnu devient
/// `ITSME_UNAVAILABLE` : mieux vaut proposer le repli par courriel que de
/// laisser une erreur non traduite remonter telle quelle à l'écran.
pub fn code_erreur_itsme(erreur: &str) -> &'static str {
    match erreur {
        "user_cancelled" | "access_denied" => "ITSME_CANCELLED",
        "timeout" => "ITSME_TIMEOUT",
        "phone_not_bound" => "ITSME_PHONE_MISSING",
        _ => "ITSME_UNAVAILABLE",
    }
}

/// Vrai si le numéro de téléphone relève d'un indicatif belge (FR-002
/// `@edge`).
///
/// **Le contrôle est fait avant de lancer l'échange**, pour que quelqu'un avec
/// un numéro français reçoive le repli par courriel plutôt qu'un échec itsme
/// qui ne lui dit rien.
pub fn numero_belge(telephone: &str) -> bool {
    let compact: String = telephone.chars().filter(|c| !c.is_whitespace()).collect();
    compact.starts_with("+32") || compact.starts_with("0032")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 1, 9, 0, 0).unwrap()
    }

    fn attendu() -> Attendu {
        Attendu {
            emetteur: "https://idp.itsme.services/v2".to_string(),
            audience: "klaar-client-id".to_string(),
        }
    }

    fn revendications() -> Revendications {
        Revendications {
            sub: "sujet-stable-123".to_string(),
            iss: "https://idp.itsme.services/v2".to_string(),
            aud: "klaar-client-id".to_string(),
            iat: t0().timestamp(),
            exp: (t0() + Duration::seconds(60)).timestamp(),
            nonce: Some("nonce-de-l-echange".to_string()),
            acr: Some(ACR_ATTENDU.to_string()),
        }
    }

    #[test]
    fn happy_un_jeton_conforme_est_accepte() {
        assert_eq!(
            verifier_revendications(&revendications(), &attendu(), "nonce-de-l-echange", t0()),
            Ok(())
        );
    }

    #[test]
    fn security_un_jeton_emis_pour_un_autre_client_est_refuse() {
        // Le cas qu'on rate le plus souvent : le jeton est authentique, signé
        // par itsme, mais destiné à un autre service. L'accepter ferait entrer
        // n'importe qui disposant d'un compte chez ce service.
        let mut r = revendications();
        r.aud = "un-autre-service".to_string();
        assert_eq!(
            verifier_revendications(&r, &attendu(), "nonce-de-l-echange", t0()),
            Err(JetonError::AudienceEtrangere)
        );
    }

    #[test]
    fn security_un_emetteur_inattendu_est_refuse() {
        // Sans ce contrôle, n'importe quel fournisseur d'identité fait
        // l'affaire — y compris un que l'attaquant contrôle.
        let mut r = revendications();
        r.iss = "https://idp.attaquant.example/v2".to_string();
        assert_eq!(
            verifier_revendications(&r, &attendu(), "nonce-de-l-echange", t0()),
            Err(JetonError::EmetteurInconnu)
        );
    }

    #[test]
    fn security_un_nonce_discordant_est_refuse() {
        // C'est ce qui empêche de rejouer un jeton authentique capté ailleurs.
        assert_eq!(
            verifier_revendications(&revendications(), &attendu(), "un-autre-nonce", t0()),
            Err(JetonError::NonceAbsent)
        );
    }

    #[test]
    fn security_un_jeton_sans_nonce_est_refuse() {
        // Il ne peut être rattaché à aucun échange, donc il est rejouable.
        let mut r = revendications();
        r.nonce = None;
        assert_eq!(
            verifier_revendications(&r, &attendu(), "nonce-de-l-echange", t0()),
            Err(JetonError::NonceAbsent)
        );
    }

    #[test]
    fn security_un_niveau_d_authentification_insuffisant_est_refuse() {
        // Accepter n'importe quel niveau reviendrait à annoncer une garantie
        // « substantial » qu'on n'a pas obtenue.
        for acr in [None, Some("urn:be:fedict:iam:fas:citizen:basic"), Some("")] {
            let mut r = revendications();
            r.acr = acr.map(str::to_string);
            assert_eq!(
                verifier_revendications(&r, &attendu(), "nonce-de-l-echange", t0()),
                Err(JetonError::NiveauInsuffisant),
                "acr accepté à tort : {acr:?}"
            );
        }
    }

    #[test]
    fn security_un_jeton_de_longue_duree_est_refuse() {
        // FR-002 `@security` : « durée de vie ≤ 60 s ». Un émetteur qui
        // émettrait des jetons d'une heure ouvrirait une fenêtre de rejeu
        // d'une heure sans que rien ne le signale de notre côté.
        let mut r = revendications();
        r.exp = (t0() + Duration::seconds(DUREE_MAX_SECONDES + 1)).timestamp();
        assert_eq!(
            verifier_revendications(&r, &attendu(), "nonce-de-l-echange", t0()),
            Err(JetonError::DureeExcessive {
                secondes: DUREE_MAX_SECONDES + 1
            })
        );
    }

    #[test]
    fn negative_un_jeton_expire_est_refuse() {
        let tard = t0() + Duration::seconds(60 + DERIVE_TOLEREE_SECONDES + 1);
        assert_eq!(
            verifier_revendications(&revendications(), &attendu(), "nonce-de-l-echange", tard),
            Err(JetonError::Echu)
        );
    }

    #[test]
    fn edge_une_derive_d_horloge_raisonnable_est_toleree() {
        // Sans tolérance, une horloge en avance de deux secondes chez itsme
        // ferait refuser des jetons parfaitement valables.
        let un_peu_avant = t0() - Duration::seconds(DERIVE_TOLEREE_SECONDES - 1);
        assert_eq!(
            verifier_revendications(
                &revendications(),
                &attendu(),
                "nonce-de-l-echange",
                un_peu_avant
            ),
            Ok(())
        );
    }

    #[test]
    fn negative_un_jeton_venu_du_futur_au_dela_de_la_derive_est_refuse() {
        let bien_avant = t0() - Duration::seconds(DERIVE_TOLEREE_SECONDES + 1);
        assert_eq!(
            verifier_revendications(
                &revendications(),
                &attendu(),
                "nonce-de-l-echange",
                bien_avant
            ),
            Err(JetonError::Echu)
        );
    }

    #[test]
    fn negative_un_sujet_vide_est_refuse() {
        for sujet in ["", "   "] {
            let mut r = revendications();
            r.sub = sujet.to_string();
            assert_eq!(
                verifier_revendications(&r, &attendu(), "nonce-de-l-echange", t0()),
                Err(JetonError::SujetAbsent)
            );
        }
    }

    #[test]
    fn security_tous_les_refus_portent_le_meme_code() {
        for erreur in [
            JetonError::EmetteurInconnu,
            JetonError::AudienceEtrangere,
            JetonError::Echu,
            JetonError::DureeExcessive { secondes: 3600 },
            JetonError::NonceAbsent,
            JetonError::NiveauInsuffisant,
            JetonError::SujetAbsent,
        ] {
            assert_eq!(erreur.code(), "ITSME_TOKEN_INVALID");
        }
    }

    #[test]
    fn happy_les_erreurs_itsme_sont_traduites() {
        // FR-002 `@negative`, tableau des exemples.
        assert_eq!(code_erreur_itsme("user_cancelled"), "ITSME_CANCELLED");
        assert_eq!(code_erreur_itsme("timeout"), "ITSME_TIMEOUT");
        assert_eq!(code_erreur_itsme("phone_not_bound"), "ITSME_PHONE_MISSING");
        assert_eq!(code_erreur_itsme("server_error"), "ITSME_UNAVAILABLE");
    }

    #[test]
    fn edge_une_erreur_itsme_inconnue_propose_le_repli() {
        // Mieux vaut proposer le repli par courriel que de laisser une erreur
        // non traduite remonter telle quelle à l'écran.
        assert_eq!(code_erreur_itsme("une_erreur_future"), "ITSME_UNAVAILABLE");
        assert_eq!(code_erreur_itsme(""), "ITSME_UNAVAILABLE");
    }

    #[test]
    fn edge_le_numero_belge_est_reconnu_sous_ses_deux_formes() {
        // FR-002 `@edge` : un numéro français doit recevoir le repli plutôt
        // qu'un échec itsme qui ne lui dit rien.
        assert!(numero_belge("+32 470 12 34 56"));
        assert!(numero_belge("+32470123456"));
        assert!(numero_belge("0032 470 12 34 56"));
        assert!(!numero_belge("+33 6 12 34 56 78"));
        assert!(
            !numero_belge("0470 12 34 56"),
            "sans indicatif, on ne sait pas"
        );
        assert!(!numero_belge(""));
    }
}
