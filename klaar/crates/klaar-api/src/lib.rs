//! API HTTP de Klaar (actix-web + utoipa — ADR-003, ADR-004).
//!
//! L'application est construite ici, et non dans `main`, pour que les tests
//! puissent la monter en mémoire avec un état choisi. Un endpoint testé
//! seulement en lançant un vrai serveur finit par n'être testé que dans les
//! cas nominaux.

use std::sync::{Arc, OnceLock};

use actix_web::dev::ResourceDef;
use actix_web::{web, App, HttpRequest, HttpResponse};
use utoipa::OpenApi;

use klaar_application::ports::horloge::HorlogeSysteme;
use klaar_application::ports::jeton_acces::EmetteurJetonAcces;
use klaar_application::usecases::soumettre_demande::ReglesSoumission;
use klaar_email_adapter::{Courriel, CourrielJournalise};
use klaar_identity::ParametresArgon2;
use klaar_push_adapter::WebPushSender;
use klaar_sqlx_repos::{
    PgAnnulationRepository, PgCatalogueAdminRepository, PgCatalogueRepository, PgDemandeRepository,
    PgDevisRepository, PgEvenementStripeRepository, PgExportRepository, PgJournalAudit,
    PgLiberationRepository, PgLitigeRepository, PgMessageRepository, PgMissionRepository,
    PgNotationRepository, PgOpsRepository, PgPaiementRepository, PgProviderRepository,
    PgPushSubscriptionRepository, PgReprogrammationRepository, PgRevueKycRepository,
    PgSessionRepository, PgSuiviRepository, PgTableauBordRepository, PgTraceRepository,
    PgUtilisateurRepository,
};

pub mod auth;
pub mod billet;
pub mod evenements;
pub mod jwt;
pub mod limitation;
pub mod routes;
pub mod telemetry;

use limitation::{LimiteurMemoire, Quota};

