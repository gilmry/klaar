//! Value objects communs à tous les bounded contexts Klaar.
//! Pure Rust, aucune IO. Chaque invariant est vérifié à la construction
//! (docs/bmad-livrables/03-Architecture.md §1.1).

mod distance;
mod email;
mod geo;
mod hash;
mod locale;
mod money;
mod perimetre;
mod vat_rate;

pub use distance::DistanceMeters;
pub use email::{Email, EmailError};
pub use geo::{Geo, GeoError};
pub use hash::HashSha256;
pub use locale::{Locale, LocaleError};
pub use money::Money;
pub use perimetre::{dans_le_perimetre, LAT_MAX, LAT_MIN, LON_MAX, LON_MIN};
pub use vat_rate::{VatRate, VatRateError};
