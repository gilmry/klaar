//! Agrégat `Devis` (FR-016, Story 4.1).
//!
//! **Le prix vient du prestataire, jamais d'ici.** C'est l'invariant §10.2 et
//! la mitigation de la loi belge du 26 avril 2024 sur le travail de plateforme :
//! une plateforme qui fixe les tarifs de ceux qui travaillent pour elle exerce
//! une autorité, et cette autorité requalifie. Ce module ne contient donc
//! **aucune** fonction qui propose, suggère, corrige ou pondère un montant. Il
//! en contient une qui refuse l'absurde — zéro, négatif, hors de toute échelle
//! de dépannage — et c'est tout. La différence entre « borner » et « fixer »
//! est ce que `security_le_montant_rendu_est_exactement_celui_propose` vérifie.
//!
//! **La TVA est calculée une fois et conservée.** Le taux belge peut changer,
//! et un devis relu dans deux ans doit montrer ce qui a été présenté ce jour-là,
//! pas ce que le calcul rendrait aujourd'hui. Recalculer à la lecture
//! réécrirait un document contractuel.
//!
//! **Un devis a une durée de vie.** Sans elle, un prestataire resterait engagé
//! sur un prix qu'il a donné il y a trois semaines, et une Mission resterait
//! suspendue à une réponse qui ne viendra pas.

use chrono::{DateTime, Duration, Utc};
use klaar_shared_kernel::{Money, VatRate};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Montant minimal, en centimes HTVA.
///
/// Un centime. Le plancher n'est pas un tarif minimum — ce serait fixer un
/// prix — mais le refus d'un montant nul, que FR-016 `@negative` nomme
/// `AMOUNT_ZERO` et qui signale une saisie ratée plutôt qu'un geste commercial.
pub const MONTANT_MIN_CENTS: i64 = 1;

/// Plafond, en centimes HTVA. Dix mille euros.
///
/// **Arbitrage.** FR-016 `@negative` demande `AMOUNT_TOO_HIGH` à 100 000 € sans
/// dire où passe la borne. Dix mille euros est retenu parce que Klaar est un
/// service de **dépannage** : au-delà, ce n'est plus une intervention d'une
/// heure mais un chantier, qui se contractualise ailleurs, avec des acomptes et
/// un cahier des charges que rien ici ne porte. La borne protège aussi le
/// demandeur d'une faute de frappe à deux zéros près, qui serait autrement
/// pré-autorisée sur sa carte.
pub const MONTANT_MAX_CENTS: i64 = 1_000_000;

/// Délai d'intervention annoncé, en minutes.
pub const DELAI_MIN_MINUTES: i64 = 1;

/// Vingt-quatre heures (FR-016 `@negative`, `DELAY_TOO_LONG`).
///
/// Un dépannage annoncé à plus d'un jour n'est plus un dépannage : le demandeur
/// a besoin de le savoir tout de suite pour appeler ailleurs, et non de
/// l'apprendre en lisant le devis.
pub const DELAI_MAX_MINUTES: i64 = 24 * 60;

/// Durée de validité d'un devis (FR-016 `@happy` : « expire dans 1 h »).
pub const VALIDITE_MINUTES: i64 = 60;

/// Devis successifs pour une même Mission (FR-016 `@edge`).
///
/// Trois. Au quatrième, la Mission est annulée et le demandeur reprend la main.
/// Sans cette borne, un prestataire attribué pourrait maintenir quelqu'un en
/// attente indéfiniment en renvoyant un devis à chaque refus.
pub const DEVIS_MAX_PAR_MISSION: usize = 3;

/// Note libre jointe au devis.
///
/// Bornée parce qu'elle est affichée telle quelle au demandeur et conservée :
/// un champ non borné est une porte ouverte à l'écriture de masse dans une
/// table que personne ne purge.
pub const NOTE_MAX_CARACTERES: usize = 500;

/// Référence de la preuve justifiant un taux réduit (FR-016 `@edge`).
pub const PREUVE_MAX_CARACTERES: usize = 200;

/// Taux de TVA qu'un devis peut porter.
///
/// Trois, et pas un de plus : les taux belges applicables à une intervention à
/// domicile (Architecture §6.5). Accepter un taux arbitraire laisserait émettre
/// un devis à 3 % de TVA, ce qui n'est pas une erreur d'affichage mais une
/// fraude fiscale documentée par nos soins.
pub const TAUX_ADMIS: [VatRate; 3] = [
    VatRate::BELGIUM_STANDARD,
    VatRate::BELGIUM_THERMAL_INSULATION,
    VatRate::BELGIUM_RENOVATION,
];