/// Dépendances partagées par les handlers.
///
/// `Clone` pour que les tests dérivent un état complet en ne changeant qu'un
/// champ (`EtatApplication { catalogue_en_maintenance: true, ..base.clone() }`).
/// Sans lui, chaque nouveau champ oblige à réénumérer toute la structure dans
/// chaque test qui en construit un — et le premier oubli passe pour une erreur
/// de compilation dans un fichier sans rapport.
#[derive(Clone)]
pub struct EtatApplication {
    pub abonnements: Arc<PgPushSubscriptionRepository>,
    /// `None` quand aucune clé VAPID n'est configurée : le déploiement tourne
    /// alors sans notifications, ce qui est un mode de fonctionnement légitime
    /// et non une panne.
    pub push: Option<Arc<WebPushSender>>,
    pub utilisateurs: Arc<PgUtilisateurRepository>,
    pub journal: Arc<PgJournalAudit>,
    pub sessions: Arc<PgSessionRepository>,
    pub catalogue: Arc<PgCatalogueRepository>,
    pub demandes: Arc<PgDemandeRepository>,
    pub paiements: Arc<PgPaiementRepository>,
    pub prestataires: Arc<PgProviderRepository>,
    pub traces: Arc<PgTraceRepository>,
    pub missions: Arc<PgMissionRepository>,
    pub devis: Arc<PgDevisRepository>,
    pub liberations: Arc<PgLiberationRepository>,
    pub annulations: Arc<PgAnnulationRepository>,
    pub notations: Arc<PgNotationRepository>,
    pub messages: Arc<PgMessageRepository>,
    pub litiges: Arc<PgLitigeRepository>,
    pub ops: Arc<PgOpsRepository>,
    pub exports: Arc<PgExportRepository>,
    pub reprogrammations: Arc<PgReprogrammationRepository>,
    /// Suivi géolocalisé du trajet (Story 4.4, FR-019).
    pub suivis: Arc<PgSuiviRepository>,
    /// Indicateurs d'exploitation (Story 8.3, FR-040).
    pub tableau_bord: Arc<PgTableauBordRepository>,
    /// Revue du contrôle d'entreprise (Story 8.1, FR-038).
    pub revues_kyc: Arc<PgRevueKycRepository>,
    /// Journal des webhooks Stripe (Story 5.5, FR-028).
    pub evenements_stripe: Arc<PgEvenementStripeRepository>,
    /// Administration du catalogue (Story 2.4, FR-010).
    pub catalogue_admin: Arc<PgCatalogueAdminRepository>,
    /// Secret de signature du webhook Stripe.
    ///
    /// **`None` ferme l'endpoint plutôt que de l'ouvrir.** Sans secret il n'y a
    /// rien à vérifier ; en déduire qu'on peut tout accepter ferait d'une
    /// configuration oubliée une porte ouverte sur des écritures d'argent.
    pub secret_webhook_stripe: Option<String>,
    /// Diffusion temps réel des événements de Mission (Story 4.9).
    pub evenements: crate::evenements::BusEvenements,
    /// Billets d'ouverture de socket, à usage unique et de courte vie.
    pub billets: Arc<crate::billet::BilletsMemoire>,
    /// Signataire du jeton d'accès. Derrière un trait : le format du jeton
    /// est remplaçable sans toucher aux cas d'usage.
    pub jetons: Arc<dyn EmetteurJetonAcces>,
    pub courriel: Arc<Courriel>,
    pub horloge: Arc<HorlogeSysteme>,
    pub limiteur: Arc<LimiteurMemoire>,
    /// Paramètres argon2id. Injectés plutôt que lus depuis `production()` pour
    /// que les tests d'intégration ne passent pas l'essentiel de leur temps
    /// dans une fonction de dérivation volontairement lente.
    pub argon2: ParametresArgon2,
    /// Faire confiance à `X-Forwarded-For`. Faux par défaut : le croire sans
    /// proxy de confiance devant rend la limitation de débit contournable par
    /// un simple en-tête.
    pub derriere_proxy: bool,
    /// Poser l'attribut `Secure` sur le cookie de rafraîchissement. Vrai
    /// partout sauf en développement local sur HTTP, où le navigateur
    /// refuserait alors le cookie sans rien dire.
    pub cookie_securise: bool,
    /// Retire temporairement le catalogue (FR-008 `@edge`). Le service
    /// répond alors 503 avec `Retry-After`, ce qui distingue un retrait
    /// volontaire d'une panne.
    pub catalogue_en_maintenance: bool,
    /// Plafond des écritures sensibles par adresse et par heure (FR-001).
    ///
    /// Cinq en temps normal. Le déploiement de démonstration le relève, parce
    /// que plusieurs parcours filmés se connectent depuis la même adresse en
    /// quelques minutes. Injecté plutôt que lu d'une constante pour que ce
    /// desserrage soit un choix visible, et non un `if` caché dans une route.
    pub quota_ecriture_sensible: Quota,
    /// Ce que le déploiement autorise à la soumission d'une Demande (FR-011) :
    /// méthode de paiement exigée ou non, quota horaire par compte.
    ///
    /// Groupé plutôt qu'égrené : ces réglages voyagent ensemble, et une suite
    /// de booléens finit par se remplir dans le mauvais ordre.
    pub regles_soumission: ReglesSoumission,
}

