//! Connexion (Story 1.3, FR-004).

use actix_web::cookie::{time::Duration as DureeCookie, Cookie, SameSite};
use actix_web::{post, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use klaar_application::ports::horloge::Horloge;
use klaar_application::usecases::connecter::{connecter, CommandeConnexion, ErreurConnexion};
use klaar_application::usecases::rafraichir::{deconnecter, rafraichir, ErreurRafraichissement};
use klaar_identity::JetonVerification;

use crate::limitation::Verdict;
use crate::routes::auth::{adresse_source, ErreurValidationDto};
use crate::EtatApplication;

/// Nom du cookie de rafraîchissement.
pub const COOKIE_REFRESH: &str = "klaar_refresh";

/// Chemin du cookie.
///
/// Restreint aux routes d'authentification, et non `/` : un cookie envoyé à
/// chaque appel d'API l'expose à toute faille de l'une d'elles, alors que seul
/// `/auth/refresh` en a besoin.
pub const CHEMIN_COOKIE: &str = "/api/v1/auth";

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnexionDto {
    pub email: String,
    pub mot_de_passe: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionOuverteDto {
    /// JWT à placer dans l'en-tête `Authorization: Bearer`.
    pub jeton_acces: String,
    /// Durée de validité de l'accès, en secondes.
    pub expire_dans: i64,
}

/// Construit le cookie de rafraîchissement.
///
/// `HttpOnly` : le refresh n'est jamais lisible par JavaScript, donc une faille
/// XSS ne l'emporte pas. `SameSite=Lax` : il n'accompagne pas une requête
/// déclenchée par un autre site. `Secure` : il ne part pas en clair — désactivable
/// pour le développement local en HTTP, jamais ailleurs.
fn cookie_refresh(valeur: &str, duree_secondes: i64, securise: bool) -> Cookie<'static> {
    Cookie::build(COOKIE_REFRESH, valeur.to_string())
        .path(CHEMIN_COOKIE)
        .http_only(true)
        .secure(securise)
        .same_site(SameSite::Lax)
        .max_age(DureeCookie::seconds(duree_secondes))
        .finish()
}

/// Agent utilisateur, quand le client en annonce un.
///
/// Sert uniquement à lier le refresh à son contexte d'obtention (Story 1.4). Il
/// n'est ni journalisé ni conservé : seule son empreinte l'est, et à cette
/// seule fin.
fn contexte(requete: &HttpRequest) -> Option<String> {
    requete
        .headers()
        .get(actix_web::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

/// Refresh présenté par le client, lu dans son cookie.
fn refresh_presente(requete: &HttpRequest) -> String {
    requete
        .cookie(COOKIE_REFRESH)
        .map(|c| c.value().to_string())
        .unwrap_or_default()
}

/// Cookie de suppression : même nom, même chemin, durée nulle.
///
/// Le chemin doit être identique à celui de la pose, sinon le navigateur
/// considère qu'il s'agit d'un autre cookie et garde l'ancien.
fn cookie_efface(securise: bool) -> Cookie<'static> {
    Cookie::build(COOKIE_REFRESH, "")
        .path(CHEMIN_COOKIE)
        .http_only(true)
        .secure(securise)
        .same_site(SameSite::Lax)
        .max_age(DureeCookie::seconds(0))
        .finish()
}

/// Ouvre une session.
#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    tag = "authentification",
    request_body = ConnexionDto,
    responses(
        (status = 200, description = "Session ouverte ; refresh posé en cookie", body = SessionOuverteDto),
        (status = 400, description = "Saisie invalide", body = ErreurValidationDto),
        (status = 401, description = "Identifiants invalides", body = ErreurValidationDto),
        (status = 403, description = "Compte non vérifié", body = ErreurValidationDto),
        (status = 429, description = "Trop de tentatives depuis cette adresse", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    )
)]
#[post("/api/v1/auth/login")]
pub async fn login(
    requete: HttpRequest,
    etat: web::Data<EtatApplication>,
    corps: web::Json<ConnexionDto>,
) -> HttpResponse {
    let maintenant = etat.horloge.maintenant();
    // Compteur distinct de celui de l'inscription : sans le préfixe, cinq
    // inscriptions épuiseraient le budget de connexion de la même adresse, et
    // le lien entre les deux serait incompréhensible pour l'utilisateur.
    let source = format!("login:{}", adresse_source(&requete, etat.derriere_proxy));

    if let Verdict::Refuse { retry_after } = etat.limiteur.verifier(&source, maintenant) {
        return HttpResponse::TooManyRequests()
            .insert_header(("Retry-After", retry_after.to_string()))
            .json(ErreurValidationDto {
                code: "RATE_LIMIT_EXCEEDED".to_string(),
            });
    }

    // Tiré ici, dans la couche qui sait ce qu'est un cookie : le cas d'usage
    // reçoit la valeur et n'a pas à connaître le générateur.
    // Nommé `nouveau` et non `refresh` : la macro `#[post("…/refresh")]` plus
    // bas engendre une struct unitaire `refresh`, qui masquerait cette liaison.
    let nouveau = JetonVerification::tirer();

    match connecter(
        etat.utilisateurs.as_ref(),
        etat.sessions.as_ref(),
        etat.journal.as_ref(),
        etat.horloge.as_ref(),
        etat.jetons.as_ref(),
        etat.courriel.as_ref(),
        etat.argon2,
        nouveau.expose(),
        contexte(&requete).as_deref(),
        CommandeConnexion {
            email: corps.email.clone(),
            mot_de_passe: corps.mot_de_passe.clone(),
        },
    )
    .await
    {
        Ok(session) => HttpResponse::Ok()
            .cookie(cookie_refresh(
                &session.refresh,
                session.refresh_expire_dans_secondes,
                etat.cookie_securise,
            ))
            .json(SessionOuverteDto {
                jeton_acces: session.jeton_acces,
                expire_dans: session.expire_dans_secondes,
            }),
        // 423 Locked, avec le délai réel restant. Ce refus n'est atteignable
        // qu'avec le bon mot de passe : il ne révèle donc rien à qui ne le
        // connaît pas — voir l'arbitrage sur le cas d'usage.
        Err(ErreurConnexion::CompteVerrouille { retry_after }) => {
            HttpResponse::build(actix_web::http::StatusCode::LOCKED)
                .insert_header(("Retry-After", retry_after.to_string()))
                .json(ErreurValidationDto {
                    code: "ACCOUNT_LOCKED".to_string(),
                })
        }
        Err(e) => {
            let statut = match &e {
                ErreurConnexion::Email(_) | ErreurConnexion::MotDePasse(_) => {
                    actix_web::http::StatusCode::BAD_REQUEST
                }
                ErreurConnexion::CompteVerrouille { .. } => unreachable!("traité au-dessus"),
                ErreurConnexion::IdentifiantsInvalides => actix_web::http::StatusCode::UNAUTHORIZED,
                // 403 et non 401 : les identifiants sont bons, c'est l'état du
                // compte qui bloque. Un 401 inviterait le client à redemander
                // un mot de passe qui est déjà le bon.
                ErreurConnexion::CompteNonVerifie => actix_web::http::StatusCode::FORBIDDEN,
                ErreurConnexion::Indisponible(_) => {
                    tracing::error!(erreur = %e, "connexion impossible");
                    actix_web::http::StatusCode::SERVICE_UNAVAILABLE
                }
            };
            HttpResponse::build(statut).json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    }
}

/// Rafraîchit la session à partir du cookie de refresh.
#[utoipa::path(
    post,
    path = "/api/v1/auth/refresh",
    tag = "authentification",
    responses(
        (status = 200, description = "Session renouvelée ; nouveau refresh posé", body = SessionOuverteDto),
        (status = 400, description = "Aucun refresh présenté", body = ErreurValidationDto),
        (status = 401, description = "Refresh invalide, expiré, révoqué ou rejoué", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    )
)]
#[post("/api/v1/auth/refresh")]
pub async fn refresh(requete: HttpRequest, etat: web::Data<EtatApplication>) -> HttpResponse {
    let nouveau = JetonVerification::tirer();

    match rafraichir(
        etat.sessions.as_ref(),
        etat.journal.as_ref(),
        etat.horloge.as_ref(),
        etat.jetons.as_ref(),
        &refresh_presente(&requete),
        nouveau.expose(),
        contexte(&requete).as_deref(),
    )
    .await
    {
        Ok(session) => HttpResponse::Ok()
            .cookie(cookie_refresh(
                &session.refresh,
                session.refresh_expire_dans_secondes,
                etat.cookie_securise,
            ))
            .json(SessionOuverteDto {
                jeton_acces: session.jeton_acces,
                expire_dans: session.expire_dans_secondes,
            }),
        Err(e) => {
            let statut = match &e {
                ErreurRafraichissement::Absent => actix_web::http::StatusCode::BAD_REQUEST,
                ErreurRafraichissement::Indisponible(_) => {
                    tracing::error!(erreur = %e, "rafraîchissement impossible");
                    actix_web::http::StatusCode::SERVICE_UNAVAILABLE
                }
                _ => actix_web::http::StatusCode::UNAUTHORIZED,
            };
            // Le cookie est effacé sur tout refus : le garder ferait rejouer le
            // même jeton mort à chaque tentative, et un rejeu suffit à couper
            // la famille. Autant que le client reparte propre.
            HttpResponse::build(statut)
                .cookie(cookie_efface(etat.cookie_securise))
                .json(ErreurValidationDto {
                    code: e.code().to_string(),
                })
        }
    }
}

/// Ferme la session courante et toute sa famille.
#[utoipa::path(
    post,
    path = "/api/v1/auth/logout",
    tag = "authentification",
    responses(
        (status = 204, description = "Session fermée, ou déjà absente"),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    )
)]
#[post("/api/v1/auth/logout")]
pub async fn logout(requete: HttpRequest, etat: web::Data<EtatApplication>) -> HttpResponse {
    match deconnecter(
        etat.sessions.as_ref(),
        etat.journal.as_ref(),
        etat.horloge.as_ref(),
        &refresh_presente(&requete),
    )
    .await
    {
        // 204 que la session ait existé ou non : répondre 404 sur un refresh
        // inconnu ferait de la déconnexion un moyen de tester la validité d'un
        // jeton volé.
        Ok(()) => HttpResponse::NoContent()
            .cookie(cookie_efface(etat.cookie_securise))
            .finish(),
        Err(e) => {
            tracing::error!(erreur = %e, "déconnexion impossible");
            HttpResponse::ServiceUnavailable().json(ErreurValidationDto {
                code: "SERVICE_UNAVAILABLE".to_string(),
            })
        }
    }
}
