//! Adaptateur d'émission du jeton d'accès : JWT signé en HS256 (FR-004).
//!
//! **Pourquoi ici et non dans un crate dédié.** Le format du jeton est un
//! détail de transport, et `klaar-api` en est le seul consommateur. Le jour où
//! un second service doit vérifier ces jetons, HS256 ne conviendra plus — il
//! faudrait partager le secret de signature avec lui, donc lui donner le
//! pouvoir d'en émettre. C'est à ce moment-là qu'un crate dédié et une paire de
//! clés asymétriques (ES256, comme VAPID) se justifieront, pas avant.

use chrono::{DateTime, TimeZone, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use klaar_application::ports::jeton_acces::{ClaimsAcces, EmetteurJetonAcces, ErreurJeton};

/// Longueur minimale du secret, en octets.
///
/// HS256 est un HMAC-SHA256 : un secret plus court que sa sortie n'ajoute rien
/// et se casse hors ligne. 32 octets sont le minimum de la RFC 8725 §3.5.
pub const LONGUEUR_MIN_SECRET: usize = 32;

/// Corps du JWT.
///
/// Noms courts et conventionnels (`sub`, `iat`, `exp`) : ce sont ceux que
/// toute bibliothèque de vérification attend, y compris celles qu'on n'a pas
/// écrites.
#[derive(Serialize, Deserialize)]
struct Corps {
    sub: String,
    iat: i64,
    exp: i64,
}

pub struct JwtHs256 {
    encodage: EncodingKey,
    decodage: DecodingKey,
    validation: Validation,
}

impl JwtHs256 {
    pub fn new(secret: &[u8]) -> Result<Self, ErreurJeton> {
        if secret.len() < LONGUEUR_MIN_SECRET {
            return Err(ErreurJeton(format!(
                "secret de {} octets, minimum {LONGUEUR_MIN_SECRET}",
                secret.len()
            )));
        }
        let mut validation = Validation::new(Algorithm::HS256);
        // L'algorithme attendu est fixé explicitement. Sans cela, un jeton
        // annonçant `alg: none` ou un algorithme faible serait accepté sur la
        // foi de son propre en-tête — la faille la plus classique du JWT.
        validation.algorithms = vec![Algorithm::HS256];
        validation.validate_exp = true;
        // Aucune tolérance d'horloge : un seul service émet et vérifie, il n'y
        // a pas de dérive à absorber, et la tolérance par défaut prolongerait
        // silencieusement la validité.
        validation.leeway = 0;
        Ok(Self {
            encodage: EncodingKey::from_secret(secret),
            decodage: DecodingKey::from_secret(secret),
            validation,
        })
    }
}

impl EmetteurJetonAcces for JwtHs256 {
    fn emettre(&self, claims: &ClaimsAcces) -> Result<String, ErreurJeton> {
        let corps = Corps {
            sub: claims.utilisateur_id.to_string(),
            iat: claims.emis_le.timestamp(),
            exp: claims.expire_le.timestamp(),
        };
        encode(&Header::new(Algorithm::HS256), &corps, &self.encodage)
            .map_err(|e| ErreurJeton(e.to_string()))
    }

    fn verifier(&self, jeton: &str) -> Result<ClaimsAcces, ErreurJeton> {
        let decode = decode::<Corps>(jeton, &self.decodage, &self.validation)
            .map_err(|e| ErreurJeton(e.to_string()))?;
        let corps = decode.claims;
        let utilisateur_id = Uuid::parse_str(&corps.sub)
            .map_err(|e| ErreurJeton(format!("sujet illisible : {e}")))?;
        Ok(ClaimsAcces {
            utilisateur_id,
            emis_le: horodatage(corps.iat)?,
            expire_le: horodatage(corps.exp)?,
        })
    }
}

fn horodatage(secondes: i64) -> Result<DateTime<Utc>, ErreurJeton> {
    Utc.timestamp_opt(secondes, 0)
        .single()
        .ok_or_else(|| ErreurJeton(format!("horodatage hors plage : {secondes}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    const SECRET: &[u8] = b"un-secret-de-test-de-quarante-huit-octets-au-moins";

    fn claims(dans: Duration) -> ClaimsAcces {
        let maintenant = Utc.timestamp_opt(1_780_000_000, 0).unwrap();
        ClaimsAcces {
            utilisateur_id: Uuid::new_v4(),
            emis_le: maintenant,
            expire_le: maintenant + dans,
        }
    }

    #[test]
    fn happy_un_jeton_emis_se_verifie_et_rend_les_memes_claims() {
        let jwt = JwtHs256::new(SECRET).unwrap();
        let attendus = ClaimsAcces {
            expire_le: Utc::now() + Duration::hours(1),
            emis_le: Utc::now(),
            ..claims(Duration::hours(1))
        };
        let jeton = jwt.emettre(&attendus).unwrap();
        let relus = jwt.verifier(&jeton).unwrap();
        assert_eq!(relus.utilisateur_id, attendus.utilisateur_id);
        assert_eq!(relus.expire_le.timestamp(), attendus.expire_le.timestamp());
    }

    #[test]
    fn negative_un_secret_trop_court_est_refuse_a_la_construction() {
        // Refuser ici plutôt qu'à la première signature : un déploiement mal
        // configuré doit échouer au démarrage, pas à la première connexion.
        assert!(JwtHs256::new(b"court").is_err());
        assert!(JwtHs256::new(&[0u8; LONGUEUR_MIN_SECRET - 1]).is_err());
        assert!(JwtHs256::new(&[0u8; LONGUEUR_MIN_SECRET]).is_ok());
    }

    #[test]
    fn negative_un_jeton_signe_par_un_autre_secret_est_refuse() {
        let emetteur = JwtHs256::new(SECRET).unwrap();
        let autre = JwtHs256::new(b"un-autre-secret-tout-aussi-long-que-le-premier").unwrap();
        let jeton = emetteur
            .emettre(&ClaimsAcces {
                expire_le: Utc::now() + Duration::hours(1),
                emis_le: Utc::now(),
                ..claims(Duration::hours(1))
            })
            .unwrap();
        assert!(autre.verifier(&jeton).is_err());
    }

    #[test]
    fn negative_un_jeton_perime_est_refuse() {
        let jwt = JwtHs256::new(SECRET).unwrap();
        let maintenant = Utc::now();
        let jeton = jwt
            .emettre(&ClaimsAcces {
                utilisateur_id: Uuid::new_v4(),
                emis_le: maintenant - Duration::hours(2),
                expire_le: maintenant - Duration::hours(1),
            })
            .unwrap();
        assert!(jwt.verifier(&jeton).is_err());
    }

    #[test]
    fn edge_une_chaine_qui_n_est_pas_un_jeton_ne_fait_pas_paniquer() {
        let jwt = JwtHs256::new(SECRET).unwrap();
        for entree in ["", "abc", "a.b.c", "....", "eyJhbGciOiJIUzI1NiJ9"] {
            assert!(jwt.verifier(entree).is_err(), "entrée {entree:?}");
        }
    }

    #[test]
    fn security_un_jeton_annoncant_alg_none_est_refuse() {
        // La faille la plus classique du JWT : une bibliothèque qui croit
        // l'en-tête du jeton plutôt que sa propre configuration accepte un
        // jeton non signé. `{"alg":"none","typ":"JWT"}` suivi d'un corps
        // valable et d'une signature vide.
        let jwt = JwtHs256::new(SECRET).unwrap();
        let entete = "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0";
        let corps = "eyJzdWIiOiIwMDAwMDAwMC0wMDAwLTAwMDAtMDAwMC0wMDAwMDAwMDAwMDEiLCJpYXQiOjE3ODAwMDAwMDAsImV4cCI6NDEwMjQ0NDgwMH0";
        assert!(jwt.verifier(&format!("{entete}.{corps}.")).is_err());
    }

    #[test]
    fn security_modifier_le_sujet_invalide_la_signature() {
        let jwt = JwtHs256::new(SECRET).unwrap();
        let jeton = jwt
            .emettre(&ClaimsAcces {
                expire_le: Utc::now() + Duration::hours(1),
                emis_le: Utc::now(),
                ..claims(Duration::hours(1))
            })
            .unwrap();
        let mut morceaux: Vec<&str> = jeton.split('.').collect();
        // Corps réécrit sur un autre sujet, signature laissée telle quelle.
        let force = "eyJzdWIiOiIwMDAwMDAwMC0wMDAwLTAwMDAtMDAwMC0wMDAwMDAwMDAwMDEiLCJpYXQiOjE3ODAwMDAwMDAsImV4cCI6NDEwMjQ0NDgwMH0";
        morceaux[1] = force;
        assert!(jwt.verifier(&morceaux.join(".")).is_err());
    }

    #[test]
    fn security_le_jeton_ne_porte_ni_adresse_ni_empreinte_de_mot_de_passe() {
        // Le corps d'un JWT est lisible par son porteur : tout champ ajouté ici
        // est un champ publié. Ce test échoue si quelqu'un y glisse l'adresse
        // « pour éviter une requête ».
        let jwt = JwtHs256::new(SECRET).unwrap();
        let jeton = jwt
            .emettre(&ClaimsAcces {
                expire_le: Utc::now() + Duration::hours(1),
                emis_le: Utc::now(),
                ..claims(Duration::hours(1))
            })
            .unwrap();
        let corps = jeton.split('.').nth(1).unwrap();
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        let lisible = String::from_utf8(URL_SAFE_NO_PAD.decode(corps).unwrap()).unwrap();
        assert!(!lisible.contains('@'), "corps du jeton : {lisible}");
        assert!(!lisible.contains("argon2"));
        let champs: serde_json::Value = serde_json::from_str(&lisible).unwrap();
        assert_eq!(
            champs.as_object().unwrap().len(),
            3,
            "seuls sub, iat et exp doivent voyager : {lisible}"
        );
    }
}