#[derive(OpenApi)]
#[openapi(
    paths(
        routes::health::health,
        routes::auth::signup,
        routes::verification::verifier,
        routes::session::login,
        routes::session::refresh,
        routes::session::logout,
        routes::compte::effacer_mon_compte,
        routes::compte::annuler_mon_effacement,
        routes::catalogue::secteurs,
        routes::demande::soumettre_demande,
        routes::acceptation::accepter_demande,
        routes::elargissement::elargir_rayon,
        routes::annulation::annuler_demande,
        routes::disponibilite::lire_disponibilite,
        routes::disponibilite::regler_disponibilite,
        routes::mission::avancer_mission,
        routes::devis::envoyer_devis,
        routes::devis::accepter_devis,
        routes::devis::refuser_devis,
        routes::validation::valider_mission,
        routes::annulation_mission::annuler_intervention,
        routes::notation::noter_intervention,
        routes::notation::lire_notes,
        routes::conversation::envoyer_message,
        routes::conversation::lire_conversation,
        routes::litige::ouvrir_litige,
        routes::litige::lire_litige,
        routes::langue::changer_langue,
        routes::ops::connexion_ops,
        routes::ops::creer_compte_ops,
        routes::ops::lire_audit,
        routes::ops::export_rgpd,
        routes::ops::export_tva,
        routes::reprogrammation::proposer_reprogrammation,
        routes::reprogrammation::repondre_reprogrammation,
        routes::ops::lire_tableau_bord,
        routes::ops::deconnexion_ops,
        routes::ops::file_litiges,
        routes::ops::lire_litige,
        routes::ops::trancher_route,
        routes::ops::file_kyc,
        routes::ops::reviser_kyc,
        routes::ops::lister_secteurs,
        routes::ops::creer_secteur,
        routes::ops::publier_secteur,
        routes::ops::desactiver_secteur,
        routes::disponibilite::retirer_inscription,
        routes::webhook_stripe::recevoir_webhook,
        routes::suivi_position::consentir_suivi,
        routes::suivi_position::relever_suivi,
        routes::suivi_position::consulter_suivi,
        routes::temps_reel::demander_billet,
        routes::temps_reel::suivre_en_direct,
        routes::suivi::suivre_demande,
        routes::suivi::demandes_recues,
        routes::suivi::suivre_mission,
        routes::push::cle_publique,
        routes::push::enregistrer_abonnement,
        routes::push::supprimer_abonnement,
    ),
    components(schemas(
        routes::health::HealthDto,
        routes::auth::InscriptionDto,
        routes::auth::InscriptionAccepteeDto,
        routes::auth::ErreurValidationDto,
        routes::verification::VerificationDto,
        routes::verification::VerificationFaiteDto,
        routes::session::ConnexionDto,
        routes::session::SessionOuverteDto,
        routes::compte::EffacementDto,
        routes::compte::EffacementProgrammeDto,
        crate::auth::ErreurAuthDto,
        routes::catalogue::CatalogueDto,
        routes::catalogue::SecteurDto,
        routes::catalogue::SkillDto,
        routes::catalogue::FourchetteDto,
        routes::demande::DemandeDto,
        routes::demande::DemandeCreeeDto,
        routes::acceptation::MissionDto,
        routes::elargissement::ElargissementDto,
        routes::annulation::AnnulationDto,
        routes::disponibilite::DisponibiliteDto,
        routes::disponibilite::ReglageDto,
        routes::mission::TransitionDto,
        routes::mission::MissionAvanceeDto,
        routes::devis::PropositionDto,
        routes::devis::DevisEmisDto,
        routes::devis::RefusDto,
        routes::devis::ReponseDevisDto,
        routes::validation::LiberationDto,
        routes::annulation_mission::AnnulationMissionDto,
        routes::annulation_mission::MissionAnnuleeDto,
        routes::notation::NoteDto,
        routes::notation::NoteEcriteDto,
        routes::notation::NoteVisibleDto,
        routes::notation::NotesDeMissionDto,
        routes::conversation::MessageDto,
        routes::conversation::MessageEnvoyeDto,
        routes::conversation::MessageLuDto,
        routes::conversation::FilDto,
        routes::conversation::RefusCoordonneesDto,
        routes::litige::LitigeDto,
        routes::litige::LitigeOuvertDto,
        routes::litige::LitigeLuDto,
        routes::langue::LangueDto,
        routes::langue::LangueChoisieDto,
        routes::ops::ConnexionOpsDto,
        routes::ops::SessionOpsDto,
        routes::ops::CreationOpsDto,
        routes::ops::CompteOpsCreeDto,
        routes::ops::GesteOpsDto,
        routes::ops::JournalOpsDto,
        routes::ops::ExportRgpdDto,
        routes::reprogrammation::ReprogrammationDto,
        routes::reprogrammation::ReponseReprogrammationDto,
        routes::reprogrammation::RepriseDto,
        routes::ops::TableauBordDto,
        routes::ops::DossierLitigeDto,
        routes::ops::FileMediationDto,
        routes::ops::DecisionDto,
        routes::ops::IssueAdmise,
        routes::ops::IssueDto,
        routes::ops::DossierKycDto,
        routes::ops::RefusEnAttenteDto,
        routes::ops::FileKycDto,
        routes::ops::DecisionKycDto,
        routes::ops::IssueRevueDto,
        routes::ops::SecteurAdminDto,
        routes::ops::CatalogueAdminDto,
        routes::ops::CreationSecteurDto,
        routes::webhook_stripe::AccuseWebhookDto,
        routes::suivi_position::ConsentementSuiviDto,
        routes::suivi_position::EtatConsentementDto,
        routes::suivi_position::PositionDto,
        routes::suivi_position::ReleveDto,
        routes::suivi_position::VueSuiviDto,
        routes::temps_reel::BilletDto,
        routes::suivi::SuiviDemandeDto,
        routes::suivi::DemandeProposeeDto,
        routes::suivi::SuiviMissionDto,
        routes::suivi::DevisDto,
        routes::push::ClePubliqueDto,
        routes::push::AbonnementDto,
        routes::push::ClesAbonnementDto,
        routes::push::AbonnementEnregistreDto,
        routes::push::DesabonnementDto,
        routes::push::ErreurDto,
    )),
    tags(
        (name = "sonde", description = "Disponibilité du service"),
        (name = "authentification", description = "Comptes et sessions (FR-001 à FR-004)"),
        (name = "compte", description = "Compte de l'utilisateur authentifié (FR-005)"),
        (name = "catalogue", description = "Secteurs et Skills (FR-008)"),
        (name = "demandes", description = "Demandes de dépannage (FR-011 à FR-015)"),
        (name = "prestataires", description = "Disponibilité et rayon d\'intervention (FR-003)"),
        (name = "missions", description = "Cycle de vie d\'une intervention (FR-018)"),
        (name = "devis", description = "Devis du prestataire (FR-016)"),
        (name = "exploitation", description = "Console ops : rôles, MFA, journal (FR-041, FR-042)"),
        (name = "litige", description = "Recours après intervention (FR-034)"),
        (name = "conversation", description = "Messagerie entre les deux parties (FR-030)"),
        (name = "notation", description = "Notation double sens (FR-033)"),
        (name = "temps-réel", description = "Flux d'événements d'une Mission (Story 4.9)"),
        (name = "push", description = "Abonnements Web Push (ADR-010)"),
    )
)]
pub struct ApiDoc;

