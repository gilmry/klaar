//! Le span de requête ne doit contenir ni adresse IP ni agent utilisateur.
//!
//! Ce test capture les journaux réellement émis plutôt que d'inspecter la
//! configuration. C'est délibéré : une première version du constructeur de
//! span déclarait ces champs vides en croyant les neutraliser, ce qui
//! paraissait correct à la lecture et laissait pourtant l'IP dans chaque
//! ligne.

use std::io;
use std::sync::{Arc, Mutex};

use actix_web::{test, web, App};
use tracing_actix_web::TracingLogger;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::prelude::*;

use klaar_api::telemetry::SpanSansDonneesPersonnelles;

/// Écrivain partagé qui accumule les journaux en mémoire.
#[derive(Clone, Default)]
struct Tampon(Arc<Mutex<Vec<u8>>>);

impl io::Write for Tampon {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Tampon {
    type Writer = Tampon;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[actix_web::test]
async fn le_span_de_requete_ne_contient_ni_ip_ni_agent_utilisateur() {
    let tampon = Tampon::default();
    // Souscripteur global et non `set_default` : ce dernier n'installe le
    // souscripteur que pour le fil courant, alors qu'actix exécute la requête
    // sur un fil de son exécuteur. Le tampon restait vide, et le test aurait
    // « passé » sans rien observer si son garde-fou ne l'avait pas signalé.
    tracing::subscriber::set_global_default(
        tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_span_events(FmtSpan::CLOSE)
                .with_writer(tampon.clone()),
        ),
    )
    .expect("un seul test par binaire installe le souscripteur");

    let app = test::init_service(
        App::new()
            .wrap(TracingLogger::<SpanSansDonneesPersonnelles>::new())
            .route("/sonde", web::get().to(|| async { "ok" })),
    )
    .await;

    let requete = test::TestRequest::get()
        .uri("/sonde")
        .insert_header(("User-Agent", "AgentQuiNeDoitPasEtreJournalise/9.9"))
        .to_request();
    // `call_and_read_body` et non `call_service` : tracing-actix-web maintient
    // le span ouvert tant que le corps de la réponse n'est pas consommé, pour
    // mesurer la durée réelle. Sans le lire, aucun évènement de fermeture
    // n'est émis et le tampon reste vide.
    let corps = test::call_and_read_body(&app, requete).await;
    assert_eq!(corps, "ok");

    let journaux = String::from_utf8(tampon.0.lock().unwrap().clone()).unwrap();
    assert!(
        !journaux.is_empty(),
        "aucun journal capturé : le test ne prouverait rien"
    );
    assert!(
        journaux.contains("http.route"),
        "les champs utiles à l'exploitation doivent rester présents : {journaux}"
    );
    for interdit in ["client_ip", "user_agent", "AgentQuiNeDoitPasEtreJournalise"] {
        assert!(
            !journaux.contains(interdit),
            "« {interdit} » ne doit pas apparaître dans les journaux : {journaux}"
        );
    }
}
