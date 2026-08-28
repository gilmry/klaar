//! Diffusion temps réel des événements de Mission (Story 4.9).
//!
//! **Deux moitiés.** L'écriture les publie dans PostgreSQL, avec `pg_notify` et
//! dans la même transaction que le changement lui-même. Ce module écoute ce
//! canal et redistribue aux sockets ouvertes sur ce processus.
//!
//! **Pourquoi passer par la base plutôt que par un canal en mémoire.** Un canal
//! en mémoire ne relie que les clients connectés au même exemplaire du service.
//! Dès qu'il y en a deux, la moitié des utilisateurs cesse de recevoir quoi que
//! ce soit — et rien ne le signale : l'écran ne bouge simplement plus. Le
//! `LISTEN`/`NOTIFY` de PostgreSQL relie tous les exemplaires par le seul point
//! qu'ils partagent déjà.
//!
//! **Une seule diffusion pour toutes les Missions**, et chaque socket filtre la
//! sienne. Un registre indexé par Mission éviterait de réveiller les autres,
//! au prix d'une comptabilité d'inscriptions et de désinscriptions qui fuit dès
//! qu'une déconnexion passe inaperçue. À l'échelle d'un service de dépannage,
//! réveiller quelques dizaines de sockets pour rien coûte moins qu'une fuite de
//! mémoire silencieuse. Le jour où cela pèsera, le point de mesure est ici.

use std::time::Duration;

use klaar_application::ports::evenements::{EvenementMission, CANAL};
use sqlx::postgres::PgListener;
use tokio::sync::broadcast;

/// Événements gardés pour un abonné en retard.
///
/// Un client qui traîne finit par en perdre : il reçoit alors `Lagged`, et la
/// socket le lui dit pour qu'il relise l'état par HTTP. Une file non bornée
/// ferait grossir la mémoire du service au rythme du client le plus lent, ce
/// qui est exactement le levier d'un déni de service.
pub const PROFONDEUR: usize = 256;

/// Délai avant de réessayer une écoute perdue.
///
/// PostgreSQL peut redémarrer, le réseau se couper. Une reprise immédiate en
/// boucle transformerait une base indisponible en tempête de connexions.
const REPRISE_SECONDES: u64 = 3;

/// Le canal de diffusion interne au processus.
#[derive(Clone)]
pub struct BusEvenements {
    emetteur: broadcast::Sender<EvenementMission>,
}

impl Default for BusEvenements {
    fn default() -> Self {
        Self::new()
    }
}

impl BusEvenements {
    pub fn new() -> Self {
        let (emetteur, _) = broadcast::channel(PROFONDEUR);
        Self { emetteur }
    }

    /// S'abonne au flux. L'abonné ne reçoit que ce qui arrive après.
    pub fn abonner(&self) -> broadcast::Receiver<EvenementMission> {
        self.emetteur.subscribe()
    }

    /// Publie un événement. Rend le nombre d'abonnés servis.
    ///
    /// Zéro abonné n'est pas une erreur : c'est l'état normal d'un service dont
    /// personne ne regarde l'écran à cet instant.
    pub fn publier(&self, evenement: EvenementMission) -> usize {
        self.emetteur.send(evenement).unwrap_or(0)
    }

    pub fn abonnes(&self) -> usize {
        self.emetteur.receiver_count()
    }
}

/// Écoute PostgreSQL sans fin et alimente le bus.
///
/// **Ne rend jamais la main** : à lancer dans une tâche de fond. Une erreur
/// d'écoute est journalisée puis réessayée, parce que perdre l'écoute ne doit
/// pas faire tomber le service — les clients ont un sondage lent en filet.
pub async fn ecouter(url: String, bus: BusEvenements) {
    loop {
        match ecouter_une_fois(&url, &bus).await {
            Ok(()) => tracing::warn!("écoute des événements terminée sans erreur ; reprise"),
            Err(e) => tracing::error!(erreur = %e, "écoute des événements interrompue"),
        }
        tokio::time::sleep(Duration::from_secs(REPRISE_SECONDES)).await;
    }
}