/// Pourquoi un devis est refusé (FR-017 `@happy`).
///
/// **Vocabulaire fermé**, comme les motifs d'annulation d'une Demande. Un champ
/// libre serait une invitation à écrire ce qu'on pense du prestataire, dans une
/// donnée qu'il pourrait lire un jour ; et il ne se compterait pas. Ces codes,
/// eux, se comptent — c'est ce qui permettra de savoir si les refus viennent du
/// prix ou du délai.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotifRefus {
    TropCher,
    DelaiTropLong,
    PlusBesoin,
    Autre,
}

impl MotifRefus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TropCher => "TOO_EXPENSIVE",
            Self::DelaiTropLong => "DELAY_TOO_LONG",
            Self::PlusBesoin => "NO_LONGER_NEEDED",
            Self::Autre => "OTHER",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "TOO_EXPENSIVE" => Some(Self::TropCher),
            "DELAY_TOO_LONG" => Some(Self::DelaiTropLong),
            "NO_LONGER_NEEDED" => Some(Self::PlusBesoin),
            "OTHER" => Some(Self::Autre),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatutDevis {
    /// Émis, en attente de réponse du demandeur.
    Envoye,
    /// Accepté par le demandeur (FR-017, pas encore livré).
    Accepte,
    /// Refusé par le demandeur.
    Refuse,
    /// Aucune réponse dans l'heure.
    Expire,
}

impl StatutDevis {
    /// Vocabulaire de FR-016, repris tel quel.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Envoye => "SENT",
            Self::Accepte => "ACCEPTED",
            Self::Refuse => "REFUSED",
            Self::Expire => "EXPIRED",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "SENT" => Some(Self::Envoye),
            "ACCEPTED" => Some(Self::Accepte),
            "REFUSED" => Some(Self::Refuse),
            "EXPIRED" => Some(Self::Expire),
            _ => None,
        }
    }

    /// Vrai si ce devis attend encore une réponse.
    ///
    /// C'est cette notion, et non le statut lui-même, qu'interroge la règle
    /// « un seul devis en cours par Mission ». Elle est ici pour qu'un statut
    /// ajouté un jour passe par ce `match` exhaustif plutôt que par une liste
    /// recopiée dans une migration.
    pub fn est_en_cours(&self) -> bool {
        match self {
            Self::Envoye => true,
            Self::Accepte | Self::Refuse | Self::Expire => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DevisError {
    MontantNul,
    MontantNegatif,
    MontantTropEleve,
    DelaiInvalide,
    DelaiTropLong,
    NoteTropLongue,
    /// Taux hors des trois taux belges applicables.
    TauxTvaInconnu,
    /// Taux réduit demandé sans la preuve qui le justifie (FR-016 `@edge`).
    PreuveTvaRequise,
    PreuveTropLongue,
    /// Réponse à un devis dont l'heure de validité est passée (FR-017 `@edge`).
    DevisExpire,
    /// Réponse à un devis qui en a déjà reçu une.
    DevisDejaRepondu,
}

impl DevisError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::MontantNul => "AMOUNT_ZERO",
            Self::MontantNegatif => "AMOUNT_NEGATIVE",
            Self::MontantTropEleve => "AMOUNT_TOO_HIGH",
            Self::DelaiInvalide => "DELAY_INVALID",
            Self::DelaiTropLong => "DELAY_TOO_LONG",
            Self::NoteTropLongue => "NOTE_TOO_LONG",
            Self::TauxTvaInconnu => "VAT_RATE_UNKNOWN",
            Self::PreuveTvaRequise => "VAT_PROOF_REQUIRED",
            Self::PreuveTropLongue => "VAT_PROOF_TOO_LONG",
            Self::DevisExpire => "QUOTE_EXPIRED",
            Self::DevisDejaRepondu => "QUOTE_ALREADY_ANSWERED",
        }
    }
}

