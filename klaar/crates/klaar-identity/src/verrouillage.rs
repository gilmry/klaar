//! Verrouillage après échecs répétés (FR-007, Story 1.8).
//!
//! **Le verrou est une arme à double tranchant.** Il coupe le brute-force, mais
//! il offre aussi à n'importe qui le moyen de bloquer le compte d'autrui en
//! échouant cinq fois sur son adresse. Trois choix limitent ce retournement :
//!
//! - la durée est **courte** (15 minutes) et le compte se rouvre seul ;
//! - les échecs ne comptent que dans une **fenêtre glissante** de dix minutes,
//!   si bien que cinq échecs étalés sur la journée ne verrouillent rien ;
//! - la limitation par adresse IP, qui plafonne à cinq tentatives par heure,
//!   reste en première ligne : atteindre le verrou depuis une seule source
//!   demande déjà de la contourner.

use chrono::{DateTime, Duration, Utc};

/// Échecs consécutifs avant verrouillage (FR-007).
pub const MAX_ECHECS: i32 = 5;
/// Fenêtre au-delà de laquelle le compteur d'échecs repart de zéro.
pub const FENETRE_ECHECS_MINUTES: i64 = 10;
/// Durée du verrou.
pub const DUREE_VERROU_MINUTES: i64 = 15;

/// État de verrouillage d'un compte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Verrouillage {
    pub echecs_consecutifs: i32,
    pub dernier_echec_le: Option<DateTime<Utc>>,
    pub verrouille_jusqu_a: Option<DateTime<Utc>>,
}

impl Verrouillage {
    pub fn est_verrouille(&self, maintenant: DateTime<Utc>) -> bool {
        self.verrouille_jusqu_a
            .is_some_and(|jusqu_a| jusqu_a > maintenant)
    }

    /// Secondes restantes avant réouverture, si le compte est verrouillé.
    pub fn secondes_restantes(&self, maintenant: DateTime<Utc>) -> Option<i64> {
        let jusqu_a = self.verrouille_jusqu_a?;
        if jusqu_a <= maintenant {
            return None;
        }
        // Arrondi vers le haut : annoncer 0 à quelqu'un qui doit encore
        // attendre une fraction de seconde le ferait réessayer pour rien.
        Some((jusqu_a - maintenant).num_seconds().max(1))
    }

    /// Enregistre un échec et rend l'état résultant.
    ///
    /// Le compteur repart de zéro si le dernier échec est plus vieux que la
    /// fenêtre : cinq échecs étalés sur une journée ne sont pas une attaque,
    /// ce sont cinq oublis.
    #[must_use]
    pub fn apres_echec(self, maintenant: DateTime<Utc>) -> Self {
        let dans_la_fenetre = self.dernier_echec_le.is_some_and(|dernier| {
            maintenant - dernier <= Duration::minutes(FENETRE_ECHECS_MINUTES)
        });
        let echecs = if dans_la_fenetre {
            self.echecs_consecutifs + 1
        } else {
            1
        };

        // Un verrou déjà en cours n'est **pas** repoussé par les échecs qui le
        // suivent. Sans cette condition, chaque tentative repoussait la fin de
        // quinze minutes, et un tiers maintenait un compte fermé indéfiniment
        // en martelant la route — soit exactement l'attaque que le verrou
        // prétend arrêter. Le premier jet posait bien le commentaire, et pas la
        // condition ; c'est un test qui l'a montré.
        //
        // Un nouveau verrou peut en revanche succéder à un verrou expiré si les
        // échecs continuent, et c'est voulu : une attaque qui dure doit
        // continuer d'être bloquée.
        let verrouille_jusqu_a = if echecs >= MAX_ECHECS && !self.est_verrouille(maintenant) {
            Some(maintenant + Duration::minutes(DUREE_VERROU_MINUTES))
        } else {
            self.verrouille_jusqu_a
        };

        Self {
            echecs_consecutifs: echecs,
            dernier_echec_le: Some(maintenant),
            verrouille_jusqu_a,
        }
    }

    /// Remet le compteur à zéro après une authentification réussie.
    #[must_use]
    pub fn apres_succes() -> Self {
        Self::default()
    }

