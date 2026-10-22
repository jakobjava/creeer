//! The art style in data (G1.3a, ADR-0026): the palette that `docs/art-style.md` sets out,
//! loaded from the packs, and the checks that keep the style from drifting (§11 there).
//!
//! - [`colour`]: sRGB, CIELAB and the CIEDE2000 difference;
//! - [`palette`]: the tokens models and the renderer name instead of colours, and the
//!   player colours;
//! - [`checks`]: contrast against the grounds, chroma and lightness limits, player colours
//!   told apart; [`exception`]: the user's approved exceptions to them;
//! - [`model`]: models (binary glTF from the scripts in `art/`) read into meshes that name
//!   tokens; [`model_check`]: each held to its manifest entry and the style;
//! - [`wearable`]: the engineer's wearables, what each hides worn, and what a worn outfit draws;
//! - [`vision`]: colour-vision deficiencies simulated (Machado et al. 2009), kept for the
//!   colour-blind mode that waits until the game is published (`docs/future-improvements.md`).

pub mod checks;
pub mod colour;
pub mod exception;
pub mod model;
pub mod model_check;
pub mod model_levels;
pub mod model_skin;
pub mod palette;
pub mod vision;
pub mod wearable;

#[cfg(test)]
mod colour_tests;
#[cfg(test)]
mod doc_tests;
#[cfg(test)]
mod model_skin_tests;
#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod palette_tests;
#[cfg(test)]
mod wearable_tests;

pub use checks::{Finding, check};
pub use colour::{Lab, Srgb, ciede2000};
pub use exception::{Exception, Rule};
pub use palette::{Group, Palette, Pattern, Shade, Token};
pub use vision::Vision;

/// Effects the style forbids (`docs/art-style.md` §5): none may appear in the renderer's
/// sources, which a test checks against this list.
pub const FORBIDDEN_EFFECTS: [&str; 10] = [
    "film grain",
    "chromatic aberration",
    "vignette",
    "lens flare",
    "depth of field",
    "motion blur",
    "outline",
    "cel shading",
    "colour grading",
    "fog",
];