async fn ecouter_une_fois(url: &str, bus: &BusEvenements) -> Result<(), sqlx::Error> {
    let mut ecouteur = PgListener::connect(url).await?;
    ecouteur.listen(CANAL).await?;
    tracing::info!(canal = CANAL, "écoute des événements de Mission");

    loop {
        let avis = ecouteur.recv().await?;
        match EvenementMission::depuis_json(avis.payload()) {
            Some(evenement) => {
                bus.publier(evenement);
            }
            // Un `NOTIFY` peut venir d'ailleurs : un `psql` resté ouvert, un
            // autre service, une version plus récente du format. L'ignorer est
            // la bonne réponse ; s'arrêter priverait tout le monde du temps
            // réel à cause d'un message parasite.
            None => tracing::warn!("avis illisible sur le canal des événements, ignoré"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    fn instant() -> chrono::DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    #[tokio::test]
    async fn happy_un_abonne_recoit_ce_qui_est_publie() {
        let bus = BusEvenements::new();
        let mut abonne = bus.abonner();
        let evenement = EvenementMission::statut(Uuid::new_v4(), "ON_SITE", instant());

        assert_eq!(bus.publier(evenement.clone()), 1);
        assert_eq!(abonne.recv().await.unwrap(), evenement);
    }

    #[tokio::test]
    async fn happy_deux_abonnes_recoivent_le_meme_evenement() {
        // Le demandeur et le prestataire regardent la même Mission ; le multi-
        // appareil ajoute encore des sockets sur le même compte.
        let bus = BusEvenements::new();
        let mut a = bus.abonner();
        let mut b = bus.abonner();
        let evenement = EvenementMission::devis_emis(Uuid::new_v4(), instant());

        assert_eq!(bus.publier(evenement.clone()), 2);
        assert_eq!(a.recv().await.unwrap(), evenement);
        assert_eq!(b.recv().await.unwrap(), evenement);
    }

    #[tokio::test]
    async fn negative_publier_sans_abonne_n_est_pas_une_erreur() {
        // L'état normal d'un service que personne ne regarde à cet instant.
        let bus = BusEvenements::new();
        assert_eq!(
            bus.publier(EvenementMission::devis_emis(Uuid::new_v4(), instant())),
            0
        );
        assert_eq!(bus.abonnes(), 0);
    }

    #[tokio::test]
    async fn edge_un_abonne_disparu_ne_bloque_pas_les_autres() {
        let bus = BusEvenements::new();
        let parti = bus.abonner();
        let mut reste = bus.abonner();
        drop(parti);

        let evenement = EvenementMission::statut(Uuid::new_v4(), "COMPLETED", instant());
        bus.publier(evenement.clone());
        assert_eq!(reste.recv().await.unwrap(), evenement);
    }

    #[tokio::test]
    async fn edge_un_abonne_trop_lent_est_distance_et_le_sait() {
        // La file est bornée : au-delà, l'abonné reçoit `Lagged` plutôt que de
        // faire grossir la mémoire du service. C'est ce que la socket traduit
        // en « resynchronise-toi », et non en silence.
        let bus = BusEvenements::new();
        let mut lent = bus.abonner();
        for _ in 0..(PROFONDEUR + 10) {
            bus.publier(EvenementMission::devis_emis(Uuid::new_v4(), instant()));
        }

        let issue = lent.recv().await;
        assert!(
            matches!(issue, Err(broadcast::error::RecvError::Lagged(_))),
            "attendu Lagged, obtenu {issue:?}"
        );

        // Et il peut continuer : le retard n'est pas une déconnexion. La lecture
        // reprend au plus ancien message encore gardé — le `Lagged` peut donc se
        // répéter si l'éviction repasse devant lui, ce qui n'a rien d'anormal.
        // La boucle bornée exprime la propriété voulue (« il finit par relire »)
        // sans dépendre de la position exacte du curseur, qui rendrait ce test
        // intermittent.
        let mut relu = false;
        for _ in 0..PROFONDEUR {
            match lent.recv().await {
                Ok(_) => {
                    relu = true;
                    break;
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(e) => panic!("le retard ne doit pas fermer le flux : {e:?}"),
            }
        }
        assert!(relu, "un abonné distancé doit finir par relire");
    }

    #[tokio::test]
    async fn security_un_abonne_recoit_toutes_les_missions_et_doit_filtrer() {
        // **Ce test documente une garantie que le bus ne donne pas.** La
        // diffusion est globale : c'est la socket qui filtre sur la Mission
        // dont elle a vérifié les droits. Quiconque touchera à ce module doit
        // savoir que l'autorisation n'est pas ici.
        let bus = BusEvenements::new();
        let mut abonne = bus.abonner();
        let autre = Uuid::new_v4();
        bus.publier(EvenementMission::statut(autre, "ON_SITE", instant()));

        assert_eq!(abonne.recv().await.unwrap().mission_id, autre);
    }
}
