//! Extraction de l'utilisateur authentifié (FR-004).
//!
//! Un extracteur plutôt qu'un middleware : le type `Authentifie` dans la
//! signature d'un handler **est** la déclaration que la route est protégée. Un
//! middleware se configure ailleurs, et une route ajoutée plus tard hors de son
//! périmètre est publique sans que rien ne le signale. Ici, oublier la
//! protection revient à ne pas demander l'argument, ce qui se voit à la
//! relecture d'une seule ligne.

use std::future::{ready, Ready};

use actix_web::http::header::AUTHORIZATION;
use actix_web::{web, FromRequest, HttpRequest};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::EtatApplication;

/// Utilisateur dont le jeton d'accès a été vérifié.
///
/// Ne porte que l'identifiant : tout le reste se lit en base au moment où on en
/// a besoin. Un jeton émis il y a cinquante-neuf minutes ne dit rien de l'état
/// courant du compte, et s'y fier laisserait agir quelqu'un dont le compte
/// vient d'être verrouillé ou effacé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Authentifie {
    pub utilisateur_id: Uuid,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ErreurAuthDto {
    /// `AUTH_MISSING` ou `AUTH_INVALID`.
    pub code: &'static str,
}

/// Refus d'authentification, rendu en `401` avec un corps stable.
#[derive(Debug)]
pub struct RefusAuth {
    code: &'static str,
}

impl std::fmt::Display for RefusAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code)
    }
}

impl actix_web::ResponseError for RefusAuth {
    fn status_code(&self) -> actix_web::http::StatusCode {
        actix_web::http::StatusCode::UNAUTHORIZED
    }

    fn error_response(&self) -> actix_web::HttpResponse {
        actix_web::HttpResponse::Unauthorized()
            // `WWW-Authenticate` : c'est cet en-tête, et non le corps, qu'un
            // client HTTP générique lit pour savoir quoi présenter.
            .insert_header(("WWW-Authenticate", "Bearer"))
            .json(ErreurAuthDto { code: self.code })
    }
}

/// Extrait le jeton d'un en-tête `Authorization: Bearer …`.
fn jeton_porteur(requete: &HttpRequest) -> Option<&str> {
    let brut = requete.headers().get(AUTHORIZATION)?.to_str().ok()?;
    // Comparaison insensible à la casse sur le schéma : la RFC 7235 le déclare
    // insensible, et certains clients envoient « bearer ».
    let (schema, valeur) = brut.split_once(' ')?;
    if !schema.eq_ignore_ascii_case("Bearer") {
        return None;
    }
    let valeur = valeur.trim();
    (!valeur.is_empty()).then_some(valeur)
}

impl FromRequest for Authentifie {
    type Error = RefusAuth;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(requete: &HttpRequest, _: &mut actix_web::dev::Payload) -> Self::Future {
        let Some(jeton) = jeton_porteur(requete) else {
            return ready(Err(RefusAuth {
                code: "AUTH_MISSING",
            }));
        };

        let Some(etat) = requete.app_data::<web::Data<EtatApplication>>() else {
            // Ne peut arriver que si l'application est montée sans son état,
            // c'est-à-dire jamais en dehors d'un test mal construit. Refuser
            // vaut mieux que laisser passer.
            return ready(Err(RefusAuth {
                code: "AUTH_INVALID",
            }));
        };

        // Signature invalide et jeton périmé donnent le même refus : les
        // distinguer apprendrait à un attaquant que sa forgerie est bien
        // formée, ce qui est précisément ce qu'il cherche à savoir.
        match etat.jetons.verifier(jeton) {
            Ok(claims) => ready(Ok(Authentifie {
                utilisateur_id: claims.utilisateur_id,
            })),
            Err(_) => ready(Err(RefusAuth {
                code: "AUTH_INVALID",
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;

    fn avec_entete(valeur: &str) -> HttpRequest {
        TestRequest::default()
            .insert_header((AUTHORIZATION, valeur))
            .to_http_request()
    }

    #[test]
    fn happy_lit_un_bearer_bien_forme() {
        assert_eq!(
            jeton_porteur(&avec_entete("Bearer abc.def.ghi")),
            Some("abc.def.ghi")
        );
    }

    #[test]
    fn edge_le_schema_est_insensible_a_la_casse() {
        // RFC 7235 §2.1. Certains clients envoient « bearer » en minuscules.
        for schema in ["Bearer", "bearer", "BEARER", "BeArEr"] {
            assert_eq!(
                jeton_porteur(&avec_entete(&format!("{schema} jeton"))),
                Some("jeton"),
                "schéma {schema}"
            );
        }
    }

    #[test]
    fn negative_refuse_un_en_tete_absent_ou_vide() {
        assert_eq!(
            jeton_porteur(&TestRequest::default().to_http_request()),
            None
        );
        assert_eq!(jeton_porteur(&avec_entete("Bearer")), None);
        assert_eq!(jeton_porteur(&avec_entete("Bearer ")), None);
        assert_eq!(jeton_porteur(&avec_entete("Bearer    ")), None);
    }

    #[test]
    fn negative_refuse_un_autre_schema() {
        // `Basic` porte des identifiants en clair : l'accepter ici ferait
        // passer un mot de passe pour un jeton.
        assert_eq!(jeton_porteur(&avec_entete("Basic bWFyaWU6bWRw")), None);
        assert_eq!(jeton_porteur(&avec_entete("Digest abc")), None);
        assert_eq!(jeton_porteur(&avec_entete("abc.def.ghi")), None);
    }

    #[test]
    fn security_le_jeton_n_est_pas_repris_dans_le_refus() {
        // Un message d'erreur qui répète le jeton le fait entrer dans les
        // journaux du client et dans les rapports d'anomalie.
        let refus = RefusAuth {
            code: "AUTH_INVALID",
        };
        assert_eq!(refus.to_string(), "AUTH_INVALID");
    }
}
