//! Répartition et libération de l'argent d'une Mission (FR-021, FR-025).
//!
//! **Ce qui est livré, et ce qui ne l'est pas.** FR-021 fait de la validation le
//! moment où Stripe libère le séquestre. Le compte n'est pas ouvert : ce module
//! calcule la répartition, décide si elle est immédiatement autorisée, et
//! l'enregistre. Le virement lui-même rejoindra l'Epic 5. La différence est
//! visible dans le vocabulaire — on prononce une libération, on ne verse pas.
//!
//! **Le calcul est en centimes entiers, et il est vérifié sur l'exemple du
//! PRD.** 180 € HTVA à 21 % font 217,80 € TTC ; la commission de 18 % sur le
//! HTVA fait 32,40 €, sa TVA 6,80 €, donc 39,20 € TTC ; il reste 178,60 € au
//! prestataire. Ces cinq nombres sont dans un test, parce qu'une erreur d'un
//! centime sur une répartition se retrouve dans une comptabilité.

use chrono::{DateTime, Duration, Utc};
use klaar_shared_kernel::{Money, VatRate};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use crate::devis::Devis;

/// Commission de la plateforme, en points de base : 18 % (PRD, FR-025).
pub const TAUX_COMMISSION_BP: u16 = 1_800;

/// Délai au-delà duquel la validation devient automatique (FR-021 `@edge`).
///
/// Soixante-douze heures. Sans ce délai, un demandeur qui ne rouvre jamais
/// l'application retiendrait indéfiniment l'argent d'un travail fait ; c'est le
/// prestataire qui en paierait le silence.
pub const DELAI_VALIDATION_HEURES: i64 = 72;

/// Au-delà de ce montant TTC, la libération attend un second regard (FR-021).
///
/// Cinq cents euros. La règle des quatre yeux ne dit pas que le devis est
/// suspect : elle dit qu'une erreur au-dessus de ce montant coûte assez cher
/// pour mériter d'être vue par quelqu'un avant qu'elle ne parte.
pub const SEUIL_QUATRE_YEUX_CENTS: i64 = 50_000;

/// Comment l'argent se partage.
// `Eq` tient : tout est en centimes entiers, aucun flottant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repartition {
    /// Ce que le demandeur doit, tel qu'il l'a accepté.
    pub total_ttc: Money,
    /// Commission de la plateforme, hors TVA.
    pub commission_htva: Money,
    /// TVA due sur la commission, au taux normal.
    pub tva_commission: Money,
    pub commission_ttc: Money,
    /// Ce qui revient au prestataire.
    pub reversement: Money,
}

/// Calcule la répartition d'un devis accepté.
///
/// **La commission porte sur le montant hors TVA**, jamais sur le TTC : la TVA
/// du devis est due à l'État, pas à la plateforme, et en prélever une part
/// reviendrait à se servir dans une taxe. Le PRD le dit ainsi, et c'est aussi
/// la seule lecture défendable devant un contrôle.
pub fn repartir(devis: &Devis) -> Repartition {
    let commission_htva = VatRate::from_basis_points(TAUX_COMMISSION_BP)
        // 1800 est inférieur à 10 000 : la construction ne peut pas échouer, et
        // un `expect` ici dit la vérité plutôt que de propager une erreur qui
        // n'arrivera jamais.
        .expect("taux de commission dans les bornes")
        .apply(devis.montant_htva.cents());
    // La commission est un service, donc soumise à TVA au taux normal — celui
    // de la plateforme, pas celui du devis : un dépannage à 6 % ne rend pas la
    // commission facturable à 6 %.
    let tva_commission = VatRate::BELGIUM_STANDARD.apply(commission_htva);
    let commission_ttc = commission_htva + tva_commission;

    Repartition {
        total_ttc: devis.total_ttc,
        commission_htva: Money::from_cents(commission_htva),
        tva_commission: Money::from_cents(tva_commission),
        commission_ttc: Money::from_cents(commission_ttc),
        reversement: Money::from_cents(devis.total_ttc.cents() - commission_ttc),
    }
}

