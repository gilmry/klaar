//! Séquestre : cycle de vie de l'argent d'une intervention (FR-024 à FR-027).
//!
//! **L'argent n'est pas ici, mais ses règles le sont.** Le mouvement passe par
//! Stripe, qui n'est pas provisionné ; ce que ce module tient est ce qu'aucune
//! passerelle ne tiendra à notre place : ce qu'on a le droit de capturer, ce
//! qu'on a le droit de rembourser, et l'égalité comptable qui doit valoir à
//! chaque instant. Le jour où les clés arrivent, il restera à câbler des appels
//! réseau — pas à décider de ces questions-là dans l'urgence.
//!
//! **Trois interdits, et ils sont la raison d'être du module :**
//! capturer plus qu'autorisé, rembourser plus que capturé, et rembourser après
//! que le prestataire a été payé. Les deux premiers créeraient de l'argent ; le
//! troisième le prendrait à quelqu'un qui l'a déjà reçu.
//!
//! **Tout en centimes** (Architecture §1.1). Un séquestre est le dernier
//! endroit où l'on voudrait découvrir que 0,1 + 0,2 ne fait pas 0,3.

use chrono::{DateTime, Utc};
use std::fmt;
use uuid::Uuid;

/// Durée de validité d'une pré-autorisation, en jours.
///
/// Sept. C'est la limite des réseaux de cartes : au-delà, la banque libère
/// l'empreinte d'elle-même et la capture échoue. La connaître ici évite de
/// promettre une capture que le réseau refusera.
pub const AUTORISATION_JOURS: i64 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatutSequestre {
    /// Pré-autorisé : l'empreinte est prise, rien n'a bougé.
    Autorise,
    /// Capturé : l'argent a quitté le compte du demandeur.
    Capture,
    /// Remboursé en totalité.
    RembourseTotal,
    /// Remboursé en partie ; le reste appartient encore au séquestre.
    ReemboursePartiel,
    /// Versé au prestataire. **État terminal** : il n'y a plus rien à rendre.
    Verse,
    /// L'autorisation a échoué ou expiré sans capture.
    Echoue,
}

impl StatutSequestre {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Autorise => "AUTHORIZED",
            Self::Capture => "CAPTURED",
            Self::RembourseTotal => "REFUNDED",
            Self::ReemboursePartiel => "PARTIALLY_REFUNDED",
            Self::Verse => "PAID_OUT",
            Self::Echoue => "FAILED",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "AUTHORIZED" => Some(Self::Autorise),
            "CAPTURED" => Some(Self::Capture),
            "REFUNDED" => Some(Self::RembourseTotal),
            "PARTIALLY_REFUNDED" => Some(Self::ReemboursePartiel),
            "PAID_OUT" => Some(Self::Verse),
            "FAILED" => Some(Self::Echoue),
            _ => None,
        }
    }

    /// Vrai si plus rien ne peut bouger.
    pub fn est_terminal(&self) -> bool {
        matches!(self, Self::Verse | Self::RembourseTotal | Self::Echoue)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequestreError {
    /// Montant nul ou négatif.
    MontantInvalide,
    /// Capture demandée au-delà de ce qui a été autorisé.
    CaptureDepasseAutorisation,
    /// Remboursement demandé au-delà de ce qui reste (FR-027 `@negative`).
    RemboursementDepasseSequestre,
    /// Le prestataire a déjà été payé (FR-027 `@edge`).
    VersementDejaExecute,
    /// L'autorisation a expiré : la banque a libéré l'empreinte.
    AutorisationExpiree,
    /// Le geste ne s'applique pas depuis cet état.
    TransitionInterdite { depuis: StatutSequestre },
}

impl SequestreError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::MontantInvalide => "AMOUNT_INVALID",
            Self::CaptureDepasseAutorisation => "CAPTURE_EXCEEDS_AUTHORIZATION",
            Self::RemboursementDepasseSequestre => "REFUND_EXCEEDS_ESCROW",
            Self::VersementDejaExecute => "PAYOUT_EXECUTED",
            Self::AutorisationExpiree => "AUTHORIZATION_EXPIRED",
            Self::TransitionInterdite { .. } => "ESCROW_TRANSITION_INVALID",
        }
    }
}

impl fmt::Display for SequestreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MontantInvalide => write!(f, "montant nul ou négatif"),
            Self::CaptureDepasseAutorisation => {
                write!(f, "capture supérieure au montant autorisé")
            }
            Self::RemboursementDepasseSequestre => {
                write!(f, "remboursement supérieur à ce qui reste en séquestre")
            }
            Self::VersementDejaExecute => {
                write!(f, "le prestataire a déjà été payé : plus rien à rendre")
            }
            Self::AutorisationExpiree => write!(
                f,
                "l'autorisation a plus de {AUTORISATION_JOURS} jours et a été libérée"
            ),
            Self::TransitionInterdite { depuis } => {
                write!(f, "geste impossible depuis l'état {}", depuis.as_str())
            }
        }
    }
}