impl fmt::Display for DevisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MontantNul => write!(f, "un devis à zéro euro n'en est pas un"),
            Self::MontantNegatif => write!(f, "montant négatif"),
            Self::MontantTropEleve => {
                write!(f, "montant au-delà de {} € HTVA", MONTANT_MAX_CENTS / 100)
            }
            Self::DelaiInvalide => write!(f, "délai d'intervention invalide"),
            Self::DelaiTropLong => write!(f, "délai au-delà de {} h", DELAI_MAX_MINUTES / 60),
            Self::NoteTropLongue => write!(f, "note au-delà de {NOTE_MAX_CARACTERES} caractères"),
            Self::TauxTvaInconnu => write!(f, "taux de TVA non applicable en Belgique"),
            Self::PreuveTvaRequise => write!(f, "un taux réduit demande une preuve"),
            Self::PreuveTropLongue => write!(
                f,
                "référence de preuve au-delà de {PREUVE_MAX_CARACTERES} caractères"
            ),
            Self::DevisExpire => write!(f, "ce devis a expiré"),
            Self::DevisDejaRepondu => write!(f, "ce devis a déjà reçu une réponse"),
        }
    }
}

impl std::error::Error for DevisError {}

/// Ce que le prestataire propose.
///
/// Groupé plutôt qu'égrené en sept paramètres : ils viennent tous du même
/// formulaire et voyagent ensemble, et une liste de sept arguments dont trois
/// entiers finit un jour par se remplir dans le mauvais ordre — ici, cela
/// échangerait un montant et un délai sans que rien ne s'en aperçoive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposition {
    pub montant_htva_cents: i64,
    /// Taux en points de base : 2100, 1200 ou 600.
    pub taux_tva_bp: u16,
    pub delai_minutes: i64,
    pub note: Option<String>,
    /// Référence de la preuve, obligatoire dès que le taux n'est pas le taux
    /// normal.
    pub preuve_tva_reduite: Option<String>,
}

/// Un devis, tel qu'il a été présenté au demandeur.
// `Eq` tient : tous les montants sont des entiers de centimes, aucun flottant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Devis {
    pub id: Uuid,
    pub mission_id: Uuid,
    /// Le prestataire qui l'a émis. Conservé sur le devis lui-même, et pas
    /// seulement déductible de la Mission : FR-016 `@security` demande un
    /// journal « timestamp + Provider ID + montant » qui se relise sans
    /// jointure, y compris après réattribution de la Mission.
    pub provider_id: Uuid,
    pub montant_htva: Money,
    pub taux_tva: VatRate,
    /// Calculée à l'émission, jamais recalculée à la lecture.
    pub tva: Money,
    pub total_ttc: Money,
    pub delai_minutes: i64,
    pub note: Option<String>,
    pub preuve_tva_reduite: Option<String>,
    pub statut: StatutDevis,
    /// Renseigné seulement quand le demandeur a refusé, et qu'il a dit pourquoi.
    pub motif_refus: Option<MotifRefus>,
    pub cree_le: DateTime<Utc>,
    pub expire_le: DateTime<Utc>,
}

fn normaliser(
    texte: Option<String>,
    maximum: usize,
    trop_long: DevisError,
) -> Result<Option<String>, DevisError> {
    match texte {
        None => Ok(None),
        Some(brut) => {
            let coupe = brut.trim();
            if coupe.is_empty() {
                // Une note vide et une note absente sont la même chose ; les
                // distinguer ferait afficher un cadre de note vide au demandeur.
                return Ok(None);
            }
            if coupe.chars().count() > maximum {
                return Err(trop_long);
            }
            Ok(Some(coupe.to_string()))
        }
    }
}

