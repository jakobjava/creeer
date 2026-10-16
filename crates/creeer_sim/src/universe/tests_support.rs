//! The default system as a universe, for tests.

use super::Universe;
use creeer_orbit::generate::{Generated, Params, generate};
use std::path::PathBuf;
use std::sync::OnceLock;

/// The physics step of the tests (s).
pub const DT: f64 = 1.0 / 60.0;

/// The data directory.
pub fn data() -> PathBuf {
    creeer_terrain::data_path("")
}

/// The default system, generated once.
pub fn system() -> &'static Generated {
    static SYSTEM: OnceLock<Generated> = OnceLock::new();
    SYSTEM.get_or_init(|| {
        let params = Params::load(&data().join("systems/default.ron"), &data()).expect("loads");
        generate(&params).expect("stable")
    })
}

/// A fresh universe of the default system.
pub fn universe() -> Universe {
    Universe::new(system().clone(), data(), DT)
}
