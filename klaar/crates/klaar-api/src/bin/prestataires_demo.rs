//! Peuple des prestataires de démonstration (Story 1.6, partielle).
//!
//! **Ces prestataires ne sont contrôlés par personne.** FR-003 exige la
//! validation du numéro à la Banque-Carrefour des Entreprises, le contrôle de
//! l'état de faillite et la collecte d'une attestation d'assurance : rien de
//! cela n'est possible ici. Ce binaire les active tout de même, en marquant
//! leur origine `DEMONSTRATION` en base — de sorte qu'un prestataire non
//! contrôlé se retrouve par une requête, longtemps après.
//!
//! Un binaire et non un endpoint : une commande hors ligne ne s'atteint pas
//! par HTTP. Une route d'activation, même protégée, serait une route qu'on peut
//! oublier d'enlever.
//!
//! Refuse de tourner sans `KLAAR_PRESTATAIRES_DEMO=1`. Le drapeau ne protège de
//! rien — quiconque peut lancer ce binaire peut aussi poser la variable — mais
//! il empêche qu'un `cargo run` distrait peuple une base de production.

use std::process::ExitCode;

use chrono::Utc;
use klaar_application::ports::provider_repository::ProviderRepository;
use klaar_identity::{
    EmpreinteMotDePasse, MotDePasse, NumeroBce, ParametresArgon2, PreuveKyc, Provider,
};
use klaar_shared_kernel::{Email, Geo};
use klaar_sqlx_repos::demonstration::compte_actif_de_demonstration;
use klaar_sqlx_repos::demonstration::remettre_a_zero;
use klaar_sqlx_repos::{creer_pool, PgProviderRepository, PoolPg};

/// Comptes demandeurs de démonstration.
///
/// Deux : les parcours à deux acteurs ont besoin d'un demandeur pendant qu'un
/// autre scénario en occupe un. Leur adresse est sur `.invalid` comme celle des
/// prestataires, pour la même raison — rien ne peut y être livré.
const DEMANDEURS: [&str; 2] = ["camille", "sacha"];

/// Prestataires fictifs, répartis dans la Région.
///
/// Positions choisies dans des communes distinctes pour que la recherche par
/// rayon de la Story 3.2 ait quelque chose à trier.
const PRESTATAIRES: [(&str, &str, f64, f64, &[&str]); 7] = [
    (
        "Plomberie Centre SRL",
        "plomberie-centre",
        50.8467,
        4.3525,
        &["plomberie"],
    ),
    (
        "Serrurerie Midi",
        "serrurerie-midi",
        50.8360,
        4.3360,
        &["serrurerie"],
    ),
    (
        "Élec Schaerbeek",
        "elec-schaerbeek",
        50.8676,
        4.3737,
        &["electricite"],
    ),
    (
        "Dépannage Auto Uccle",
        "auto-uccle",
        50.8003,
        4.3383,
        &["auto"],
    ),
    (
        "Multiservices Anderlecht",
        "multi-anderlecht",
        50.8367,
        4.3097,
        &["plomberie", "serrurerie", "livraison"],
    ),
    // Deux plombiers au même endroit, au sud de la Région.
    //
    // Ils servent le parcours qui montre la course à l'acceptation : il faut
    // deux candidats à égale distance pour que « le premier arrivé gagne »
    // signifie quelque chose. Posés loin du centre parce qu'une base de
    // développement partagée avec la suite de tests y accumule des centaines de
    // prestataires, qui les évinceraient du classement.
    //
    // **Ajoutés à la fin, et pas ailleurs.** Le rang dans cette liste est la
    // graine du numéro d'entreprise fictif : insérer au milieu décale tous les
    // suivants, qui tentent alors de se recréer sur des comptes déjà pris.
    (
        "Plomberie Sud",
        "plomberie-sud",
        50.8020,
        4.3400,
        &["plomberie"],
    ),
    (
        "Dépannage Sud",
        "depannage-sud",
        50.8021,
        4.3401,
        &["plomberie"],
    ),
];

/// Fabrique un numéro BCE dont la clé de contrôle est correcte.
///
/// Construit et non copié d'une entreprise réelle : un numéro BCE identifie une
/// personne morale, et en figer un dans un jeu de démonstration publié la
/// rattacherait durablement à ce dépôt.
fn numero_fictif(rang: u64) -> NumeroBce {
    let corps = 1_000_000 + rang;
    NumeroBce::parse(&format!("{corps:08}{:02}", 97 - (corps % 97)))
        .expect("numéro construit avec sa clé")
}