impl Devis {
    /// Émet un devis, ou refuse la proposition.
    ///
    /// L'ordre des contrôles suit celui de la lecture d'un devis : le montant
    /// d'abord, qui est ce dont on discute ; le taux ensuite, qui en dépend ;
    /// le délai ; les textes en dernier, qui n'engagent rien.
    ///
    /// Aucun paramètre ne permet de le créer dans un autre statut : un devis
    /// naît envoyé, comme une Mission naît acceptée.
    pub fn emettre(
        mission_id: Uuid,
        provider_id: Uuid,
        proposition: Proposition,
        maintenant: DateTime<Utc>,
    ) -> Result<Self, DevisError> {
        // Zéro et négatif sont deux codes distincts parce que FR-016 les
        // distingue, et il a raison : un zéro est un champ laissé vide, un
        // négatif est une tentative.
        if proposition.montant_htva_cents == 0 {
            return Err(DevisError::MontantNul);
        }
        if proposition.montant_htva_cents < 0 {
            return Err(DevisError::MontantNegatif);
        }
        if proposition.montant_htva_cents > MONTANT_MAX_CENTS {
            return Err(DevisError::MontantTropEleve);
        }
        debug_assert!(proposition.montant_htva_cents >= MONTANT_MIN_CENTS);

        let taux = VatRate::from_basis_points(proposition.taux_tva_bp)
            .map_err(|_| DevisError::TauxTvaInconnu)?;
        if !TAUX_ADMIS.contains(&taux) {
            return Err(DevisError::TauxTvaInconnu);
        }

        let preuve = normaliser(
            proposition.preuve_tva_reduite,
            PREUVE_MAX_CARACTERES,
            DevisError::PreuveTropLongue,
        )?;
        // Tout taux inférieur au taux normal doit se justifier. La règle porte
        // sur « réduit », pas sur « 6 % » : le taux à 12 % de l'isolation
        // thermique se justifie exactement pour les mêmes raisons, et écrire
        // `== 600` aurait laissé passer l'autre.
        if taux != VatRate::BELGIUM_STANDARD && preuve.is_none() {
            return Err(DevisError::PreuveTvaRequise);
        }

        if proposition.delai_minutes < DELAI_MIN_MINUTES {
            return Err(DevisError::DelaiInvalide);
        }
        if proposition.delai_minutes > DELAI_MAX_MINUTES {
            return Err(DevisError::DelaiTropLong);
        }

        let note = normaliser(
            proposition.note,
            NOTE_MAX_CARACTERES,
            DevisError::NoteTropLongue,
        )?;

        // Troncature et non arrondi : `VatRate::apply` tronque, donc le
        // demandeur ne paie jamais le centime de TVA qui n'est pas dû. L'écart
        // maximal est inférieur au centime et va dans le sens du payeur.
        let tva_cents = taux.apply(proposition.montant_htva_cents);

        Ok(Self {
            id: Uuid::new_v4(),
            mission_id,
            provider_id,
            montant_htva: Money::from_cents(proposition.montant_htva_cents),
            taux_tva: taux,
            tva: Money::from_cents(tva_cents),
            total_ttc: Money::from_cents(proposition.montant_htva_cents + tva_cents),
            delai_minutes: proposition.delai_minutes,
            note,
            preuve_tva_reduite: preuve,
            statut: StatutDevis::Envoye,
            motif_refus: None,
            cree_le: maintenant,
            expire_le: maintenant + Duration::minutes(VALIDITE_MINUTES),
        })
    }

    /// Vrai si l'heure de validité est passée.
    ///
    /// Distinct du statut : un devis peut être matériellement expiré sans que
    /// le balayage soit encore passé, et la vue rendue au demandeur doit dire
    /// la vérité entre les deux.
    pub fn est_expire(&self, maintenant: DateTime<Utc>) -> bool {
        self.statut.est_en_cours() && maintenant >= self.expire_le
    }

    /// Passe le devis en expiré. Rend `false` s'il n'y avait rien à faire.
    pub fn expirer(&mut self, maintenant: DateTime<Utc>) -> bool {
        if !self.est_expire(maintenant) {
            return false;
        }
        self.statut = StatutDevis::Expire;
        true
    }

    pub fn appartient_a(&self, provider_id: Uuid) -> bool {
        self.provider_id == provider_id
    }

    /// Accepte le devis (FR-017 `@happy`).
    ///
    /// L'expiration est vérifiée **ici et pas seulement au balayage** : un devis
    /// peut être matériellement échu sans que le balayage soit passé, et
    /// l'accepter engagerait le prestataire sur un prix qu'il ne tient plus.
    /// FR-017 `@edge` le nomme, et rend 410.
    pub fn accepter(&mut self, maintenant: DateTime<Utc>) -> Result<(), DevisError> {
        self.verifier_repondable(maintenant)?;
        self.statut = StatutDevis::Accepte;
        Ok(())
    }

