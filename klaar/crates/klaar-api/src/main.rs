//! Serveur HTTP Klaar.

use std::sync::Arc;

use actix_web::{web, App, HttpServer};
use actix_web_prom::PrometheusMetricsBuilder;
use tracing_actix_web::TracingLogger;
use tracing_subscriber::{fmt, fmt::format::FmtSpan, prelude::*, EnvFilter};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use klaar_api::jwt::JwtHs256;
use klaar_api::limitation::LimiteurMemoire;
use klaar_api::telemetry::SpanSansDonneesPersonnelles;
use klaar_api::{configurer, ApiDoc, EtatApplication};
use klaar_application::ports::horloge::HorlogeSysteme;
use klaar_audit_adapter::SignataireTrace;
use klaar_email_adapter::CourrielJournalise;
use klaar_identity::ParametresArgon2;
use klaar_push_adapter::{ClesVapid, WebPushSender};
use klaar_sqlx_repos::{
    creer_pool, PgAnnulationRepository, PgCatalogueRepository, PgDemandeRepository,
    PgDevisRepository, PgJournalAudit, PgLiberationRepository, PgMissionRepository,
    PgNotationRepository, PgPaiementRepository, PgProviderRepository, PgPushSubscriptionRepository,
    PgSessionRepository, PgTraceRepository, PgUtilisateurRepository,
};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // with_span_events(CLOSE) : sans ça, tracing-actix-web crée un span par
    // requête mais rien ne l'imprime — fmt::layer() par défaut n'émet une
    // ligne qu'à un tracing::info!() explicite, pas à la fermeture d'un span.
    // Vérifié en local : sans cette ligne, aucune requête n'apparaît dans les
    // logs malgré TracingLogger::default() actif.
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(fmt::layer().json().with_span_events(FmtSpan::CLOSE))
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://klaar:klaar_dev_only@localhost:5433/klaar".to_string());
    let pool = creer_pool(&database_url).await.unwrap_or_else(|e| {
        // Échouer au démarrage plutôt qu'à la première requête : un service
        // qui démarre « vert » puis refuse tout est plus difficile à
        // diagnostiquer qu'un service qui refuse de démarrer.
        eprintln!("connexion PostgreSQL impossible ({database_url}) : {e}");
        std::process::exit(1);
    });

    // Le push est optionnel : sans clé configurée, le service tourne sans
    // notifications au lieu de refuser de démarrer.
    let push = match std::env::var("KLAAR_VAPID_PRIVATE_KEY") {
        Ok(cle) if !cle.is_empty() => {
            let sujet = std::env::var("KLAAR_VAPID_SUBJECT")
                .unwrap_or_else(|_| "mailto:ops@klaar.be".to_string());
            match ClesVapid::depuis_base64url(&cle, sujet) {
                Ok(cles) => Some(Arc::new(WebPushSender::new(cles))),
                Err(e) => {
                    eprintln!("clé VAPID invalide : {e}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            tracing::warn!(
                "KLAAR_VAPID_PRIVATE_KEY absente : les notifications push sont désactivées"
            );
            None
        }
    };

    // Le secret de signature n'est pas optionnel, contrairement aux clés VAPID :
    // sans lui, personne ne peut se connecter. Refuser de démarrer vaut mieux
    // qu'en générer un à la volée, qui invaliderait toutes les sessions à
    // chaque redémarrage sans que personne ne comprenne pourquoi.
    let jetons = match std::env::var("KLAAR_JWT_SECRET") {
        Ok(secret) => match JwtHs256::new(secret.as_bytes()) {
            Ok(emetteur) => Arc::new(emetteur),
            Err(e) => {
                eprintln!("KLAAR_JWT_SECRET invalide : {e}");
                eprintln!("en générer un : openssl rand -base64 48");
                std::process::exit(1);
            }
        },
        Err(_) => {
            eprintln!("KLAAR_JWT_SECRET absente : klaar-api ne peut pas signer de session.");
            eprintln!("en générer un : openssl rand -base64 48");
            std::process::exit(1);
        }
    };

    // La clé de scellement de la trace, elle, **est** optionnelle, à l'inverse
    // du secret de signature des jetons. Sans elle, la trace est écrite mais
    // non signée : elle explique toujours une décision, ce qui est ce que l'AI
    // Act exige. Refuser de démarrer priverait le service de sa trace entière
    // pour protéger cette trace, ce qui est le contraire du but. Le rapport
    // d'audit compte les lignes non signées et le dit.
    let signataire_trace = match std::env::var("KLAAR_TRACE_HMAC_KEY") {
        Ok(cle) if !cle.is_empty() => match SignataireTrace::new(cle.as_bytes()) {
            Ok(s) => Some(Arc::new(s)),
            Err(e) => {
                eprintln!("KLAAR_TRACE_HMAC_KEY invalide : {e}");
                eprintln!("en générer une : openssl rand -base64 48");
                std::process::exit(1);
            }
        },
        _ => {
            tracing::warn!(
                "KLAAR_TRACE_HMAC_KEY absente : la trace de matching est écrite sans \
                 signature. Elle explique toujours les décisions, mais une altération \
                 faite depuis la base ne se détecterait plus."
            );
            None
        }
    };

    // Plafond des écritures sensibles. Cinq par heure et par adresse en temps
    // normal ; le relever sert au déploiement de démonstration, où plusieurs
    // parcours se connectent depuis la même adresse en quelques minutes.
    // Un chiffre paramétré plutôt qu'un interrupteur : un quota qu'on peut
    // éteindre finit éteint en production, un chiffre annoncé au démarrage se
    // remarque.
    let quota_ecriture_sensible = match std::env::var("KLAAR_QUOTA_ECRITURE_SENSIBLE") {
        Ok(v) if !v.is_empty() => match v.parse::<usize>() {
            Ok(max) if max > 0 => {
                tracing::warn!(
                    max,
                    defaut = klaar_api::limitation::Quota::ecriture_sensible().max,
                    "KLAAR_QUOTA_ECRITURE_SENSIBLE relève le plafond des inscriptions et \
                     connexions par adresse. Démonstration uniquement."
                );
                klaar_api::limitation::Quota::ecriture_sensible_plafond(max)
            }
            _ => {
                eprintln!("KLAAR_QUOTA_ECRITURE_SENSIBLE doit être un entier strictement positif");
                std::process::exit(1);
            }
        },
        _ => klaar_api::limitation::Quota::ecriture_sensible(),
    };

    // Vrai par défaut : un cookie de session sans `Secure` voyage en clair. Le
    // désactiver n'a de sens qu'en développement local sur HTTP, où le
    // navigateur refuserait sinon le cookie sans rien signaler.
    let cookie_securise = std::env::var("KLAAR_COOKIE_SECURE").as_deref() != Ok("0");
    if !cookie_securise {
        tracing::warn!(
            "KLAAR_COOKIE_SECURE=0 : le cookie de rafraîchissement part sans l'attribut \
             Secure. Développement local uniquement."
        );
    }

    // Un seul limiteur pour tout le processus. Le construire dans la fabrique
    // de `App` en donnerait un par fil d'exécution, et la limite annoncée
    // serait silencieusement multipliée par le nombre de coeurs.
    let limiteur = Arc::new(LimiteurMemoire::new());

    // Temps réel (Story 4.9). Créés **avant** `HttpServer::new`, dont la
    // fermeture est appelée une fois par fil d'exécution : un bus par fil ne
    // relierait qu'une fraction des sockets à l'écoute PostgreSQL, et le défaut
    // ne se verrait qu'en production, sous la forme d'écrans qui ne bougent
    // plus pour certains utilisateurs.
    let evenements = klaar_api::evenements::BusEvenements::new();
    let billets = Arc::new(klaar_api::billet::BilletsMemoire::new());

    // L'écoute tourne dans sa propre tâche et ne rend jamais la main. Sa perte
    // ne fait pas tomber le service : les clients gardent un sondage lent en
    // filet, et l'écoute se reprend d'elle-même.
    actix_web::rt::spawn(klaar_api::evenements::ecouter(
        database_url.clone(),
        evenements.clone(),
    ));
    let courriel = Arc::new(CourrielJournalise::depuis_environnement());
    let derriere_proxy = std::env::var("KLAAR_DERRIERE_PROXY").as_deref() == Ok("1");
    if !derriere_proxy {
        tracing::info!(
            "KLAAR_DERRIERE_PROXY absente : X-Forwarded-For ignoré, la limitation \
             de débit compte par adresse de connexion directe"
        );
    }

    // Retire le catalogue le temps d'une mise à jour (FR-008 `@edge`). Le
    // service répond alors 503 avec `Retry-After`, ce qui distingue un retrait
    // volontaire d'une panne — et évite qu'un visiteur tombe sur un catalogue à
    // moitié réécrit.
    let catalogue_en_maintenance =
        std::env::var("KLAAR_CATALOGUE_MAINTENANCE").as_deref() == Ok("1");
    if catalogue_en_maintenance {
        tracing::warn!("KLAAR_CATALOGUE_MAINTENANCE=1 : le catalogue répond 503");
    }

    // FR-011 fait de la méthode de paiement une précondition à toute Demande.
    // Quota de Demandes par compte et par heure (FR-011). Relevé pour le seul
    // déploiement de démonstration, où le même compte en soumet plusieurs en
    // quelques minutes. Un chiffre, et non un interrupteur : un quota qu'on
    // peut éteindre finit éteint en production.
    let max_demandes_par_heure = match std::env::var("KLAAR_MAX_DEMANDES_PAR_HEURE") {
        Ok(v) if !v.is_empty() => match v.parse::<i64>() {
            Ok(max) if max > 0 => {
                tracing::warn!(
                    max,
                    defaut = klaar_application::usecases::soumettre_demande::MAX_DEMANDES_PAR_HEURE,
                    "KLAAR_MAX_DEMANDES_PAR_HEURE relève le quota de Demandes par compte. \
                     Démonstration uniquement."
                );
                max
            }
            _ => {
                eprintln!("KLAAR_MAX_DEMANDES_PAR_HEURE doit être un entier strictement positif");
                std::process::exit(1);
            }
        },
        _ => klaar_application::usecases::soumettre_demande::MAX_DEMANDES_PAR_HEURE,
    };

    // Le contrôle est actif par défaut : l'oublier allumé ne coûte qu'un refus
    // explicite, l'oublier éteint laisse passer des Demandes qu'aucun paiement
    // ne garantit. Le déploiement vitrine le désactive, faute de compte Stripe
    // (Story 1.7).
    let exiger_methode_paiement =
        std::env::var("KLAAR_EXIGER_METHODE_PAIEMENT").as_deref() != Ok("0");
    if !exiger_methode_paiement {
        tracing::warn!(
            "KLAAR_EXIGER_METHODE_PAIEMENT=0 : les Demandes sont acceptées sans méthode \
             de paiement enregistrée."
        );
    }

    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    tracing::info!(%bind_addr, "klaar-api démarre — /api/v1/docs Swagger UI, /metrics Prometheus");

    let metrics = PrometheusMetricsBuilder::new("klaar_api")
        .endpoint("/metrics")
        .build()
        .expect("configuration Prometheus invalide");

    HttpServer::new(move || {
        let etat = web::Data::new(EtatApplication {
            abonnements: Arc::new(PgPushSubscriptionRepository::new(pool.clone())),
            push: push.clone(),
            utilisateurs: Arc::new(PgUtilisateurRepository::new(pool.clone())),
            journal: Arc::new(PgJournalAudit::new(pool.clone())),
            sessions: Arc::new(PgSessionRepository::new(pool.clone())),
            catalogue: Arc::new(PgCatalogueRepository::new(pool.clone())),
            demandes: Arc::new(PgDemandeRepository::new(pool.clone())),
            paiements: Arc::new(PgPaiementRepository::new(pool.clone())),
            prestataires: Arc::new(PgProviderRepository::new(pool.clone())),
            traces: Arc::new(match signataire_trace.clone() {
                Some(s) => PgTraceRepository::avec_signature(pool.clone(), s),
                None => PgTraceRepository::new(pool.clone()),
            }),
            missions: Arc::new(PgMissionRepository::new(pool.clone())),
            devis: Arc::new(PgDevisRepository::new(pool.clone())),
            liberations: Arc::new(PgLiberationRepository::new(pool.clone())),
            annulations: Arc::new(PgAnnulationRepository::new(pool.clone())),
            notations: Arc::new(PgNotationRepository::new(pool.clone())),
            // Le bus et les billets sont **partagés entre les fabriques
            // d'application** : `HttpServer::new` appelle sa fermeture une fois
            // par fil d'exécution, et un bus par fil ne relierait qu'un
            // huitième des sockets à l'écoute PostgreSQL.
            evenements: evenements.clone(),
            billets: billets.clone(),
            jetons: jetons.clone(),
            courriel: courriel.clone(),
            horloge: Arc::new(HorlogeSysteme),
            limiteur: limiteur.clone(),
            argon2: ParametresArgon2::production(),
            derriere_proxy,
            cookie_securise,
            catalogue_en_maintenance,
            quota_ecriture_sensible,
            regles_soumission: klaar_application::usecases::soumettre_demande::ReglesSoumission {
                exiger_methode_paiement,
                max_demandes_par_heure,
            },
        });
        App::new()
            // Span racine expurgé de l'IP et de l'agent utilisateur, cf.
            // klaar_api::telemetry.
            .wrap(TracingLogger::<SpanSansDonneesPersonnelles>::new())
            .wrap(metrics.clone())
            .app_data(etat)
            .configure(configurer)
            .service(
                SwaggerUi::new("/api/v1/docs/{_:.*}")
                    .url("/api/v1/openapi.json", ApiDoc::openapi()),
            )
    })
    .bind(bind_addr)?
    .run()
    .await
}
