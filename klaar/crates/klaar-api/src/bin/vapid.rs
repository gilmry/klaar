//! Génère un couple de clés VAPID (Story 0.12).
//!
//! À exécuter une fois par environnement. Changer de clé invalide **tous** les
//! abonnements existants : les navigateurs ont lié le leur à la clé publique
//! qu'on leur avait donnée, et il faudra les faire se réabonner.

use klaar_push_adapter::ClesVapid;

fn main() {
    let sujet = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "mailto:ops@klaar.be".to_string());

    match ClesVapid::generer(&sujet) {
        Ok((_, privee, publique)) => {
            println!("# À placer dans l'environnement du serveur, jamais dans le dépôt :");
            println!("KLAAR_VAPID_PRIVATE_KEY={privee}");
            println!("KLAAR_VAPID_SUBJECT={sujet}");
            println!();
            println!("# Clé publique, distribuée aux navigateurs par");
            println!("# GET /api/v1/push/cle-publique :");
            println!("# {publique}");
        }
        Err(e) => {
            eprintln!("génération impossible : {e}");
            std::process::exit(1);
        }
    }
}