    /// Refuse le devis, avec ou sans motif (FR-017 `@happy`).
    ///
    /// Le motif est facultatif : l'exiger obligerait à choisir une raison pour
    /// dire non, ce qui n'est pas dû. Le prestataire pourra en renvoyer un
    /// autre, dans la limite du plafond.
    pub fn refuser(
        &mut self,
        motif: Option<MotifRefus>,
        maintenant: DateTime<Utc>,
    ) -> Result<(), DevisError> {
        self.verifier_repondable(maintenant)?;
        self.statut = StatutDevis::Refuse;
        self.motif_refus = motif;
        Ok(())
    }

    /// Les deux mêmes gardes, dans le même ordre, pour les deux réponses.
    ///
    /// L'ordre n'est pas arbitraire mais il ne départage rien : `est_expire` ne
    /// parle que des devis qui attendent encore une réponse. Un devis déjà
    /// accepté ou refusé dit donc « déjà répondu » quelle que soit l'heure, ce
    /// qui est l'information utile — la réponse existe, et c'est elle qui
    /// compte. Seul un devis resté en attente peut répondre « expiré ».
    fn verifier_repondable(&self, maintenant: DateTime<Utc>) -> Result<(), DevisError> {
        if self.est_expire(maintenant) {
            return Err(DevisError::DevisExpire);
        }
        if !self.statut.est_en_cours() {
            return Err(DevisError::DevisDejaRepondu);
        }
        Ok(())
    }