/// Enregistre toutes les routes. Séparé de la construction de `App` pour être
/// réutilisable par `actix_web::test::init_service`.
pub fn configurer(cfg: &mut web::ServiceConfig) {
    cfg.app_data(json_config())
        .app_data(query_config())
        .app_data(path_config())
        .service(routes::health::health)
        .service(routes::auth::signup)
        .service(routes::verification::verifier)
        .service(routes::session::login)
        .service(routes::session::refresh)
        .service(routes::session::logout)
        .service(routes::compte::effacer_mon_compte)
        .service(routes::compte::annuler_mon_effacement)
        .service(routes::catalogue::secteurs)
        .service(routes::demande::soumettre_demande)
        .service(routes::acceptation::accepter_demande)
        .service(routes::elargissement::elargir_rayon)
        .service(routes::annulation::annuler_demande)
        .service(routes::disponibilite::lire_disponibilite)
        .service(routes::disponibilite::regler_disponibilite)
        .service(routes::mission::avancer_mission)
        .service(routes::devis::envoyer_devis)
        .service(routes::devis::accepter_devis)
        .service(routes::devis::refuser_devis)
        .service(routes::validation::valider_mission)
        .service(routes::annulation_mission::annuler_intervention)
        .service(routes::notation::noter_intervention)
        .service(routes::notation::lire_notes)
        .service(routes::conversation::envoyer_message)
        .service(routes::conversation::lire_conversation)
        .service(routes::litige::ouvrir_litige)
        .service(routes::litige::lire_litige)
        .service(routes::langue::changer_langue)
        .service(routes::ops::connexion_ops)
        .service(routes::ops::creer_compte_ops)
        .service(routes::ops::lire_audit)
        .service(routes::ops::export_rgpd)
        .service(routes::ops::export_tva)
        .service(routes::reprogrammation::proposer_reprogrammation)
        .service(routes::reprogrammation::repondre_reprogrammation)
        .service(routes::ops::lire_tableau_bord)
        .service(routes::ops::deconnexion_ops)
        .service(routes::ops::file_litiges)
        .service(routes::ops::lire_litige)
        .service(routes::ops::trancher_route)
        .service(routes::ops::file_kyc)
        .service(routes::ops::reviser_kyc)
        .service(routes::ops::lister_secteurs)
        .service(routes::ops::creer_secteur)
        .service(routes::ops::publier_secteur)
        .service(routes::ops::desactiver_secteur)
        .service(routes::disponibilite::retirer_inscription)
        .service(routes::webhook_stripe::recevoir_webhook)
        .service(routes::suivi_position::consentir_suivi)
        .service(routes::suivi_position::relever_suivi)
        .service(routes::suivi_position::consulter_suivi)
        .service(routes::temps_reel::demander_billet)
        .service(routes::temps_reel::suivre_en_direct)
        .service(routes::suivi::suivre_demande)
        .service(routes::suivi::demandes_recues)
        .service(routes::suivi::suivre_mission)
        .service(routes::push::cle_publique)
        .service(routes::push::enregistrer_abonnement)
        .service(routes::push::supprimer_abonnement)
        .default_service(web::to(repli));
}

