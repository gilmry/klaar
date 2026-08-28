//! Billets d'ouverture de socket (Story 4.9).
//!
//! **Pourquoi un billet plutôt que le jeton d'accès.** Un navigateur ne peut
//! pas poser d'en-tête `Authorization` sur une WebSocket : `new WebSocket(url)`
//! n'en accepte pas. Il reste l'URL — et une URL finit dans les journaux du
//! serveur, dans ceux du proxy, dans l'historique du navigateur et dans le
//! `Referer`. Y mettre un JWT valable une heure reviendrait à l'y publier.
//!
//! Le billet est donc **à usage unique**, valable **trente secondes**, obtenu
//! par une requête authentifiée normale, et échangé aussitôt contre une socket.
//! S'il fuite, il est déjà consommé ou déjà périmé.
//!
//! **Ce qui est conservé est son condensé**, jamais le billet lui-même : la
//! même discipline que pour les jetons de rafraîchissement. Quelqu'un qui lit
//! la mémoire du service y trouve des empreintes, pas des laissez-passer.
//!
//! **En mémoire, donc par exemplaire du service.** Avec plusieurs exemplaires
//! derrière un répartiteur, un billet émis par l'un et présenté à l'autre est
//! refusé, et le client réessaie. C'est une limite assumée : la partager
//! demanderait un magasin commun pour un secret qui vit trente secondes.

use std::collections::HashMap;
use std::sync::Mutex;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Durée de vie d'un billet.
///
/// Trente secondes : le temps d'un aller-retour et d'une poignée de main, pas
/// celui de traîner dans un journal.
pub const VALIDITE_SECONDES: i64 = 30;

/// Longueur du secret tiré, en octets.
///
/// 32 octets, soit 256 bits d'entropie. Un billet se devine autrement que par
/// force brute — il ne se devine pas.
const OCTETS: usize = 32;

/// Billets vivants, au-delà duquel l'émission refuse.
///
/// Une borne, parce qu'un compte qui demanderait des billets en boucle ferait
/// autrement grossir cette table sans fin. Le nettoyage des périmés a lieu à
/// chaque émission, donc ce plafond ne devrait jamais être vu.
const VIVANTS_MAX: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Reservation {
    utilisateur_id: Uuid,
    expire_le: DateTime<Utc>,
}

#[derive(Default)]
pub struct BilletsMemoire {
    reservations: Mutex<HashMap<[u8; 32], Reservation>>,
}

fn empreinte(billet: &str) -> [u8; 32] {
    let condense = Sha256::digest(billet.as_bytes());
    let mut cle = [0u8; 32];
    cle.copy_from_slice(&condense);
    cle
}

impl BilletsMemoire {
    pub fn new() -> Self {
        Self::default()
    }

    /// Émet un billet pour ce compte.
    ///
    /// Rend `None` quand la table est pleine : refuser est préférable à laisser
    /// la mémoire du service suivre le rythme de celui qui insiste.
    pub fn emettre(&self, utilisateur_id: Uuid, maintenant: DateTime<Utc>) -> Option<String> {
        let mut secret = [0u8; OCTETS];
        rand::rng().fill_bytes(&mut secret);
        let billet = URL_SAFE_NO_PAD.encode(secret);

        let mut reservations = self.reservations.lock().expect("verrou des billets");
        // Le ménage a lieu ici plutôt que dans une tâche de fond : la table ne
        // grossit qu'à l'émission, donc c'est le seul moment où le nettoyage a
        // quelque chose à faire.
        reservations.retain(|_, r| r.expire_le > maintenant);
        if reservations.len() >= VIVANTS_MAX {
            return None;
        }

        reservations.insert(
            empreinte(&billet),
            Reservation {
                utilisateur_id,
                expire_le: maintenant + Duration::seconds(VALIDITE_SECONDES),
            },
        );
        Some(billet)
    }

    /// Consomme un billet et rend le compte qu'il désigne.
    ///
    /// **Usage unique** : la ligne est retirée qu'elle soit encore valable ou
    /// non. Un billet rejoué ne doit pas ouvrir une seconde socket, et un
    /// billet périmé n'a plus rien à faire en mémoire.
    pub fn consommer(&self, billet: &str, maintenant: DateTime<Utc>) -> Option<Uuid> {
        let mut reservations = self.reservations.lock().expect("verrou des billets");
        let reservation = reservations.remove(&empreinte(billet))?;
        (reservation.expire_le > maintenant).then_some(reservation.utilisateur_id)
    }

