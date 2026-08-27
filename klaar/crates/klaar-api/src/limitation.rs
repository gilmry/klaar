//! Limitation de débit par adresse IP (FR-001 `@edge` : 5 inscriptions par IP
//! et par heure).
//!
//! **Portée réelle, à ne pas surestimer.** Le compteur vit en mémoire du
//! processus. Il tient donc pour un déploiement à un seul exemplaire, et
//! seulement jusqu'au redémarrage. Derrière plusieurs instances, chacune
//! compterait pour elle et la limite effective serait multipliée d'autant.
//! C'est suffisant à ce stade et écrit dans `COMPLIANCE.md` ; la version
//! partagée (Redis ou table dédiée) viendra avec le déploiement réel, bloqué
//! par l'absence de compte OVH.
//!
//! **RGPD.** L'adresse IP n'est pas conservée : la clé est son empreinte
//! SHA-256, tronquée à 16 octets. Cela suffit à compter, et un vidage de la
//! mémoire ne rend pas la liste des adresses ayant tenté de s'inscrire. La
//! finalité — borner l'abus — est celle qui rend ce traitement licite, et elle
//! n'a pas besoin de l'adresse elle-même.

use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;

/// Tentatives autorisées par fenêtre.
pub const MAX_PAR_FENETRE: usize = 5;
/// Durée de la fenêtre, en secondes. Reprise telle quelle dans `Retry-After`.
pub const FENETRE_SECONDES: i64 = 3600;

/// Nombre de clés au-delà duquel un nettoyage complet est déclenché.
///
/// Sans lui, une avalanche d'adresses distinctes fait croître la table sans
/// fin : la limitation de débit deviendrait elle-même le vecteur d'épuisement
/// mémoire qu'elle est censée prévenir.
const SEUIL_NETTOYAGE: usize = 10_000;

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Autorise,
    /// Refusé, avec le nombre de secondes à attendre.
    Refuse {
        retry_after: i64,
    },
}

#[derive(Default)]
pub struct LimiteurMemoire {
    tentatives: Mutex<HashMap<[u8; 16], Vec<DateTime<Utc>>>>,
}

fn cle(ip: &str) -> [u8; 16] {
    let condense = Sha256::digest(ip.as_bytes());
    let mut cle = [0u8; 16];
    cle.copy_from_slice(&condense[..16]);
    cle
}

impl LimiteurMemoire {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enregistre une tentative et dit si elle est autorisée.
    ///
    /// Fenêtre glissante et non fenêtre fixe : avec une fenêtre fixe, dix
    /// tentatives passent en quelques secondes à cheval sur la bascule
    /// horaire, ce qui vide la limite de son sens précisément au moment où
    /// elle devrait tenir.
    pub fn verifier(&self, ip: &str, maintenant: DateTime<Utc>) -> Verdict {
        let debut = maintenant - Duration::seconds(FENETRE_SECONDES);
        let mut tentatives = self
            .tentatives
            .lock()
            .unwrap_or_else(|empoisonne| empoisonne.into_inner());

        if tentatives.len() > SEUIL_NETTOYAGE {
            tentatives.retain(|_, dates| {
                dates.retain(|d| *d > debut);
                !dates.is_empty()
            });
        }

        let dates = tentatives.entry(cle(ip)).or_default();
        dates.retain(|d| *d > debut);

        if dates.len() >= MAX_PAR_FENETRE {
            // Le délai annoncé est celui qui libère réellement une place :
            // l'expiration de la plus ancienne tentative encore comptée.
            // Annoncer la fenêtre entière ferait attendre pour rien.
            let plus_ancienne = dates.iter().min().copied().unwrap_or(maintenant);
            let libre_le = plus_ancienne + Duration::seconds(FENETRE_SECONDES);
            let retry_after = (libre_le - maintenant).num_seconds().max(1);
            return Verdict::Refuse { retry_after };
        }

        dates.push(maintenant);
        Verdict::Autorise
    }

