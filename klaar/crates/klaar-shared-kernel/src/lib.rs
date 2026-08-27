//! Value objects communs à tous les bounded contexts Klaar.
//! Pure Rust, aucune IO. Chaque invariant est vérifié à la construction
//! (docs/bmad-livrables/03-Architecture.md §1.1).

mod distance;
mod email;
mod geo;
mod hash;
mod locale;
mod money;
mod vat_rate;

pub use distance::DistanceMeters;
pub use email::{Email, EmailError};
pub use geo::{Geo, GeoError};
pub use hash::HashSha256;
pub use locale::{Locale, LocaleError};
pub use money::Money;
pub use vat_rate::{VatRate, VatRateError};