    #[cfg(test)]
    fn vivants(&self) -> usize {
        self.reservations.lock().expect("verrou des billets").len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn instant() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    // === @happy ===

    #[test]
    fn happy_un_billet_emis_ouvre_la_socket_de_son_compte() {
        let billets = BilletsMemoire::new();
        let compte = Uuid::new_v4();
        let billet = billets.emettre(compte, instant()).expect("émission");

        assert_eq!(billets.consommer(&billet, instant()), Some(compte));
    }

    #[test]
    fn happy_deux_billets_du_meme_compte_coexistent() {
        // Le multi-appareil : deux onglets, deux sockets, deux billets.
        let billets = BilletsMemoire::new();
        let compte = Uuid::new_v4();
        let a = billets.emettre(compte, instant()).unwrap();
        let b = billets.emettre(compte, instant()).unwrap();

        assert_ne!(a, b);
        assert_eq!(billets.consommer(&a, instant()), Some(compte));
        assert_eq!(billets.consommer(&b, instant()), Some(compte));
    }

    // === @negative ===

    #[test]
    fn negative_un_billet_inconnu_n_ouvre_rien() {
        let billets = BilletsMemoire::new();
        assert_eq!(billets.consommer("pas-un-billet", instant()), None);
        assert_eq!(billets.consommer("", instant()), None);
    }

    #[test]
    fn negative_un_billet_perime_n_ouvre_rien() {
        let billets = BilletsMemoire::new();
        let billet = billets.emettre(Uuid::new_v4(), instant()).unwrap();
        let trop_tard = instant() + Duration::seconds(VALIDITE_SECONDES + 1);

        assert_eq!(billets.consommer(&billet, trop_tard), None);
    }

    // === @edge ===

    #[test]
    fn edge_la_derniere_seconde_de_validite_passe_encore() {
        let billets = BilletsMemoire::new();
        let billet = billets.emettre(Uuid::new_v4(), instant()).unwrap();
        let juste_avant = instant() + Duration::seconds(VALIDITE_SECONDES - 1);

        assert!(billets.consommer(&billet, juste_avant).is_some());
    }

    #[test]
    fn edge_l_emission_nettoie_les_billets_perimes() {
        // Sans ce ménage, la table grossirait au rythme des ouvertures de page.
        let billets = BilletsMemoire::new();
        for _ in 0..5 {
            billets.emettre(Uuid::new_v4(), instant());
        }
        assert_eq!(billets.vivants(), 5);

        billets.emettre(Uuid::new_v4(), instant() + Duration::minutes(1));
        assert_eq!(billets.vivants(), 1, "seul le dernier survit");
    }

    #[test]
    fn edge_un_billet_perime_est_retire_meme_en_echouant() {
        let billets = BilletsMemoire::new();
        let billet = billets.emettre(Uuid::new_v4(), instant()).unwrap();
        billets.consommer(&billet, instant() + Duration::minutes(1));

        assert_eq!(billets.vivants(), 0);
    }

    // === @security ===

    #[test]
    fn security_un_billet_ne_sert_qu_une_fois() {
        // C'est ce qui rend acceptable son passage par l'URL : rejoué, il
        // n'ouvre rien.
        let billets = BilletsMemoire::new();
        let compte = Uuid::new_v4();
        let billet = billets.emettre(compte, instant()).unwrap();

        assert_eq!(billets.consommer(&billet, instant()), Some(compte));
        assert_eq!(billets.consommer(&billet, instant()), None);
    }

    #[test]
    fn security_le_billet_lui_meme_n_est_pas_conserve() {
        // La mémoire du service ne contient que des empreintes. Le test le
        // vérifie par la clé de la table, seul endroit où le billet pourrait
        // se retrouver.
        let billets = BilletsMemoire::new();
        let billet = billets.emettre(Uuid::new_v4(), instant()).unwrap();

        let table = billets.reservations.lock().unwrap();
        let cle = table.keys().next().expect("une réservation");
        assert_ne!(cle.as_slice(), billet.as_bytes());
        assert_eq!(*cle, empreinte(&billet));
    }

    #[test]
    fn security_deux_billets_ne_se_ressemblent_pas() {
        // 256 bits tirés à chaque fois : une collision signalerait un générateur
        // cassé, ce qui rendrait les billets devinables.
        let billets = BilletsMemoire::new();
        let mut vus = std::collections::HashSet::new();
        for _ in 0..200 {
            assert!(vus.insert(billets.emettre(Uuid::new_v4(), instant()).unwrap()));
        }
    }

    #[test]
    fn security_un_billet_n_ouvre_que_le_compte_qui_l_a_demande() {
        // Le compte vient du billet, jamais d'un paramètre : sans cela, une
        // socket s'ouvrirait au nom d'autrui avec un billet à soi.
        let billets = BilletsMemoire::new();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let billet_de_a = billets.emettre(a, instant()).unwrap();
        billets.emettre(b, instant());

        assert_eq!(billets.consommer(&billet_de_a, instant()), Some(a));
    }

    #[test]
    fn security_la_table_de_billets_est_bornee() {
        // Sinon un compte qui demande des billets en boucle ferait grossir la
        // mémoire du service à son rythme.
        let billets = BilletsMemoire::new();
        for _ in 0..VIVANTS_MAX {
            assert!(billets.emettre(Uuid::new_v4(), instant()).is_some());
        }
        assert!(billets.emettre(Uuid::new_v4(), instant()).is_none());
    }
}
