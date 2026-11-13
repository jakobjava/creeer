//! Sound from the simulation (ADR-0037). Nothing is recorded: every sound is worked out from what
//! the physics knows of what made it.
//!
//! - **What sounds are made of** (`library.rs`, `data/sound/`): each material's elastic constants
//!   and loss factor, how it cracks and creaks, the grounds, what drives a fastener, the mix.
//! - **How a body rings** (`modes.rs`, `acoustic.rs`): its modes from its shape and material
//!   (van den Doel and Pai 1998), how hard and how briefly it was struck (Hertz), how loud each
//!   mode is a metre away.
//! - **What happened** (`events.rs`, `happenings.rs`): an impact, a tool's blow, a nail driven,
//!   a member breaking or creaking, a footstep, rolling and sliding, the wind, each as a
//!   [`VoiceSpec`] ready to play (worked out on the game's thread).
//! - **Playing it** (audio thread: `voice.rs`, `resonator.rs`, `filters.rs`, `mixer.rs`,
//!   `space.rs`, `limiter.rs`): resonators and filters, placed for the listener, at most so many
//!   at once, limited so nothing clips.
//! - **Where it goes** (`output.rs`): the device (kira), nowhere (headless), or a recording;
//!   `offline.rs`, `wav.rs` and `analysis.rs` render, write and measure without ears.
//!
//! The settings menu sets the volumes with [`Audio::set_volumes`] ([`Volumes`]: master, effects,
//! ambience, each 0 to 1).

// Sound is output only: nothing here feeds back into the simulation, so the platform's maths
// library may be used (clippy.toml: "rendering-only code ... may opt out").
#![allow(clippy::disallowed_methods)]

pub mod acoustic;
pub mod analysis;
pub mod events;
pub mod filters;
pub mod happenings;
pub mod library;
pub mod limiter;
pub mod mixer;
pub mod modes;
pub mod offline;
pub mod output;
pub mod report;
pub mod resonator;
pub mod rng;
pub mod scenes;
pub mod space;
pub mod voice;
pub mod wav;

pub use events::{Body, Other};
pub use library::Library;
pub use mixer::{Command, Volumes};
pub use output::Audio;
pub use voice::VoiceSpec;

#[cfg(test)]
mod library_tests;
