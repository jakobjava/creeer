//! egui panels and engineering overlays, the game's interface look (`theme.rs`) and the HUD's
//! drawn pieces (`paint.rs`).

pub mod legend;
pub mod paint;
pub mod perf;
pub mod theme;

pub use egui;
pub use egui_plot;
pub use legend::heatmap_legend;
pub use perf::{PerfInfo, perf_window};