    /// Vrai si cet état vient de franchir le seuil.
    ///
    /// Sert à n'alerter qu'une fois : sans cela, chaque échec suivant enverrait
    /// un courriel de plus, ce qui transforme l'alerte en nuisance et le
    /// service en relais de spam.
    pub fn vient_de_verrouiller(&self, precedent: &Self) -> bool {
        self.echecs_consecutifs >= MAX_ECHECS && precedent.echecs_consecutifs < MAX_ECHECS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn instant() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    /// Enchaîne `n` échecs espacés d'une minute.
    fn echecs(n: i32) -> (Verrouillage, DateTime<Utc>) {
        let mut etat = Verrouillage::default();
        let mut t = instant();
        for _ in 0..n {
            etat = etat.apres_echec(t);
            t += Duration::minutes(1);
        }
        (etat, t)
    }

    #[test]
    fn happy_un_compte_neuf_n_est_pas_verrouille() {
        assert!(!Verrouillage::default().est_verrouille(instant()));
        assert_eq!(Verrouillage::default().secondes_restantes(instant()), None);
    }

    #[test]
    fn happy_quatre_echecs_ne_verrouillent_pas() {
        let (etat, t) = echecs(4);
        assert_eq!(etat.echecs_consecutifs, 4);
        assert!(!etat.est_verrouille(t));
    }

    #[test]
    fn happy_le_cinquieme_echec_verrouille_quinze_minutes() {
        let (etat, _) = echecs(5);
        let dernier = instant() + Duration::minutes(4);
        assert!(etat.est_verrouille(dernier));
        assert_eq!(
            etat.verrouille_jusqu_a,
            Some(dernier + Duration::minutes(15))
        );
    }

    #[test]
    fn happy_le_verrou_expire_de_lui_meme() {
        let (etat, _) = echecs(5);
        let apres = instant() + Duration::minutes(4) + Duration::minutes(16);
        assert!(!etat.est_verrouille(apres));
        assert_eq!(etat.secondes_restantes(apres), None);
    }

    #[test]
    fn happy_un_succes_remet_le_compteur_a_zero() {
        let apres = Verrouillage::apres_succes();
        assert_eq!(apres.echecs_consecutifs, 0);
        assert!(!apres.est_verrouille(instant()));
        assert_eq!(apres.verrouille_jusqu_a, None);
    }

    #[test]
    fn negative_cinq_echecs_etales_ne_verrouillent_pas() {
        // Cinq oublis dans la journée ne sont pas une attaque. Sans fenêtre,
        // le compteur ne redescendrait jamais et finirait par verrouiller un
        // utilisateur simplement distrait.
        let mut etat = Verrouillage::default();
        let mut t = instant();
        for _ in 0..5 {
            etat = etat.apres_echec(t);
            t += Duration::hours(1);
        }
        assert_eq!(etat.echecs_consecutifs, 1);
        assert!(!etat.est_verrouille(t));
    }

    #[test]
    fn edge_le_compteur_repart_juste_apres_la_fenetre() {
        let etat = Verrouillage::default().apres_echec(instant());
        let dans = etat.apres_echec(instant() + Duration::minutes(FENETRE_ECHECS_MINUTES));
        assert_eq!(dans.echecs_consecutifs, 2, "à la limite, encore dedans");

        let dehors = etat.apres_echec(
            instant() + Duration::minutes(FENETRE_ECHECS_MINUTES) + Duration::seconds(1),
        );
        assert_eq!(dehors.echecs_consecutifs, 1, "au-delà, on repart");
    }

    #[test]
    fn edge_le_delai_annonce_decroit_avec_le_temps() {
        let (etat, _) = echecs(5);
        let verrouille_a = instant() + Duration::minutes(4);
        let t0 = etat.secondes_restantes(verrouille_a).unwrap();
        let t1 = etat
            .secondes_restantes(verrouille_a + Duration::minutes(5))
            .unwrap();
        assert_eq!(t0, 15 * 60);
        assert_eq!(t0 - t1, 5 * 60);
    }

    #[test]
    fn edge_le_delai_ne_tombe_jamais_a_zero_tant_que_le_verrou_tient() {
        let (etat, _) = echecs(5);
        let juste_avant = etat.verrouille_jusqu_a.unwrap() - Duration::milliseconds(1);
        assert_eq!(etat.secondes_restantes(juste_avant), Some(1));
    }

    #[test]
    fn security_marteler_un_compte_verrouille_ne_prolonge_pas_le_verrou() {
        // Sinon, un tiers maintient un compte fermé indéfiniment en réessayant
        // — soit exactement l'attaque que le verrou prétend arrêter.
        let (etat, t) = echecs(5);
        let fin_initiale = etat.verrouille_jusqu_a.unwrap();

        let mut martele = etat;
        let mut t = t;
        for _ in 0..20 {
            martele = martele.apres_echec(t);
            t += Duration::seconds(10);
        }
        assert_eq!(
            martele.verrouille_jusqu_a,
            Some(fin_initiale),
            "le verrou ne doit pas être repoussé"
        );
    }

    #[test]
    fn security_une_attaque_qui_dure_reverrouille_apres_expiration() {
        // Le pendant du test précédent. Ne pas prolonger un verrou en cours ne
        // doit pas vouloir dire laisser passer une attaque qui continue au-delà
        // des quinze minutes.
        let (etat, _) = echecs(5);
        let fin = etat.verrouille_jusqu_a.unwrap();

        // L'attaquant doit continuer d'échouer pendant le verrou pour rester
        // dans la fenêtre de comptage. S'il s'arrête, le compteur repart de
        // zéro à sa prochaine tentative — et c'est correct : quinze minutes de
        // silence ne ressemblent plus à une attaque.
        let mut martele = etat;
        let mut t = instant() + Duration::minutes(5);
        while t < fin {
            martele = martele.apres_echec(t);
            assert_eq!(
                martele.verrouille_jusqu_a,
                Some(fin),
                "le verrou en cours ne doit pas bouger"
            );
            t += Duration::minutes(1);
        }

        let juste_apres = fin + Duration::seconds(1);
        let reverrouille = martele.apres_echec(juste_apres);
        assert!(reverrouille.est_verrouille(juste_apres));
        assert!(reverrouille.verrouille_jusqu_a.unwrap() > fin);
    }

    #[test]
    fn security_l_alerte_n_est_levee_qu_une_fois() {
        // Une alerte par échec transformerait le service en relais de courriels
        // vers une adresse que son titulaire n'a pas sollicitée.
        let (quatre, t) = echecs(4);
        let cinq = quatre.apres_echec(t);
        assert!(cinq.vient_de_verrouiller(&quatre));

        let six = cinq.apres_echec(t + Duration::seconds(10));
        assert!(!six.vient_de_verrouiller(&cinq));
    }

    #[test]
    fn security_un_verrou_expire_ne_reverrouille_pas_au_premier_echec_suivant() {
        let (etat, _) = echecs(5);
        // Bien après la fin du verrou et de la fenêtre.
        let plus_tard = instant() + Duration::hours(2);
        let suivant = etat.apres_echec(plus_tard);
        assert_eq!(suivant.echecs_consecutifs, 1);
        assert!(!suivant.est_verrouille(plus_tard));
    }
}