async fn compte(pool: &PoolPg, marqueur: &str) -> Result<uuid::Uuid, String> {
    // Adresse sur `.invalid`, réservé par la RFC 2606 : rien ne peut y être
    // livré, et aucun courriel de démonstration ne partira vers une vraie boîte.
    let email = Email::parse(&format!("{marqueur}@demo.klaar.invalid"))
        .map_err(|e| format!("adresse de démonstration invalide : {e}"))?;
    let mdp = MotDePasse::parse("demonstration-klaar-2026")
        .map_err(|e| format!("mot de passe de démonstration invalide : {e}"))?;
    let empreinte = EmpreinteMotDePasse::calculer(&mdp, ParametresArgon2::production())
        .map_err(|e| format!("hachage impossible : {e}"))?;

    compte_actif_de_demonstration(pool, &email, &empreinte)
        .await
        .map_err(|e| format!("compte de démonstration : {e}"))
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt().json().init();

    if std::env::var("KLAAR_PRESTATAIRES_DEMO").as_deref() != Ok("1") {
        eprintln!("KLAAR_PRESTATAIRES_DEMO=1 requise.");
        eprintln!(
            "Cette commande active des prestataires SANS contrôle BCE. Leur origine est \
             marquée DEMONSTRATION en base."
        );
        return ExitCode::FAILURE;
    }

    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL requise");
        return ExitCode::FAILURE;
    };
    let pool = match creer_pool(&database_url).await {
        Ok(pool) => pool,
        Err(e) => {
            eprintln!("connexion PostgreSQL impossible : {e}");
            return ExitCode::FAILURE;
        }
    };
    let depot = PgProviderRepository::new(pool.clone());

    tracing::warn!(
        "création de prestataires de DÉMONSTRATION : aucun contrôle BCE, aucune attestation \
         d'assurance, aucune vérification de faillite"
    );

    let mut crees = 0;
    for (rang, (raison, marqueur, lat, lon, secteurs)) in PRESTATAIRES.iter().enumerate() {
        let numero = numero_fictif(rang as u64);
        match depot.par_numero_bce(&numero).await {
            Ok(Some(_)) => continue,
            Ok(None) => {}
            Err(e) => {
                tracing::error!(erreur = %e, "lecture impossible");
                return ExitCode::FAILURE;
            }
        }

        let utilisateur_id = match compte(&pool, marqueur).await {
            Ok(id) => id,
            Err(e) => {
                tracing::error!(erreur = e, "compte de démonstration impossible");
                return ExitCode::FAILURE;
            }
        };

        let competences: Vec<_> = secteurs
            .iter()
            .filter_map(|s| klaar_catalog::CodeCatalogue::parse(s).ok())
            .collect();
        let base = match Geo::new(*lat, *lon) {
            Ok(g) => g,
            Err(e) => {
                tracing::error!(erreur = ?e, "position de démonstration invalide");
                return ExitCode::FAILURE;
            }
        };

        let mut provider = match Provider::inscrire(
            utilisateur_id,
            numero,
            raison,
            base,
            competences,
            Utc::now(),
        ) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(erreur = %e, "prestataire de démonstration invalide");
                return ExitCode::FAILURE;
            }
        };
        // La seule porte d'activation sans contrôle réel, et elle porte son nom.
        provider.valider_kyc(PreuveKyc::demonstration(Utc::now()));

        if let Err(e) = depot.creer(&provider).await {
            tracing::error!(erreur = %e, "création impossible");
            return ExitCode::FAILURE;
        }
        if let Err(e) = depot.definir_disponibilite(provider.id, true).await {
            tracing::error!(erreur = %e, "disponibilité impossible");
            return ExitCode::FAILURE;
        }
        crees += 1;
    }

    tracing::warn!(
        prestataires = crees,
        "prestataires de démonstration actifs — origine DEMONSTRATION, aucun contrôle réel"
    );

    // Comptes demandeurs, pour que les parcours filmés aient les deux côtés.
    let mut demandeurs = 0;
    for marqueur in DEMANDEURS {
        match compte(&pool, marqueur).await {
            Ok(_) => demandeurs += 1,
            Err(e) => {
                tracing::error!(erreur = e, "compte demandeur de démonstration impossible");
                return ExitCode::FAILURE;
            }
        }
    }
    tracing::warn!(demandeurs, "comptes demandeurs de démonstration prêts");

    // Remise à zéro de l'état laissé par une exécution précédente.
    //
    // Sans elle, un prestataire resté « occupé » ne recevrait plus rien, et le
    // parcours filmé s'arrêterait à la première étape sans que la cause soit
    // visible à l'écran. Ne touche **que** les comptes de démonstration : leur
    // adresse est sur `.invalid`, réservé par la RFC 2606, et aucun compte réel
    // ne peut en porter une.
    if let Err(e) = remettre_a_zero(&pool).await {
        tracing::error!(erreur = %e, "remise à zéro du jeu de démonstration impossible");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
