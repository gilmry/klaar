//! Inscription (Story 1.1, FR-001).

use actix_web::{post, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use klaar_application::ports::horloge::Horloge;
use klaar_application::usecases::inscrire_utilisateur::{inscrire, CommandeInscription};

use crate::limitation::{Verdict, FENETRE_SECONDES};
use crate::EtatApplication;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct InscriptionDto {
    pub email: String,
    /// Jamais journalisé, jamais renvoyé. Le domaine le convertit en
    /// `MotDePasse`, un type qui ne sait pas s'afficher.
    pub mot_de_passe: String,
    /// `fr`, `nl` ou `en`. Toute autre valeur, ou l'absence de valeur, donne
    /// `fr` sans que l'inscription échoue.
    #[serde(default)]
    pub locale: Option<String>,
}

/// Réponse unique de l'inscription.
///
/// Un seul code, quelle que soit l'issue. Renvoyer « compte créé » d'un côté
/// et « adresse déjà prise » de l'autre suffirait à énumérer les comptes, ce
/// que FR-001 `@security` interdit — voir l'arbitrage documenté sur le cas
/// d'usage.
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct InscriptionAccepteeDto {
    /// Toujours `SIGNUP_ACCEPTED`.
    pub code: &'static str,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ErreurValidationDto {
    /// Code stable : `EMAIL_EMPTY`, `EMAIL_MALFORMED`, `PASSWORD_EMPTY`,
    /// `PASSWORD_TOO_SHORT`, `PASSWORD_TOO_LONG`, `RATE_LIMIT_EXCEEDED`.
    pub code: String,
}

/// Adresse à laquelle imputer la tentative.
///
/// `X-Forwarded-For` n'est cru que si le déploiement déclare être derrière un
/// proxy de confiance. Le croire par défaut donnerait à n'importe qui le moyen
/// de contourner la limitation en changeant un en-tête, ce qui reviendrait à
/// ne pas en avoir.
pub fn adresse_source(requete: &HttpRequest, derriere_proxy: bool) -> String {
    let info = requete.connection_info();
    let adresse = if derriere_proxy {
        info.realip_remote_addr()
    } else {
        info.peer_addr()
    };
    adresse.unwrap_or("inconnue").to_string()
}

/// Crée un compte utilisateur.
#[utoipa::path(
    post,
    path = "/api/v1/auth/signup",
    tag = "authentification",
    request_body = InscriptionDto,
    responses(
        (status = 202, description = "Demande acceptée. Ne dit pas si un compte a été créé.", body = InscriptionAccepteeDto),
        (status = 400, description = "Saisie invalide", body = ErreurValidationDto),
        (status = 429, description = "Trop de tentatives depuis cette adresse", body = ErreurValidationDto),
        (status = 503, description = "Service indisponible", body = ErreurValidationDto),
    )
)]
#[post("/api/v1/auth/signup")]
pub async fn signup(
    requete: HttpRequest,
    etat: web::Data<EtatApplication>,
    corps: web::Json<InscriptionDto>,
) -> HttpResponse {
    let maintenant = etat.horloge.maintenant();
    // Préfixé : inscription et connexion ont chacune leur budget, sinon cinq
    // inscriptions épuisent les tentatives de connexion de la même adresse.
    let source = format!("signup:{}", adresse_source(&requete, etat.derriere_proxy));

    // Contrôlé avant tout travail : le hachage argon2 coûte de la mémoire et
    // du temps par construction, et les faire dépenser sans limite est
    // précisément ce que la limitation prévient.
    if let Verdict::Refuse { retry_after } = etat.limiteur.verifier(&source, maintenant) {
        return HttpResponse::TooManyRequests()
            .insert_header(("Retry-After", retry_after.to_string()))
            .json(ErreurValidationDto {
                code: "RATE_LIMIT_EXCEEDED".to_string(),
            });
    }

    let commande = CommandeInscription {
        email: corps.email.clone(),
        mot_de_passe: corps.mot_de_passe.clone(),
        locale: corps.locale.clone(),
    };

    match inscrire(
        etat.utilisateurs.as_ref(),
        etat.courriel.as_ref(),
        etat.journal.as_ref(),
        etat.horloge.as_ref(),
        etat.argon2,
        commande,
    )
    .await
    {
        // Les deux issues du cas d'usage donnent la même réponse. C'est
        // volontaire, et c'est tout l'objet de l'arbitrage.
        Ok(_) => HttpResponse::Accepted().json(InscriptionAccepteeDto {
            code: "SIGNUP_ACCEPTED",
        }),
        Err(e) if e.est_saisie_invalide() => HttpResponse::BadRequest().json(ErreurValidationDto {
            code: e.code().to_string(),
        }),
        Err(e) => {
            tracing::error!(erreur = %e, "inscription impossible");
            HttpResponse::ServiceUnavailable().json(ErreurValidationDto {
                code: e.code().to_string(),
            })
        }
    }
}

/// Exposé pour la documentation : la fenêtre annoncée dans `Retry-After`.
pub const FENETRE_LIMITATION_SECONDES: i64 = FENETRE_SECONDES;
