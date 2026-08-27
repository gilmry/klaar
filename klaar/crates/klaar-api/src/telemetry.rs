//! Journalisation structurée, expurgée des données personnelles.
//!
//! `tracing_actix_web::DefaultRootSpanBuilder` place `http.client_ip` et
//! `http.user_agent` dans chaque span de requête. Une adresse IP est une
//! donnée à caractère personnel au sens du RGPD, et l'agent utilisateur
//! contribue à l'empreinte du navigateur. Les journaliser à chaque requête
//! constitue un traitement que rien ne documente et dont personne n'a défini
//! la durée de conservation.
//!
//! Tant que `/api/v1/health` était le seul endpoint, la question était
//! théorique ; elle a cessé de l'être avec les endpoints d'abonnement push.
//!
//! Le span est construit champ par champ plutôt que par la macro `root_span!`.
//! Une première tentative déclarait les deux champs à `Empty` en espérant
//! qu'ils resteraient vides : elle ne marchait pas, la macro les renseigne
//! elle-même, et les journaux contenaient toujours l'IP. Le test
//! `tests/telemetry.rs` existe pour que cette illusion ne puisse pas se
//! reproduire silencieusement.

use actix_web::body::MessageBody;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::Error;
use tracing::{field::Empty, Span};
use tracing_actix_web::{RequestId, RootSpanBuilder};

pub struct SpanSansDonneesPersonnelles;

impl RootSpanBuilder for SpanSansDonneesPersonnelles {
    fn on_request_start(request: &ServiceRequest) -> Span {
        use actix_web::HttpMessage;

        let request_id = request
            .extensions()
            .get::<RequestId>()
            .map(|id| id.to_string())
            .unwrap_or_default();
        let methode = request.method().as_str();
        // `match_pattern` et non le chemin brut : `/missions/{id}` plutôt que
        // `/missions/9f3a…`. Un identifiant de Mission dans chaque ligne de
        // journal reconstitue l'activité d'une personne.
        let route = request
            .match_pattern()
            .unwrap_or_else(|| request.path().to_string());

        tracing::info_span!(
            "requête HTTP",
            http.method = %methode,
            http.route = %route,
            http.scheme = %request.connection_info().scheme(),
            http.flavor = ?request.version(),
            http.status_code = Empty,
            otel.name = %format!("{methode} {route}"),
            otel.kind = "server",
            otel.status_code = Empty,
            request_id = %request_id,
            exception.message = Empty,
            exception.details = Empty,
        )
    }

    fn on_request_end<B: MessageBody>(span: Span, outcome: &Result<ServiceResponse<B>, Error>) {
        // La finalisation par défaut ne fait que renseigner les champs déclarés
        // ci-dessus ; elle n'en ajoute pas. Réutilisée telle quelle pour ne pas
        // avoir à redire comment une erreur se note.
        tracing_actix_web::DefaultRootSpanBuilder::on_request_end(span, outcome);
    }
}