/// Qui a validé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrigineValidation {
    /// Le demandeur, de sa main.
    Demandeur,
    /// Le balayage, après le délai de FR-021 `@edge`.
    Automatique,
}

impl OrigineValidation {
    /// Vocabulaire d'audit, figé parce qu'il sort du code.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Demandeur => "USER_VALIDATION",
            Self::Automatique => "AUTO_RELEASE_72H",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "USER_VALIDATION" => Some(Self::Demandeur),
            "AUTO_RELEASE_72H" => Some(Self::Automatique),
            _ => None,
        }
    }
}

/// Où en est la libération.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatutLiberation {
    /// Prononcée, et rien ne la retient.
    Autorisee,
    /// Au-dessus du seuil : elle attend un second regard (FR-021 `@edge`).
    EnAttenteOps,
}

impl StatutLiberation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Autorisee => "AUTHORISED",
            Self::EnAttenteOps => "PENDING_OPS",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "AUTHORISED" => Some(Self::Autorisee),
            "PENDING_OPS" => Some(Self::EnAttenteOps),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiberationError {
    /// Le devis n'a pas été accepté : il n'y a pas d'accord à honorer.
    DevisNonAccepte,
}

impl LiberationError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::DevisNonAccepte => "QUOTE_NOT_ACCEPTED",
        }
    }
}

impl fmt::Display for LiberationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DevisNonAccepte => write!(f, "aucun devis accepté pour cette Mission"),
        }
    }
}

impl std::error::Error for LiberationError {}

/// La décision, telle qu'elle sera consignée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Liberation {
    pub id: Uuid,
    pub mission_id: Uuid,
    /// Le devis qui fixe le montant. Conservé : la répartition ne se relit pas
    /// sans savoir sur quel accord elle portait.
    pub devis_id: Uuid,
    pub provider_id: Uuid,
    pub repartition: Repartition,
    pub origine: OrigineValidation,
    pub statut: StatutLiberation,
    pub decidee_le: DateTime<Utc>,
}

impl Liberation {
    /// Prononce la libération pour un devis accepté.
    ///
    /// Aucun paramètre ne permet de la créer dans un autre statut que celui que
    /// le montant impose : le seuil des quatre yeux n'est pas une option
    /// d'appel, sinon il suffirait de ne pas la passer.
    pub fn prononcer(
        mission_id: Uuid,
        devis: &Devis,
        origine: OrigineValidation,
        maintenant: DateTime<Utc>,
    ) -> Result<Self, LiberationError> {
        if devis.statut != crate::devis::StatutDevis::Accepte {
            return Err(LiberationError::DevisNonAccepte);
        }
        let repartition = repartir(devis);
        Ok(Self {
            id: Uuid::new_v4(),
            mission_id,
            devis_id: devis.id,
            provider_id: devis.provider_id,
            statut: if repartition.total_ttc.cents() > SEUIL_QUATRE_YEUX_CENTS {
                StatutLiberation::EnAttenteOps
            } else {
                StatutLiberation::Autorisee
            },
            repartition,
            origine,
            decidee_le: maintenant,
        })
    }
}