impl std::error::Error for SequestreError {}

/// Le séquestre d'une intervention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sequestre {
    pub id: Uuid,
    pub mission_id: Uuid,
    /// Ce que la banque a réservé.
    pub autorise_cents: i64,
    /// Ce qui a effectivement été prélevé. Zéro tant que rien n'est capturé.
    pub capture_cents: i64,
    /// Ce qui a été rendu au demandeur.
    pub rembourse_cents: i64,
    pub statut: StatutSequestre,
    pub autorise_le: DateTime<Utc>,
}

impl Sequestre {
    /// Ouvre un séquestre sur une pré-autorisation acceptée.
    pub fn autoriser(
        mission_id: Uuid,
        montant_cents: i64,
        maintenant: DateTime<Utc>,
    ) -> Result<Self, SequestreError> {
        if montant_cents <= 0 {
            return Err(SequestreError::MontantInvalide);
        }
        Ok(Self {
            id: Uuid::new_v4(),
            mission_id,
            autorise_cents: montant_cents,
            capture_cents: 0,
            rembourse_cents: 0,
            statut: StatutSequestre::Autorise,
            autorise_le: maintenant,
        })
    }

    /// Ce qui reste en séquestre, c'est-à-dire capturé et non rendu.
    ///
    /// **La seule source du montant remboursable.** Le calculer à chaque appel
    /// plutôt que de le conserver évite qu'un champ dérive de ses composantes,
    /// ce qui est la façon habituelle dont un solde devient faux.
    pub fn solde_cents(&self) -> i64 {
        self.capture_cents - self.rembourse_cents
    }

    /// Vrai si l'autorisation est encore honorable par la banque.
    pub fn autorisation_valide(&self, maintenant: DateTime<Utc>) -> bool {
        maintenant < self.autorise_le + chrono::Duration::days(AUTORISATION_JOURS)
    }

    /// Capture tout ou partie du montant autorisé.
    ///
    /// **Une capture partielle est légitime** : une intervention peut coûter
    /// moins que le devis pré-autorisé — un déplacement seul, par exemple. Ce
    /// qui n'est pas capturé retourne au demandeur sans passer par un
    /// remboursement.
    pub fn capturer(
        &mut self,
        montant_cents: i64,
        maintenant: DateTime<Utc>,
    ) -> Result<(), SequestreError> {
        if self.statut != StatutSequestre::Autorise {
            return Err(SequestreError::TransitionInterdite {
                depuis: self.statut,
            });
        }
        if montant_cents <= 0 {
            return Err(SequestreError::MontantInvalide);
        }
        if montant_cents > self.autorise_cents {
            return Err(SequestreError::CaptureDepasseAutorisation);
        }
        if !self.autorisation_valide(maintenant) {
            return Err(SequestreError::AutorisationExpiree);
        }
        self.capture_cents = montant_cents;
        self.statut = StatutSequestre::Capture;
        Ok(())
    }

    /// Rembourse tout ou partie de ce qui a été capturé (FR-027).
    pub fn rembourser(&mut self, montant_cents: i64) -> Result<(), SequestreError> {
        // **Le versement d'abord.** L'ordre des contrôles compte : un
        // remboursement après versement doit dire « le prestataire a été payé »
        // et non « montant trop élevé », parce que la suite à donner n'est pas
        // la même — l'un se corrige en baissant le montant, l'autre se règle
        // avec le prestataire.
        if self.statut == StatutSequestre::Verse {
            return Err(SequestreError::VersementDejaExecute);
        }
        if !matches!(
            self.statut,
            StatutSequestre::Capture | StatutSequestre::ReemboursePartiel
        ) {
            return Err(SequestreError::TransitionInterdite {
                depuis: self.statut,
            });
        }
        if montant_cents <= 0 {
            return Err(SequestreError::MontantInvalide);
        }
        if montant_cents > self.solde_cents() {
            return Err(SequestreError::RemboursementDepasseSequestre);
        }

        self.rembourse_cents += montant_cents;
        self.statut = if self.solde_cents() == 0 {
            StatutSequestre::RembourseTotal
        } else {
            StatutSequestre::ReemboursePartiel
        };
        Ok(())
    }

    /// Verse au prestataire ce qui reste. **État terminal.**
    pub fn verser(&mut self) -> Result<i64, SequestreError> {
        if !matches!(
            self.statut,
            StatutSequestre::Capture | StatutSequestre::ReemboursePartiel
        ) {
            return Err(SequestreError::TransitionInterdite {
                depuis: self.statut,
            });
        }
        let solde = self.solde_cents();
        if solde <= 0 {
            // Ne peut arriver que d'un état incohérent : le remboursement total
            // bascule en `RembourseTotal`, qui n'atteint pas cette branche.
            return Err(SequestreError::MontantInvalide);
        }
        self.statut = StatutSequestre::Verse;
        Ok(solde)
    }

