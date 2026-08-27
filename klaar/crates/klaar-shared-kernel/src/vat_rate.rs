use serde::{Deserialize, Serialize};

/// Taux de TVA en points de base (2100 = 21 %). Belgique : 21 % (normal),
/// 12 % (isolation thermique), 6 % (rénovation logement ≥ 5 ans) — Architecture §6.5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VatRate {
    basis_points: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VatRateError {
    ExceedsHundredPercent,
}

impl VatRate {
    pub const BELGIUM_STANDARD: VatRate = VatRate { basis_points: 2100 };
    pub const BELGIUM_THERMAL_INSULATION: VatRate = VatRate { basis_points: 1200 };
    pub const BELGIUM_RENOVATION: VatRate = VatRate { basis_points: 600 };

    pub fn from_basis_points(basis_points: u16) -> Result<Self, VatRateError> {
        if basis_points > 10_000 {
            return Err(VatRateError::ExceedsHundredPercent);
        }
        Ok(Self { basis_points })
    }

    pub fn basis_points(&self) -> u16 {
        self.basis_points
    }

    pub fn apply(&self, amount_cents: i64) -> i64 {
        (amount_cents as i128 * self.basis_points as i128 / 10_000) as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_belgium_standard_rate_is_21_percent() {
        assert_eq!(VatRate::BELGIUM_STANDARD.basis_points(), 2100);
    }

    #[test]
    fn negative_rejects_rate_above_100_percent() {
        assert_eq!(
            VatRate::from_basis_points(10_001),
            Err(VatRateError::ExceedsHundredPercent)
        );
    }

    #[test]
    fn edge_accepts_exactly_100_percent() {
        assert!(VatRate::from_basis_points(10_000).is_ok());
    }

    #[test]
    fn security_applies_standard_rate_without_rounding_drift() {
        // 100,00 € HTVA à 21 % = 21,00 € de TVA
        assert_eq!(VatRate::BELGIUM_STANDARD.apply(10_000), 2_100);
    }
}