/// Réponse d'erreur du lecteur de corps JSON.
///
/// **Ce qu'il y avait avant.** Un corps illisible recevait la réponse par
/// défaut d'actix-web : `400` en `text/plain`, portant le message du
/// désérialiseur — « Json deserialize error: expected value at line 1 column
/// 1 ». Trois problèmes dans une seule réponse : le type de contenu contredit
/// le contrat, qui n'annonce que du JSON ; la forme `{"code": "…"}` que toutes
/// les autres erreurs respectent n'est pas tenue, donc le client a deux
/// analyseurs à écrire ; et le texte est celui d'une bibliothèque interne, en
/// anglais, sur une API dont les refus sont traduits.
///
/// **Le statut est conservé tel quel.** Un contenu de type inattendu mériterait
/// un `415`, mais le changer ici modifierait le contrat de routes déjà
/// documentées et testées. Ce qui est corrigé, c'est ce qui était faux : le
/// corps et son type.
fn json_config() -> web::JsonConfig {
    web::JsonConfig::default().error_handler(|erreur, _requete| {
        use actix_web::error::JsonPayloadError;
        use actix_web::ResponseError;
        let code = match &erreur {
            JsonPayloadError::ContentType => "CONTENT_TYPE_UNSUPPORTED",
            JsonPayloadError::Overflow { .. } | JsonPayloadError::OverflowKnownLength { .. } => {
                "BODY_TOO_LARGE"
            }
            _ => "BODY_MALFORMED",
        };
        let statut = erreur.status_code();
        actix_web::error::InternalError::from_response(
            erreur,
            HttpResponse::build(statut).json(routes::auth::ErreurValidationDto {
                code: code.to_string(),
            }),
        )
        .into()
    })
}

/// Même correction pour les paramètres de requête et de chemin.
///
/// Un paramètre manquant rendait « Query deserialize error: missing field
/// `debut` » en `text/plain` : le message du désérialiseur, en anglais, avec le
/// nom du champ Rust. Ici comme pour le corps, ce qui sort est la forme
/// d'erreur du contrat.
fn query_config() -> web::QueryConfig {
    web::QueryConfig::default()
        .error_handler(|erreur, _requete| erreur_de_lecture(erreur, "QUERY_MALFORMED"))
}

fn path_config() -> web::PathConfig {
    web::PathConfig::default()
        .error_handler(|erreur, _requete| erreur_de_lecture(erreur, "PATH_MALFORMED"))
}

