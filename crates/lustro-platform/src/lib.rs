//! OS integration.
//!
//! - [`trash`]: the only destructive operation (move to Recycle Bin / Trash), behind
//!   [`protected`] paths, plus "free up space" for downloaded cloud files.
//! - [`reader`]: reads local file data for duplicate hashing without changing access times.
//! - [`reveal`]: show an item in Explorer / Finder / Files.
//! - [`system_space`]: "Where did my space go?" system items and [`system_space::open_setting`].
//! - [`elevation`]: privilege check and volume list.
//!
//! Errors never include paths, so they are safe to log.

pub mod elevation;
pub mod fileops;
pub mod protected;
pub mod reader;
// The drive-type policy is Windows-only, but stays compiled (and tested) everywhere.
#[cfg_attr(not(windows), allow(dead_code))]
mod recycle;
pub mod reveal;
pub mod system_space;
pub mod trash;

pub use elevation::{is_elevated, list_volumes, VolumeInfo};
pub use fileops::protected_rules;
pub use protected::ProtectedPaths;
pub use reader::PlatformFileReader;
pub use trash::{FreeUpOutcome, PlatformTrash, TrashError};