    /// Secondes restantes avant expiration, jamais négatives.
    pub fn secondes_restantes(&self, maintenant: DateTime<Utc>) -> i64 {
        (self.expire_le - maintenant).num_seconds().max(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proposition(montant_cents: i64) -> Proposition {
        Proposition {
            montant_htva_cents: montant_cents,
            taux_tva_bp: 2100,
            delai_minutes: 45,
            note: None,
            preuve_tva_reduite: None,
        }
    }

    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-03-01T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn emettre(p: Proposition) -> Result<Devis, DevisError> {
        Devis::emettre(Uuid::new_v4(), Uuid::new_v4(), p, t0())
    }

    // === @happy ===

    #[test]
    fn happy_le_devis_nominal_de_fr_016() {
        // 180 € HTVA à 21 % → 217,80 € TTC, l'exemple du PRD au centime près.
        let devis = emettre(Proposition {
            note: Some("remplacement joint".to_string()),
            ..proposition(18_000)
        })
        .unwrap();

        assert_eq!(devis.montant_htva.cents(), 18_000);
        assert_eq!(devis.tva.cents(), 3_780);
        assert_eq!(devis.total_ttc.cents(), 21_780);
        assert_eq!(devis.delai_minutes, 45);
        assert_eq!(devis.note.as_deref(), Some("remplacement joint"));
        assert_eq!(devis.statut, StatutDevis::Envoye);
    }

    #[test]
    fn happy_le_devis_expire_dans_une_heure() {
        let devis = emettre(proposition(18_000)).unwrap();
        assert_eq!((devis.expire_le - devis.cree_le).num_minutes(), 60);
        assert_eq!(devis.secondes_restantes(t0()), 3_600);
    }

    #[test]
    fn happy_un_taux_reduit_avec_sa_preuve_est_accepte() {
        let devis = emettre(Proposition {
            taux_tva_bp: 600,
            preuve_tva_reduite: Some("logement de 1974, attestation jointe".to_string()),
            ..proposition(18_000)
        })
        .unwrap();

        assert_eq!(devis.taux_tva, VatRate::BELGIUM_RENOVATION);
        assert_eq!(devis.tva.cents(), 1_080);
        assert_eq!(devis.total_ttc.cents(), 19_080);
        assert!(devis.preuve_tva_reduite.is_some());
    }

    // === @negative ===

    #[test]
    fn negative_un_montant_nul_est_refuse() {
        assert_eq!(emettre(proposition(0)), Err(DevisError::MontantNul));
        assert_eq!(DevisError::MontantNul.code(), "AMOUNT_ZERO");
    }

    #[test]
    fn negative_un_montant_negatif_est_refuse() {
        assert_eq!(
            emettre(proposition(-1_000)),
            Err(DevisError::MontantNegatif)
        );
        assert_eq!(DevisError::MontantNegatif.code(), "AMOUNT_NEGATIVE");
    }

    #[test]
    fn negative_un_montant_hors_echelle_est_refuse() {
        // L'exemple de FR-016 : 100 000 €.
        assert_eq!(
            emettre(proposition(10_000_000)),
            Err(DevisError::MontantTropEleve)
        );
    }

    #[test]
    fn negative_un_delai_au_dela_de_vingt_quatre_heures_est_refuse() {
        let refus = emettre(Proposition {
            delai_minutes: 25 * 60,
            ..proposition(18_000)
        });
        assert_eq!(refus, Err(DevisError::DelaiTropLong));
        assert_eq!(DevisError::DelaiTropLong.code(), "DELAY_TOO_LONG");
    }

    #[test]
    fn negative_un_delai_nul_ou_negatif_est_refuse() {
        for minutes in [0, -30] {
            assert_eq!(
                emettre(Proposition {
                    delai_minutes: minutes,
                    ..proposition(18_000)
                }),
                Err(DevisError::DelaiInvalide),
                "délai {minutes}"
            );
        }
    }

    #[test]
    fn negative_un_taux_reduit_sans_preuve_est_refuse() {
        // Sans cette règle, tout devis passerait à 6 % : c'est le prestataire
        // qui y gagne, et c'est nous qui aurions documenté la fraude.
        assert_eq!(
            emettre(Proposition {
                taux_tva_bp: 600,
                ..proposition(18_000)
            }),
            Err(DevisError::PreuveTvaRequise)
        );
    }

    // === @edge ===

    #[test]
    fn edge_le_plafond_lui_meme_passe() {
        let devis = emettre(proposition(MONTANT_MAX_CENTS)).unwrap();
        assert_eq!(devis.montant_htva.cents(), MONTANT_MAX_CENTS);
        assert_eq!(
            emettre(proposition(MONTANT_MAX_CENTS + 1)),
            Err(DevisError::MontantTropEleve)
        );
    }

    #[test]
    fn edge_le_delai_de_vingt_quatre_heures_pile_passe() {
        assert!(emettre(Proposition {
            delai_minutes: DELAI_MAX_MINUTES,
            ..proposition(18_000)
        })
        .is_ok());
    }

    #[test]
    fn edge_un_centime_passe() {
        // Le plancher n'est pas un tarif minimum : un dépannage à un centime
        // est un choix commercial, pas une erreur de saisie.
        let devis = emettre(proposition(MONTANT_MIN_CENTS)).unwrap();
        assert_eq!(devis.montant_htva.cents(), 1);
        // 21 % d'un centime, tronqué, font zéro. Le total reste cohérent.
        assert_eq!(devis.tva.cents(), 0);
        assert_eq!(devis.total_ttc.cents(), 1);
    }

    #[test]
    fn edge_une_note_vide_vaut_une_note_absente() {
        let devis = emettre(Proposition {
            note: Some("   ".to_string()),
            ..proposition(18_000)
        })
        .unwrap();
        assert_eq!(devis.note, None);
    }

    #[test]
    fn edge_le_devis_expire_a_la_seconde_pres() {
        let mut devis = emettre(proposition(18_000)).unwrap();
        let avant = devis.expire_le - Duration::seconds(1);

        assert!(!devis.est_expire(avant));
        assert!(!devis.clone().expirer(avant));

        assert!(devis.est_expire(devis.expire_le));
        assert!(devis.expirer(devis.expire_le));
        assert_eq!(devis.statut, StatutDevis::Expire);
        // Deux passages du balayage ne doivent pas produire deux expirations.
        assert!(!devis.expirer(devis.expire_le));
    }

    #[test]
    fn edge_un_devis_deja_repondu_n_expire_plus() {
        let mut devis = emettre(proposition(18_000)).unwrap();
        devis.statut = StatutDevis::Accepte;
        assert!(!devis.est_expire(devis.expire_le + Duration::hours(3)));
        assert!(!devis.expirer(devis.expire_le + Duration::hours(3)));
        assert_eq!(devis.statut, StatutDevis::Accepte);
    }

    #[test]
    fn edge_les_secondes_restantes_ne_passent_pas_sous_zero() {
        let devis = emettre(proposition(18_000)).unwrap();
        assert_eq!(
            devis.secondes_restantes(devis.expire_le + Duration::hours(2)),
            0
        );
    }

    // === @security ===

    // === Réponse du demandeur (FR-017) ===

    #[test]
    fn happy_le_demandeur_accepte_le_devis() {
        let mut devis = emettre(proposition(18_000)).unwrap();
        assert!(devis.accepter(t0()).is_ok());
        assert_eq!(devis.statut, StatutDevis::Accepte);
        assert_eq!(devis.motif_refus, None);
    }

    #[test]
    fn happy_le_demandeur_refuse_avec_un_motif() {
        let mut devis = emettre(proposition(18_000)).unwrap();
        assert!(devis.refuser(Some(MotifRefus::TropCher), t0()).is_ok());
        assert_eq!(devis.statut, StatutDevis::Refuse);
        assert_eq!(devis.motif_refus, Some(MotifRefus::TropCher));
    }

    #[test]
    fn happy_le_refus_sans_motif_est_permis() {
        // Exiger une raison obligerait à en choisir une pour dire non, ce qui
        // n'est pas dû.
        let mut devis = emettre(proposition(18_000)).unwrap();
        assert!(devis.refuser(None, t0()).is_ok());
        assert_eq!(devis.statut, StatutDevis::Refuse);
        assert_eq!(devis.motif_refus, None);
    }

    #[test]
    fn negative_un_devis_expire_ne_s_accepte_plus() {
        // FR-017 `@edge` : accepter à T+10 s un devis qui expirait dans 5 s.
        // Sans cette garde, le prestataire serait engagé sur un prix qu'il ne
        // tient plus.
        let mut devis = emettre(proposition(18_000)).unwrap();
        let apres = devis.expire_le + Duration::seconds(1);
        assert_eq!(devis.accepter(apres), Err(DevisError::DevisExpire));
        assert_eq!(devis.refuser(None, apres), Err(DevisError::DevisExpire));
        assert_eq!(
            devis.statut,
            StatutDevis::Envoye,
            "rien ne doit avoir bougé"
        );
    }

    #[test]
    fn negative_un_devis_deja_repondu_ne_se_reprend_pas() {
        let mut devis = emettre(proposition(18_000)).unwrap();
        devis.accepter(t0()).unwrap();
        assert_eq!(devis.accepter(t0()), Err(DevisError::DevisDejaRepondu));
        assert_eq!(
            devis.refuser(Some(MotifRefus::TropCher), t0()),
            Err(DevisError::DevisDejaRepondu)
        );
        assert_eq!(devis.statut, StatutDevis::Accepte);
    }

    #[test]
    fn edge_l_instant_exact_d_expiration_refuse_deja() {
        // Même borne qu'au balayage : l'égalité vaut expiration, sinon deux
        // parties du service ne diraient pas la même chose à la seconde près.
        let mut devis = emettre(proposition(18_000)).unwrap();
        let expire_le = devis.expire_le;
        assert_eq!(devis.accepter(expire_le), Err(DevisError::DevisExpire));
        assert!(devis.accepter(expire_le - Duration::seconds(1)).is_ok());
    }

    #[test]
    fn edge_un_devis_repondu_apres_coup_dit_deja_repondu_et_non_expire() {
        // `est_expire` ne parle que des devis en attente : une fois la réponse
        // donnée, c'est elle l'information utile, quelle que soit l'heure.
        let mut devis = emettre(proposition(18_000)).unwrap();
        devis.refuser(None, t0()).unwrap();
        assert_eq!(
            devis.accepter(devis.expire_le + Duration::hours(5)),
            Err(DevisError::DevisDejaRepondu)
        );
    }

    #[test]
    fn security_le_motif_de_refus_est_un_vocabulaire_ferme() {
        // Un champ libre serait une invitation à écrire ce qu'on pense du
        // prestataire, dans une donnée qu'il pourrait lire un jour.
        for motif in [
            MotifRefus::TropCher,
            MotifRefus::DelaiTropLong,
            MotifRefus::PlusBesoin,
            MotifRefus::Autre,
        ] {
            assert_eq!(MotifRefus::parse(motif.as_str()), Some(motif));
        }
        assert_eq!(MotifRefus::parse("ce plombier est un voleur"), None);
        assert_eq!(MotifRefus::parse(""), None);
    }

    #[test]
    fn security_accepter_ne_touche_a_aucun_montant() {
        // L'accord porte sur ce qui a été présenté, au centime près. Un
        // recalcul à l'acceptation changerait le contrat après signature.
        let mut devis = emettre(proposition(18_000)).unwrap();
        let avant = (
            devis.montant_htva,
            devis.tva,
            devis.total_ttc,
            devis.delai_minutes,
        );
        devis.accepter(t0()).unwrap();
        assert_eq!(
            (
                devis.montant_htva,
                devis.tva,
                devis.total_ttc,
                devis.delai_minutes
            ),
            avant
        );
    }

    #[test]
    fn security_le_montant_rendu_est_exactement_celui_propose() {
        // **C'est le test de l'invariant §10.2.** Aucun montant admissible n'est
        // modifié, arrondi, plafonné vers un « tarif conseillé » ni corrigé.
        // Le jour où quelqu'un ajoutera une grille tarifaire, ce test tombera.
        for cents in [1, 999, 5_000, 12_345, 18_000, 99_999, MONTANT_MAX_CENTS] {
            let devis = emettre(proposition(cents)).unwrap();
            assert_eq!(devis.montant_htva.cents(), cents, "montant {cents}");
        }
    }

    #[test]
    fn security_deux_prestataires_peuvent_proposer_des_prix_differents() {
        // La liberté tarifaire ne se prouve pas par l'absence de code : elle se
        // prouve en montrant que deux propositions opposées passent toutes deux.
        let mission = Uuid::new_v4();
        let a = Devis::emettre(mission, Uuid::new_v4(), proposition(8_000), t0()).unwrap();
        let b = Devis::emettre(mission, Uuid::new_v4(), proposition(45_000), t0()).unwrap();
        assert_eq!(a.montant_htva.cents(), 8_000);
        assert_eq!(b.montant_htva.cents(), 45_000);
    }

    #[test]
    fn security_le_devis_porte_de_quoi_auditer_la_fixation_du_prix() {
        // FR-016 `@security` : « chaque Devis est journalisé avec timestamp +
        // Provider ID + montant ». Les trois sont sur l'agrégat, donc sur la
        // ligne écrite, et non reconstruits par jointure.
        let provider = Uuid::new_v4();
        let devis = Devis::emettre(Uuid::new_v4(), provider, proposition(18_000), t0()).unwrap();
        assert_eq!(devis.provider_id, provider);
        assert_eq!(devis.cree_le, t0());
        assert_eq!(devis.montant_htva.cents(), 18_000);
        assert!(devis.appartient_a(provider));
        assert!(!devis.appartient_a(Uuid::new_v4()));
    }

    #[test]
    fn security_un_taux_hors_bareme_belge_est_refuse() {
        // 3 % n'existe pas en Belgique. L'accepter ferait de la plateforme le
        // lieu d'émission de factures fiscalement fausses.
        for bp in [0, 300, 500, 1_000, 2_000, 2_500] {
            assert_eq!(
                emettre(Proposition {
                    taux_tva_bp: bp,
                    preuve_tva_reduite: Some("preuve".to_string()),
                    ..proposition(18_000)
                }),
                Err(DevisError::TauxTvaInconnu),
                "taux {bp}"
            );
        }
    }

    #[test]
    fn security_les_textes_libres_sont_bornes() {
        assert_eq!(
            emettre(Proposition {
                note: Some("x".repeat(NOTE_MAX_CARACTERES + 1)),
                ..proposition(18_000)
            }),
            Err(DevisError::NoteTropLongue)
        );
        assert_eq!(
            emettre(Proposition {
                taux_tva_bp: 600,
                preuve_tva_reduite: Some("x".repeat(PREUVE_MAX_CARACTERES + 1)),
                ..proposition(18_000)
            }),
            Err(DevisError::PreuveTropLongue)
        );
    }

    #[test]
    fn security_un_devis_nait_toujours_envoye() {
        // Rien dans la signature ne permet d'en fabriquer un déjà accepté, ce
        // qui court-circuiterait l'accord du demandeur (FR-017).
        let devis = emettre(proposition(18_000)).unwrap();
        assert_eq!(devis.statut, StatutDevis::Envoye);
        assert!(devis.statut.est_en_cours());
    }

    #[test]
    fn security_le_statut_en_cours_est_exhaustif() {
        assert!(StatutDevis::Envoye.est_en_cours());
        for termine in [
            StatutDevis::Accepte,
            StatutDevis::Refuse,
            StatutDevis::Expire,
        ] {
            assert!(!termine.est_en_cours(), "{}", termine.as_str());
            assert_eq!(StatutDevis::parse(termine.as_str()), Some(termine));
        }
        assert_eq!(StatutDevis::parse("PENDING"), None);
    }
}