    /// L'autorisation a échoué, ou expiré sans capture.
    pub fn echouer(&mut self) -> Result<(), SequestreError> {
        if self.statut != StatutSequestre::Autorise {
            return Err(SequestreError::TransitionInterdite {
                depuis: self.statut,
            });
        }
        self.statut = StatutSequestre::Echoue;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 1, 9, 0, 0).unwrap()
    }

    /// 217,80 € TTC — le séquestre de l'exemple du PRD (FR-025).
    const TTC: i64 = 21_780;

    fn autorise() -> Sequestre {
        Sequestre::autoriser(Uuid::new_v4(), TTC, t0()).unwrap()
    }

    fn capture() -> Sequestre {
        let mut s = autorise();
        s.capturer(TTC, t0()).unwrap();
        s
    }

    #[test]
    fn happy_le_cycle_nominal_autorise_capture_verse() {
        let mut s = autorise();
        assert_eq!(s.solde_cents(), 0, "rien n'a bougé à l'autorisation");

        s.capturer(TTC, t0()).unwrap();
        assert_eq!(s.statut, StatutSequestre::Capture);
        assert_eq!(s.solde_cents(), TTC);

        assert_eq!(s.verser().unwrap(), TTC);
        assert_eq!(s.statut, StatutSequestre::Verse);
        assert!(s.statut.est_terminal());
    }

    #[test]
    fn happy_un_remboursement_partiel_laisse_le_reste_au_prestataire() {
        // FR-027 `@happy` : 30 % rendu, 70 % versé.
        let mut s = capture();
        s.rembourser(6_534).unwrap();
        assert_eq!(s.statut, StatutSequestre::ReemboursePartiel);
        assert_eq!(s.solde_cents(), 15_246);
        assert_eq!(s.verser().unwrap(), 15_246);
    }

    #[test]
    fn happy_un_remboursement_total_ferme_le_sequestre() {
        let mut s = capture();
        s.rembourser(TTC).unwrap();
        assert_eq!(s.statut, StatutSequestre::RembourseTotal);
        assert_eq!(s.solde_cents(), 0);
        assert!(s.statut.est_terminal());
    }

    #[test]
    fn happy_plusieurs_remboursements_partiels_s_additionnent() {
        let mut s = capture();
        s.rembourser(5_000).unwrap();
        s.rembourser(5_000).unwrap();
        assert_eq!(s.rembourse_cents, 10_000);
        assert_eq!(s.solde_cents(), TTC - 10_000);
        assert_eq!(s.statut, StatutSequestre::ReemboursePartiel);
    }

    #[test]
    fn security_on_ne_capture_pas_plus_qu_autorise() {
        // Créerait de l'argent : la banque n'a réservé que ce montant.
        let mut s = autorise();
        assert_eq!(
            s.capturer(TTC + 1, t0()),
            Err(SequestreError::CaptureDepasseAutorisation)
        );
        assert_eq!(s.statut, StatutSequestre::Autorise, "rien n'a bougé");
    }

    #[test]
    fn security_on_ne_rembourse_pas_plus_que_le_solde() {
        // FR-027 `@negative` : 422 `REFUND_EXCEEDS_ESCROW`.
        let mut s = capture();
        assert_eq!(
            s.rembourser(TTC + 1),
            Err(SequestreError::RemboursementDepasseSequestre)
        );
        s.rembourser(20_000).unwrap();
        // Le solde a baissé : le second remboursement se mesure sur ce qui
        // reste, pas sur le montant capturé au départ.
        assert_eq!(
            s.rembourser(2_000),
            Err(SequestreError::RemboursementDepasseSequestre)
        );
        assert_eq!(s.rembourse_cents, 20_000, "rien n'a bougé au refus");
    }

    #[test]
    fn security_on_ne_rembourse_pas_apres_versement() {
        // FR-027 `@edge` : 422 `PAYOUT_EXECUTED`. L'argent est chez le
        // prestataire ; le lui reprendre n'est pas une opération de paiement.
        let mut s = capture();
        s.verser().unwrap();
        assert_eq!(s.rembourser(100), Err(SequestreError::VersementDejaExecute));
    }

    #[test]
    fn security_le_code_de_refus_dit_lequel_des_deux_obstacles() {
        // Un remboursement excessif **après** versement doit dire « déjà
        // payé » : la suite à donner n'est pas la même qu'un montant trop
        // élevé, qui se corrige en baissant le montant.
        let mut s = capture();
        s.verser().unwrap();
        assert_eq!(
            s.rembourser(TTC * 10).unwrap_err().code(),
            "PAYOUT_EXECUTED"
        );
    }

    #[test]
    fn security_rien_ne_se_cree_ni_ne_disparait_sur_toute_l_echelle() {
        // L'invariant comptable, parcouru : ce qui est capturé se retrouve
        // toujours dans ce qui est rendu plus ce qui est versé.
        for part in (0..=TTC).step_by(997) {
            let mut s = capture();
            if part > 0 {
                s.rembourser(part).unwrap();
            }
            let verse = if s.solde_cents() > 0 {
                s.verser().unwrap()
            } else {
                0
            };
            assert_eq!(
                s.rembourse_cents + verse,
                TTC,
                "part remboursée {part} : {} + {verse} ≠ {TTC}",
                s.rembourse_cents
            );
        }
    }

    #[test]
    fn negative_une_capture_apres_expiration_est_refusee() {
        // Au-delà de sept jours la banque a libéré l'empreinte : promettre la
        // capture ferait échouer le réseau, et le demandeur ne comprendrait pas.
        let mut s = autorise();
        let tard = t0() + Duration::days(AUTORISATION_JOURS) + Duration::seconds(1);
        assert_eq!(
            s.capturer(TTC, tard),
            Err(SequestreError::AutorisationExpiree)
        );
    }

    #[test]
    fn edge_la_capture_a_la_borne_exacte_passe_encore() {
        let mut s = autorise();
        let limite = t0() + Duration::days(AUTORISATION_JOURS) - Duration::seconds(1);
        assert_eq!(s.capturer(TTC, limite), Ok(()));
    }

    #[test]
    fn edge_une_capture_partielle_est_legitime() {
        // Un déplacement seul, sans réparation : ce qui n'est pas capturé
        // retourne au demandeur sans passer par un remboursement.
        let mut s = autorise();
        s.capturer(3_000, t0()).unwrap();
        assert_eq!(s.solde_cents(), 3_000);
        assert_eq!(
            s.autorise_cents, TTC,
            "l'autorisation reste ce qu'elle était"
        );
    }

    #[test]
    fn negative_les_montants_nuls_ou_negatifs_sont_refuses() {
        assert_eq!(
            Sequestre::autoriser(Uuid::new_v4(), 0, t0()),
            Err(SequestreError::MontantInvalide)
        );
        assert_eq!(
            Sequestre::autoriser(Uuid::new_v4(), -1, t0()),
            Err(SequestreError::MontantInvalide)
        );
        let mut s = autorise();
        assert_eq!(s.capturer(0, t0()), Err(SequestreError::MontantInvalide));
        let mut s = capture();
        assert_eq!(s.rembourser(0), Err(SequestreError::MontantInvalide));
        assert_eq!(s.rembourser(-100), Err(SequestreError::MontantInvalide));
    }

    #[test]
    fn negative_aucun_geste_ne_repart_d_un_etat_terminal() {
        for construire in [
            // Versé.
            (|| {
                let mut s = capture();
                s.verser().unwrap();
                s
            }) as fn() -> Sequestre,
            // Remboursé en totalité.
            || {
                let mut s = capture();
                s.rembourser(TTC).unwrap();
                s
            },
            // Autorisation échouée.
            || {
                let mut s = autorise();
                s.echouer().unwrap();
                s
            },
        ] {
            let mut s = construire();
            assert!(s.statut.est_terminal(), "{:?}", s.statut);
            assert!(s.capturer(100, t0()).is_err());
            assert!(s.rembourser(100).is_err());
            assert!(s.verser().is_err());
            assert!(s.echouer().is_err());
        }
    }

    #[test]
    fn negative_on_ne_capture_pas_deux_fois() {
        // Sans cette garde, un webhook de capture arrivé en double
        // prélèverait deux fois.
        let mut s = capture();
        assert!(matches!(
            s.capturer(TTC, t0()),
            Err(SequestreError::TransitionInterdite { .. })
        ));
    }

    #[test]
    fn negative_on_ne_rembourse_pas_une_autorisation_non_capturee() {
        // Rien n'a été prélevé : il n'y a rien à rendre. Le geste attendu est
        // d'abandonner l'autorisation, pas de rembourser.
        let mut s = autorise();
        assert!(matches!(
            s.rembourser(100),
            Err(SequestreError::TransitionInterdite { .. })
        ));
    }

    #[test]
    fn edge_le_vocabulaire_fait_l_aller_retour() {
        for statut in [
            StatutSequestre::Autorise,
            StatutSequestre::Capture,
            StatutSequestre::RembourseTotal,
            StatutSequestre::ReemboursePartiel,
            StatutSequestre::Verse,
            StatutSequestre::Echoue,
        ] {
            assert_eq!(StatutSequestre::parse(statut.as_str()), Some(statut));
        }
        assert_eq!(StatutSequestre::parse("PEUT_ETRE"), None);
    }
}
