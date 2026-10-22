//! wgpu renderer (ADR-0003).
//!
//! Everything is drawn in camera-relative space (see `creeer_math::render_space`) with a
//! reversed-Z infinite projection, so precision holds from centimetres to planetary scale.

pub mod atmosphere;
pub mod camera;
pub mod capture;
pub mod debug_lines;
mod half;
pub mod heatmap;
pub mod mesh;
pub mod primitives;
pub mod renderer;
pub mod scene;
mod shadow;
pub mod terrain;
pub mod vertex;

pub use atmosphere::{Atmosphere, SeaSurface, Sky};
pub use camera::Camera;
pub use capture::Image;
pub use debug_lines::{Color, DebugLines, LineVertex};
pub use heatmap::{Heatmap, Palette};
pub use mesh::{MeshArena, MeshData, MeshId, MeshVertex};
pub use renderer::{Frame, GpuTime, OverlayFrame, RenderError, RenderStats, Renderer};
pub use scene::{Instance, Lighting, Material, Scene, srgb_to_linear};
pub use shadow::ShadowQuality;
pub use terrain::{TerrainFrame, TerrainMeshData, TerrainVertex};
/// Re-exported so the client does not need its own (possibly mismatched) wgpu dependency.
pub use wgpu;
