//! wgpu renderer.
//!
//! Everything is drawn in camera-relative space (see `creeer_math::render_space`) with a
//! reversed-Z infinite projection, so precision holds from centimetres to planetary scale.

pub mod camera;
pub mod debug_lines;
pub mod renderer;

pub use camera::Camera;
pub use debug_lines::{Color, DebugLines, LineVertex};
pub use renderer::{Frame, OverlayFrame, RenderError, Renderer};
/// Re-exported so the client does not need its own (possibly mismatched) wgpu dependency.
pub use wgpu;
