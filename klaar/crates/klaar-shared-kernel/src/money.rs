use serde::{Deserialize, Serialize};
use std::ops::Add;

/// Montant monétaire en centimes d'euro. Jamais de flottant (Architecture §1.1)
/// pour éviter les erreurs d'arrondi sur les calculs de take-rate/TVA.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Money {
    cents: i64,
}

impl Money {
    pub fn from_cents(cents: i64) -> Self {
        Self { cents }
    }

    pub fn from_euros(euros: i64) -> Self {
        Self { cents: euros * 100 }
    }

    pub fn cents(&self) -> i64 {
        self.cents
    }

    pub fn is_negative(&self) -> bool {
        self.cents < 0
    }
}

impl Add for Money {
    type Output = Money;

    fn add(self, rhs: Money) -> Money {
        Money::from_cents(self.cents + rhs.cents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_adds_two_amounts_without_float_drift() {
        let total = Money::from_cents(1050) + Money::from_cents(250);
        assert_eq!(total.cents(), 1300);
    }

    #[test]
    fn negative_flags_negative_amount_as_negative() {
        assert!(Money::from_cents(-1).is_negative());
    }

    #[test]
    fn edge_zero_is_not_negative() {
        assert!(!Money::from_cents(0).is_negative());
    }

    #[test]
    fn security_from_euros_does_not_overflow_typical_mission_amounts() {
        let mission = Money::from_euros(65_000);
        assert_eq!(mission.cents(), 6_500_000);
    }
}
