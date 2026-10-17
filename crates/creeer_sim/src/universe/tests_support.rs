//! The default system as a universe, for tests.

use super::Universe;
use creeer_data::Data;
use creeer_orbit::generate::{Generated, Params, generate};
use std::sync::Arc;
use std::sync::OnceLock;

/// The physics step of the tests (s).
pub const DT: f64 = 1.0 / 60.0;

/// The repository's data.
pub fn data() -> Arc<Data> {
    Data::base()
}

/// The default system, generated once.
pub fn system() -> &'static Generated {
    static SYSTEM: OnceLock<Generated> = OnceLock::new();
    SYSTEM.get_or_init(|| {
        let params = Params::load(&data(), "systems/default.ron").expect("loads");
        generate(&params).expect("stable")
    })
}

/// A fresh universe of the default system.
pub fn universe() -> Universe {
    Universe::new(system().clone(), data(), DT)
}
