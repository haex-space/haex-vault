//! Files of this device (spec 044, research R3): resolving paths, haex-vault's own places and the known
//! places. Extensions (`extensions::fs`), the file browser and agents share these rules.

pub mod drives;
pub mod edit;
pub mod ops;
pub mod places;
pub mod resolve;
pub mod text;
#[cfg(not(target_os = "ios"))]
pub mod watch;

pub use places::{known_places, OwnPlaces, Place};
pub use resolve::{resolve, resolve_entry};