fn erreur_de_lecture<E>(erreur: E, code: &str) -> actix_web::Error
where
    E: std::fmt::Debug + std::fmt::Display + 'static,
{
    actix_web::error::InternalError::from_response(
        erreur,
        HttpResponse::BadRequest().json(routes::auth::ErreurValidationDto {
            code: code.to_string(),
        }),
    )
    .into()
}

/// Les chemins du contrat, avec les méthodes que chacun accepte.
///
/// **Lus dans l'OpenAPI, pas recopiés.** Une seconde liste écrite à la main
/// aurait divergé au premier endpoint ajouté, et le repli aurait alors annoncé
/// `Allow` faux — pire qu'un 404, parce qu'on l'aurait cru.
fn contrat() -> &'static [(ResourceDef, String)] {
    static TABLE: OnceLock<Vec<(ResourceDef, String)>> = OnceLock::new();
    TABLE.get_or_init(|| {
        ApiDoc::openapi()
            .paths
            .paths
            .iter()
            .map(|(chemin, item)| {
                let methodes = [
                    ("GET", item.get.is_some()),
                    ("HEAD", item.head.is_some()),
                    ("POST", item.post.is_some()),
                    ("PUT", item.put.is_some()),
                    ("PATCH", item.patch.is_some()),
                    ("DELETE", item.delete.is_some()),
                    ("OPTIONS", item.options.is_some()),
                    ("TRACE", item.trace.is_some()),
                ]
                .into_iter()
                .filter(|(_, presente)| *presente)
                .map(|(nom, _)| nom)
                .collect::<Vec<_>>()
                .join(", ");
                (ResourceDef::new(chemin.as_str()), methodes)
            })
            .collect()
    })
}

/// Repli : 405 quand le chemin existe mais pas la méthode, 404 sinon.
///
/// **Pourquoi il faut l'écrire.** Le routage d'actix-web par macro
/// `#[get("…")]` crée une ressource par gestionnaire, chacune gardée par sa
/// méthode. Une méthode non déclarée ne satisfait aucune garde, aucune
/// ressource ne correspond, et le routeur rend **404**. HTTP demande **405**
/// assorti d'un en-tête `Allow` : la nuance n'est pas cosmétique, elle dit à
/// qui appelle « ce n'est pas l'adresse qui est fausse, c'est le verbe », ce
/// qui est la moitié du diagnostic.
///
/// Le fuzz de contrat (`schemathesis`, ADR-004) vérifie exactement cela. Son
/// check `unsupported_method` était exclu en CI, avec pour motif qu'il faudrait
/// s'en occuper « quand le contrat aura plusieurs endpoints par chemin » — ce
/// qui est le cas depuis longtemps : `GET`/`POST /missions/{id}/dispute`,
/// `GET`/`PATCH /providers/me/availability`, `GET`/`POST
/// /missions/{id}/tracking`, `GET`/`POST /ops/catalog/sectors`. L'exclusion est
/// levée avec ce repli.
async fn repli(requete: HttpRequest) -> HttpResponse {
    let chemin = requete.path();
    if let Some((_, autorisees)) = contrat().iter().find(|(def, _)| def.is_match(chemin)) {
        return HttpResponse::MethodNotAllowed()
            .insert_header((actix_web::http::header::ALLOW, autorisees.clone()))
            .json(routes::auth::ErreurValidationDto {
                code: "METHOD_NOT_ALLOWED".to_string(),
            });
    }
    HttpResponse::NotFound().json(routes::auth::ErreurValidationDto {
        code: "NOT_FOUND".to_string(),
    })
}

/// État câblé sur une base réelle, avec des paramètres argon2 faibles.
///
/// Vit à côté d'`app_de_test` pour la même raison : les tests d'intégration
/// sont une caisse séparée et ne voient pas les `#[cfg(test)]` de celle-ci. Les
/// paramètres argon2 de production coûteraient ici une centaine de
/// millisecondes par inscription, soit une suite qu'on finit par ne plus
/// lancer.
/// Secret de signature employé par les tests d'intégration du webhook.
///
/// Public exprès : un test doit pouvoir fabriquer une signature authentique,
/// et cacher la valeur l'obligerait à la recopier — donc à diverger le jour où
/// elle change.
pub const SECRET_WEBHOOK_DE_TEST: &str = "whsec_de_test_jamais_en_production";

