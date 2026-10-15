//! egui panels and engineering overlays.

pub mod legend;
pub mod perf;

pub use egui;
pub use legend::heatmap_legend;
pub use perf::{PerfInfo, perf_window};