/// Instant à partir duquel une Mission terminée se valide toute seule.
pub fn echeance_validation(terminee_le: DateTime<Utc>) -> DateTime<Utc> {
    terminee_le + Duration::hours(DELAI_VALIDATION_HEURES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devis::{Proposition, StatutDevis};
    use chrono::TimeZone;

    fn t0() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    fn devis_accepte(montant_htva_cents: i64) -> Devis {
        let mut devis = Devis::emettre(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Proposition {
                montant_htva_cents,
                taux_tva_bp: 2100,
                delai_minutes: 45,
                note: None,
                preuve_tva_reduite: None,
            },
            t0(),
        )
        .expect("devis valide");
        devis.accepter(t0()).expect("acceptation");
        devis
    }

    // === @happy ===

    #[test]
    fn happy_la_repartition_de_l_exemple_du_prd_au_centime_pres() {
        // FR-025 : 217,80 € TTC, commission 32,40 € HTVA, TVA 6,80 €,
        // commission 39,20 € TTC, reversement 178,60 €. Les cinq nombres.
        let r = repartir(&devis_accepte(18_000));
        assert_eq!(r.total_ttc.cents(), 21_780);
        assert_eq!(r.commission_htva.cents(), 3_240);
        assert_eq!(r.tva_commission.cents(), 680);
        assert_eq!(r.commission_ttc.cents(), 3_920);
        assert_eq!(r.reversement.cents(), 17_860);
    }

    #[test]
    fn happy_une_liberation_sous_le_seuil_est_autorisee() {
        let liberation = Liberation::prononcer(
            Uuid::new_v4(),
            &devis_accepte(18_000),
            OrigineValidation::Demandeur,
            t0(),
        )
        .expect("prononcée");
        assert_eq!(liberation.statut, StatutLiberation::Autorisee);
        assert_eq!(liberation.origine.as_str(), "USER_VALIDATION");
    }

    // === @negative ===

    #[test]
    fn negative_un_devis_non_accepte_ne_libere_rien() {
        // Sans accord, il n'y a pas de montant dû : prononcer une libération
        // reviendrait à décider seul de ce que quelqu'un doit payer.
        let mut devis = devis_accepte(18_000);
        devis.statut = StatutDevis::Envoye;
        assert_eq!(
            Liberation::prononcer(Uuid::new_v4(), &devis, OrigineValidation::Demandeur, t0()),
            Err(LiberationError::DevisNonAccepte)
        );
    }

    #[test]
    fn negative_un_devis_refuse_ne_libere_rien() {
        let mut devis = devis_accepte(18_000);
        devis.statut = StatutDevis::Refuse;
        assert!(Liberation::prononcer(
            Uuid::new_v4(),
            &devis,
            OrigineValidation::Automatique,
            t0()
        )
        .is_err());
    }

    // === @edge ===

    #[test]
    fn edge_au_dela_du_seuil_la_liberation_attend_un_second_regard() {
        // 500 € TTC pile passent ; un centime de plus attend.
        let sous_le_seuil = Liberation::prononcer(
            Uuid::new_v4(),
            // 413,22 € HTVA à 21 % font 499,99 € TTC.
            &devis_accepte(41_322),
            OrigineValidation::Demandeur,
            t0(),
        )
        .unwrap();
        assert!(sous_le_seuil.repartition.total_ttc.cents() <= SEUIL_QUATRE_YEUX_CENTS);
        assert_eq!(sous_le_seuil.statut, StatutLiberation::Autorisee);

        let au_dessus = Liberation::prononcer(
            Uuid::new_v4(),
            &devis_accepte(50_000),
            OrigineValidation::Demandeur,
            t0(),
        )
        .unwrap();
        assert!(au_dessus.repartition.total_ttc.cents() > SEUIL_QUATRE_YEUX_CENTS);
        assert_eq!(au_dessus.statut, StatutLiberation::EnAttenteOps);
    }

    #[test]
    fn edge_un_devis_a_taux_reduit_paie_sa_commission_au_taux_normal() {
        // La commission est un service de la plateforme : un dépannage à 6 % ne
        // rend pas la commission facturable à 6 %.
        let mut devis = Devis::emettre(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Proposition {
                montant_htva_cents: 18_000,
                taux_tva_bp: 600,
                delai_minutes: 45,
                note: None,
                preuve_tva_reduite: Some("logement de 1974".to_string()),
            },
            t0(),
        )
        .unwrap();
        devis.accepter(t0()).unwrap();

        let r = repartir(&devis);
        assert_eq!(r.total_ttc.cents(), 19_080);
        assert_eq!(r.commission_htva.cents(), 3_240);
        // 21 % de la commission, et non 6 %.
        assert_eq!(r.tva_commission.cents(), 680);
        assert_eq!(r.reversement.cents(), 19_080 - 3_920);
    }

    #[test]
    fn edge_l_echeance_de_validation_tombe_a_soixante_douze_heures() {
        assert_eq!(
            (echeance_validation(t0()) - t0()).num_hours(),
            DELAI_VALIDATION_HEURES
        );
    }

    #[test]
    fn edge_un_montant_d_un_centime_ne_produit_pas_de_commission_negative() {
        // La troncature peut ramener la commission à zéro ; le reversement doit
        // rester le total, jamais un nombre négatif.
        let r = repartir(&devis_accepte(1));
        assert_eq!(r.commission_htva.cents(), 0);
        assert_eq!(r.commission_ttc.cents(), 0);
        assert_eq!(r.reversement.cents(), r.total_ttc.cents());
    }

    // === @security ===

    #[test]
    fn security_la_somme_des_parts_fait_toujours_le_total() {
        // L'invariant comptable : rien ne se crée, rien ne disparaît. Le
        // vérifier sur toute l'échelle attrape une erreur d'arrondi qu'un seul
        // exemple laisserait passer.
        for cents in [1, 999, 5_000, 18_000, 41_322, 50_000, 99_999, 1_000_000] {
            let r = repartir(&devis_accepte(cents));
            assert_eq!(
                r.commission_ttc.cents() + r.reversement.cents(),
                r.total_ttc.cents(),
                "montant {cents}"
            );
            assert_eq!(
                r.commission_htva.cents() + r.tva_commission.cents(),
                r.commission_ttc.cents(),
                "montant {cents}"
            );
        }
    }

    #[test]
    fn security_la_commission_ne_touche_jamais_la_tva_du_devis() {
        // La TVA du devis est due à l'État, pas à la plateforme. En prélever
        // une part reviendrait à se servir dans une taxe.
        for cents in [5_000, 18_000, 50_000] {
            let devis = devis_accepte(cents);
            let r = repartir(&devis);
            // Ce que le prestataire touche couvre au moins la TVA qu'il devra
            // reverser sur son propre devis.
            assert!(
                r.reversement.cents() >= devis.tva.cents(),
                "montant {cents} : reversement {} < TVA due {}",
                r.reversement.cents(),
                devis.tva.cents()
            );
        }
    }

    #[test]
    fn security_le_seuil_des_quatre_yeux_ne_se_contourne_pas_a_l_appel() {
        // Le statut découle du montant, pas d'un paramètre : sinon il suffirait
        // de ne pas le passer.
        for origine in [OrigineValidation::Demandeur, OrigineValidation::Automatique] {
            let liberation =
                Liberation::prononcer(Uuid::new_v4(), &devis_accepte(80_000), origine, t0())
                    .unwrap();
            assert_eq!(liberation.statut, StatutLiberation::EnAttenteOps);
        }
    }

    #[test]
    fn security_la_liberation_porte_de_quoi_auditer_le_versement() {
        // FR-021 `@security` : « chaque libération génère un audit_log avec
        // montant, take, payout_id ». Les trois se lisent sur la ligne.
        let devis = devis_accepte(18_000);
        let liberation =
            Liberation::prononcer(Uuid::new_v4(), &devis, OrigineValidation::Automatique, t0())
                .unwrap();
        assert_eq!(liberation.devis_id, devis.id);
        assert_eq!(liberation.provider_id, devis.provider_id);
        assert_eq!(liberation.repartition.commission_ttc.cents(), 3_920);
        assert_eq!(liberation.decidee_le, t0());
        assert_eq!(liberation.origine.as_str(), "AUTO_RELEASE_72H");
    }

    #[test]
    fn security_le_vocabulaire_d_audit_est_stable() {
        // Ces codes sortent du service et se retrouvent dans des exports.
        for origine in [OrigineValidation::Demandeur, OrigineValidation::Automatique] {
            assert_eq!(OrigineValidation::parse(origine.as_str()), Some(origine));
        }
        for statut in [StatutLiberation::Autorisee, StatutLiberation::EnAttenteOps] {
            assert_eq!(StatutLiberation::parse(statut.as_str()), Some(statut));
        }
        assert_eq!(OrigineValidation::parse("MANUAL"), None);
    }
}
