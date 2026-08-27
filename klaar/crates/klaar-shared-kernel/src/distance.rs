use serde::{Deserialize, Serialize};

/// Distance en mètres. Utilisé pour le rayon de matching (FR-012, 5 km MVP).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DistanceMeters(u32);

impl DistanceMeters {
    pub fn new(meters: u32) -> Self {
        Self(meters)
    }

    pub fn meters(&self) -> u32 {
        self.0
    }

    pub fn from_km(km: u32) -> Self {
        Self(km * 1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_five_kilometers_matches_mvp_matching_radius() {
        assert_eq!(DistanceMeters::from_km(5).meters(), 5_000);
    }

    #[test]
    fn edge_zero_distance_is_valid_same_spot() {
        assert_eq!(DistanceMeters::new(0).meters(), 0);
    }
}
