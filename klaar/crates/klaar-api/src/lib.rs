//! API HTTP de Klaar (actix-web + utoipa — ADR-003, ADR-004).
//!
//! L'application est construite ici, et non dans `main`, pour que les tests
//! puissent la monter en mémoire avec un état choisi. Un endpoint testé
//! seulement en lançant un vrai serveur finit par n'être testé que dans les
//! cas nominaux.

use std::sync::Arc;

use actix_web::{web, App};
use utoipa::OpenApi;

use klaar_application::ports::horloge::HorlogeSysteme;
use klaar_application::ports::jeton_acces::EmetteurJetonAcces;
use klaar_application::usecases::soumettre_demande::ReglesSoumission;
use klaar_email_adapter::CourrielJournalise;
use klaar_identity::ParametresArgon2;
use klaar_push_adapter::WebPushSender;
use klaar_sqlx_repos::{
    PgAnnulationRepository, PgCatalogueRepository, PgDemandeRepository, PgDevisRepository,
    PgJournalAudit, PgLiberationRepository, PgLitigeRepository, PgMessageRepository,
    PgMissionRepository, PgNotationRepository, PgOpsRepository, PgPaiementRepository,
    PgProviderRepository, PgPushSubscriptionRepository, PgSessionRepository, PgTraceRepository,
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
    /// Diffusion temps réel des événements de Mission (Story 4.9).
    pub evenements: crate::evenements::BusEvenements,
    /// Billets d'ouverture de socket, à usage unique et de courte vie.
    pub billets: Arc<crate::billet::BilletsMemoire>,
    /// Signataire du jeton d'accès. Derrière un trait : le format du jeton
    /// est remplaçable sans toucher aux cas d'usage.
    pub jetons: Arc<dyn EmetteurJetonAcces>,
    pub courriel: Arc<CourrielJournalise>,
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
    cfg.service(routes::health::health)
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
        .service(routes::temps_reel::demander_billet)
        .service(routes::temps_reel::suivre_en_direct)
        .service(routes::suivi::suivre_demande)
        .service(routes::suivi::demandes_recues)
        .service(routes::suivi::suivre_mission)
        .service(routes::push::cle_publique)
        .service(routes::push::enregistrer_abonnement)
        .service(routes::push::supprimer_abonnement);
}

/// État câblé sur une base réelle, avec des paramètres argon2 faibles.
///
/// Vit à côté d'`app_de_test` pour la même raison : les tests d'intégration
/// sont une caisse séparée et ne voient pas les `#[cfg(test)]` de celle-ci. Les
/// paramètres argon2 de production coûteraient ici une centaine de
/// millisecondes par inscription, soit une suite qu'on finit par ne plus
/// lancer.
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
        ops: Arc::new(PgOpsRepository::new(pool)),
        evenements: crate::evenements::BusEvenements::new(),
        billets: Arc::new(crate::billet::BilletsMemoire::new()),
        jetons: Arc::new(
            crate::jwt::JwtHs256::new(b"secret-de-test-uniquement-quarante-huit-octets")
                .expect("secret de test valide"),
        ),
        courriel: Arc::new(CourrielJournalise::new("https://klaar.test", false)),
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
