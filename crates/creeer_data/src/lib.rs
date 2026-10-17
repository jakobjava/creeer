//! Data packs (F7.1, ADR-0018): every content file of the game is read through [`Data`],
//! the base pack (`data/`) and the mods merged in load order.
//!
//! - A **file** (a planet, a geology, a solar system) comes from the last pack that has it.
//! - A **table** of named entries (materials, species, minerals, rocks) merges by name
//!   across packs ([`merge`]): a later pack's entry replaces an earlier one of the same name,
//!   new names add, and a manifest can take names out.
//! - The merged content has one [`Data::hash`], so a server and its clients can tell whether
//!   they have the same data.
//! - [`locate`] finds the packs at run time; [`watch`] notices when they change (hot-reload).

pub mod data;
pub mod locate;
pub mod manifest;
pub mod merge;
pub mod pack;
#[cfg(test)]
mod tests;
pub mod watch;

pub use data::Data;
pub use locate::Location;
pub use manifest::Manifest;
pub use merge::Table;
pub use pack::Pack;
pub use watch::Watcher;

use std::fmt;

/// A problem with the data: where (a pack, a file, an entry) and what.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataError {
    pub origin: String,
    pub message: String,
}

impl DataError {
    pub fn new(origin: &str, message: impl Into<String>) -> Self {
        Self {
            origin: origin.to_owned(),
            message: message.into(),
        }
    }
}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.origin, self.message)
    }
}

impl std::error::Error for DataError {}
