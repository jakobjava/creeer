//! A pack's manifest, `pack.ron` at its root.
//!
//! ```ron
//! Pack(
//!     name: "steelworks",
//!     version: "0.1.0",
//!     description: "Stainless steels and a planet of iron.",
//!     requires: ["base"],
//!     // Entries of the packs before this one taken out, by table and name.
//!     remove: { "materials": ["wood_spruce"] },
//! )
//! ```

use crate::DataError;
use serde::Deserialize;
use std::collections::BTreeMap;

/// The name of the base pack (`data/`).
pub const BASE: &str = "base";

/// What a pack says about itself.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename = "Pack")]
pub struct Manifest {
    /// Unique name: lower-case letters, digits, `_` and `-`.
    pub name: String,
    /// Its version, for the player (not compared).
    pub version: String,
    /// One line for the player.
    #[serde(default)]
    pub description: String,
    /// The packs that must load before it.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Entries of earlier packs it takes out: table name → entry names.
    #[serde(default)]
    pub remove: BTreeMap<String, Vec<String>>,
}

impl Manifest {
    /// Reads a manifest; `origin` names it in errors.
    ///
    /// # Errors
    /// If it does not parse or its name is not a valid pack name.
    pub fn parse(text: &str, origin: &str) -> Result<Self, DataError> {
        let manifest: Self =
            ron::from_str(text).map_err(|e| DataError::new(origin, format!("pack.ron: {e}")))?;
        let valid = !manifest.name.is_empty()
            && manifest
                .name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-');
        if !valid {
            return Err(DataError::new(
                origin,
                format!(
                    "pack.ron: name {:?} must be lower-case letters, digits, _ and -",
                    manifest.name
                ),
            ));
        }
        if manifest.requires.contains(&manifest.name) {
            return Err(DataError::new(
                origin,
                "pack.ron: a pack cannot require itself",
            ));
        }
        Ok(manifest)
    }
}