    #[cfg(test)]
    fn nombre_de_cles(&self) -> usize {
        self.tentatives.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn instant() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    #[test]
    fn happy_les_cinq_premieres_tentatives_passent() {
        let l = LimiteurMemoire::new();
        for i in 0..MAX_PAR_FENETRE {
            assert_eq!(
                l.verifier("1.2.3.4", instant()),
                Verdict::Autorise,
                "tentative {i}"
            );
        }
    }

    #[test]
    fn negative_la_sixieme_est_refusee_avec_un_delai() {
        let l = LimiteurMemoire::new();
        for _ in 0..MAX_PAR_FENETRE {
            l.verifier("1.2.3.4", instant());
        }
        match l.verifier("1.2.3.4", instant()) {
            Verdict::Refuse { retry_after } => assert_eq!(retry_after, FENETRE_SECONDES),
            Verdict::Autorise => panic!("la 6e tentative aurait dû être refusée"),
        }
    }

    #[test]
    fn negative_un_refus_ne_prolonge_pas_la_punition() {
        // Une tentative refusée ne doit pas être comptée : sinon, marteler la
        // route repousse indéfiniment la réouverture, et un tiers peut
        // maintenir une adresse bloquée en continu.
        let l = LimiteurMemoire::new();
        for _ in 0..MAX_PAR_FENETRE {
            l.verifier("1.2.3.4", instant());
        }
        for _ in 0..20 {
            l.verifier("1.2.3.4", instant() + Duration::seconds(10));
        }
        assert_eq!(
            l.verifier(
                "1.2.3.4",
                instant() + Duration::seconds(FENETRE_SECONDES + 1)
            ),
            Verdict::Autorise
        );
    }

    #[test]
    fn edge_la_fenetre_glisse_tentative_par_tentative() {
        let l = LimiteurMemoire::new();
        // Cinq tentatives espacées d'une minute.
        for i in 0..MAX_PAR_FENETRE as i64 {
            assert_eq!(
                l.verifier("1.2.3.4", instant() + Duration::seconds(60 * i)),
                Verdict::Autorise
            );
        }
        // Une heure après la première : elle sort de la fenêtre, une place
        // se libère, mais une seule.
        let apres = instant() + Duration::seconds(FENETRE_SECONDES + 1);
        assert_eq!(l.verifier("1.2.3.4", apres), Verdict::Autorise);
        assert!(matches!(
            l.verifier("1.2.3.4", apres),
            Verdict::Refuse { .. }
        ));
    }

    #[test]
    fn edge_deux_adresses_ont_chacune_leur_compteur() {
        let l = LimiteurMemoire::new();
        for _ in 0..MAX_PAR_FENETRE {
            l.verifier("1.2.3.4", instant());
        }
        assert!(matches!(
            l.verifier("1.2.3.4", instant()),
            Verdict::Refuse { .. }
        ));
        assert_eq!(l.verifier("5.6.7.8", instant()), Verdict::Autorise);
    }

    #[test]
    fn edge_le_delai_annonce_decroit_avec_le_temps() {
        let l = LimiteurMemoire::new();
        for _ in 0..MAX_PAR_FENETRE {
            l.verifier("1.2.3.4", instant());
        }
        let Verdict::Refuse { retry_after: t0 } = l.verifier("1.2.3.4", instant()) else {
            panic!("refus attendu");
        };
        let Verdict::Refuse { retry_after: t1 } =
            l.verifier("1.2.3.4", instant() + Duration::seconds(600))
        else {
            panic!("refus attendu");
        };
        assert_eq!(t0 - t1, 600);
    }

    #[test]
    fn security_l_adresse_n_est_pas_conservee_en_clair() {
        let l = LimiteurMemoire::new();
        l.verifier("192.0.2.42", instant());
        let tentatives = l.tentatives.lock().unwrap();
        let clefs: Vec<_> = tentatives.keys().collect();
        assert_eq!(clefs.len(), 1);
        // La clé est une empreinte : la chaîne d'origine ne s'y retrouve pas.
        let brut: Vec<u8> = clefs[0].to_vec();
        assert!(!String::from_utf8_lossy(&brut).contains("192.0.2.42"));
        assert_eq!(brut.len(), 16);
    }

    #[test]
    fn security_la_table_ne_croit_pas_sans_borne() {
        let l = LimiteurMemoire::new();
        // Des adresses toutes différentes, puis un saut au-delà de la fenêtre :
        // le nettoyage doit vider ce qui a expiré.
        for i in 0..(SEUIL_NETTOYAGE + 100) {
            l.verifier(&format!("10.0.{}.{}", i / 256, i % 256), instant());
        }
        assert!(l.nombre_de_cles() > SEUIL_NETTOYAGE);
        l.verifier(
            "1.2.3.4",
            instant() + Duration::seconds(FENETRE_SECONDES + 1),
        );
        assert!(
            l.nombre_de_cles() < 10,
            "les tentatives expirées auraient dû être purgées, restant : {}",
            l.nombre_de_cles()
        );
    }

    #[test]
    fn security_un_verrou_empoisonne_ne_fait_pas_tomber_la_limitation() {
        // Un panic dans un autre fil ne doit pas transformer la limitation en
        // panne totale de l'inscription, ni la désactiver silencieusement.
        let l = std::sync::Arc::new(LimiteurMemoire::new());
        let l2 = l.clone();
        let _ = std::thread::spawn(move || {
            let _garde = l2.tentatives.lock().unwrap();
            panic!("empoisonne le verrou");
        })
        .join();
        assert_eq!(l.verifier("1.2.3.4", instant()), Verdict::Autorise);
    }
}
