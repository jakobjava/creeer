//! The art style in data (G1.3a, ADR-0026): the palette that `docs/art-style.md` sets out,
//! loaded from the packs, and the checks that keep the style from drifting (§11 there).
//!
//! - [`colour`]: sRGB, CIELAB and the CIEDE2000 difference;
//! - [`vision`]: colour-vision deficiencies simulated (Machado et al. 2009);
//! - [`palette`]: the tokens models and the renderer name instead of colours, and the
//!   player colours;
//! - [`checks`]: contrast against the grounds, chroma and lightness limits, player colours
//!   told apart.

pub mod checks;
pub mod colour;
pub mod palette;
pub mod vision;

#[cfg(test)]
mod colour_tests;
#[cfg(test)]
mod doc_tests;
#[cfg(test)]
mod palette_tests;

pub use checks::{Finding, check};
pub use colour::{Lab, Srgb, ciede2000};
pub use palette::{Group, Palette, Shade, Token};
pub use vision::Vision;

/// Effects the style forbids (`docs/art-style.md` §5): none may appear in the renderer's
/// post-processing chain, which its own test checks against this list.
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