pub fn etat_de_test(
    pool: klaar_sqlx_repos::PoolPg,
    push: Option<Arc<WebPushSender>>,
) -> web::Data<EtatApplication> {
    web::Data::new(EtatApplication {
        abonnements: Arc::new(PgPushSubscriptionRepository::new(pool.clone())),
        push,
        utilisateurs: Arc::new(PgUtilisateurRepository::new(pool.clone())),
        journal: Arc::new(PgJournalAudit::new(pool.clone())),
        sessions: Arc::new(PgSessionRepository::new(pool.clone())),
        catalogue: Arc::new(PgCatalogueRepository::new(pool.clone())),
        demandes: Arc::new(PgDemandeRepository::new(pool.clone())),
        paiements: Arc::new(PgPaiementRepository::new(pool.clone())),
        prestataires: Arc::new(PgProviderRepository::new(pool.clone())),
        traces: Arc::new(PgTraceRepository::new(pool.clone())),
        missions: Arc::new(PgMissionRepository::new(pool.clone())),
        devis: Arc::new(PgDevisRepository::new(pool.clone())),
        liberations: Arc::new(PgLiberationRepository::new(pool.clone())),
        annulations: Arc::new(PgAnnulationRepository::new(pool.clone())),
        notations: Arc::new(PgNotationRepository::new(pool.clone())),
        messages: Arc::new(PgMessageRepository::new(pool.clone())),
        litiges: Arc::new(PgLitigeRepository::new(pool.clone())),
        ops: Arc::new(PgOpsRepository::new(pool.clone())),
        exports: Arc::new(PgExportRepository::new(pool.clone())),
        reprogrammations: Arc::new(PgReprogrammationRepository::new(pool.clone())),
        suivis: Arc::new(PgSuiviRepository::new(pool.clone())),
        tableau_bord: Arc::new(PgTableauBordRepository::new(pool.clone())),
        revues_kyc: Arc::new(PgRevueKycRepository::new(pool.clone())),
        evenements_stripe: Arc::new(PgEvenementStripeRepository::new(pool.clone())),
        catalogue_admin: Arc::new(PgCatalogueAdminRepository::new(pool)),
        // Un secret fixe pour les tests, et **connu d'eux** : sans lui,
        // l'endpoint de webhook refuse tout et il n'y aurait rien à vérifier.
        // Il n'ouvre rien en production, où la valeur vient de
        // `KLAAR_STRIPE_WEBHOOK_SECRET` ou reste absente.
        secret_webhook_stripe: Some(SECRET_WEBHOOK_DE_TEST.to_string()),
        evenements: crate::evenements::BusEvenements::new(),
        billets: Arc::new(crate::billet::BilletsMemoire::new()),
        jetons: Arc::new(
            crate::jwt::JwtHs256::new(b"secret-de-test-uniquement-quarante-huit-octets")
                .expect("secret de test valide"),
        ),
        courriel: Arc::new(Courriel::Journalise(CourrielJournalise::new(
            "https://klaar.test",
            false,
        ))),
        horloge: Arc::new(HorlogeSysteme),
        limiteur: Arc::new(LimiteurMemoire::new()),
        argon2: ParametresArgon2::tests(),
        derriere_proxy: false,
        cookie_securise: false,
        catalogue_en_maintenance: false,
        quota_ecriture_sensible: Quota::ecriture_sensible(),
        // Les tests n'ont pas de Stripe : le contrôle est vérifié par ses
        // propres cas, avec un état construit à la main.
        regles_soumission: ReglesSoumission {
            exiger_methode_paiement: false,
            ..Default::default()
        },
    })
}

/// Type de retour de `App::new()` sans middleware, pour les tests.
pub fn app_de_test(
    etat: web::Data<EtatApplication>,
) -> App<
    impl actix_web::dev::ServiceFactory<
        actix_web::dev::ServiceRequest,
        Config = (),
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
        InitError = (),
    >,
> {
    App::new().app_data(etat).configure(configurer)
}
