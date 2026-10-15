//! Immediate-mode debug lines in f64 simulation space.
//!
//! Lines are collected with f64 coordinates and converted to camera-relative f32 only when
//! drawn, so they stay precise anywhere in a solar system.

use bytemuck::{Pod, Zeroable};
use creeer_math::glam::DVec3;
use creeer_math::render_space::to_render_space;

/// RGBA colour, sRGB-encoded (written as-is to the non-sRGB swapchain egui requires).
pub type Color = [f32; 4];

/// One line endpoint as uploaded to the GPU.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct LineVertex {
    /// Camera-relative position (m).
    pub position: [f32; 3],
    /// RGBA colour.
    pub color: Color,
}

/// A list of line segments, rebuilt every frame.
#[derive(Clone, Debug, Default)]
pub struct DebugLines {
    segments: Vec<(DVec3, DVec3, Color)>,
}

impl DebugLines {
    /// Removes all lines (call once per frame before adding new ones).
    pub fn clear(&mut self) {
        self.segments.clear();
    }

    /// Number of line segments.
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// True when no lines have been added.
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Adds a segment from `a` to `b`.
    pub fn line(&mut self, a: DVec3, b: DVec3, color: Color) {
        self.segments.push((a, b, color));
    }

    /// Adds three axis-aligned strokes through `center`, each `2 * half_size` long.
    pub fn cross(&mut self, center: DVec3, half_size: f64, color: Color) {
        for axis in [DVec3::X, DVec3::Y, DVec3::Z] {
            self.line(center - axis * half_size, center + axis * half_size, color);
        }
    }

    /// Adds a square grid in the XZ plane centred on `center`, extending `half_extent` in
    /// each direction with lines every `spacing` metres.
    ///
    /// # Panics
    /// If `spacing` is not positive.
    pub fn grid_xz(&mut self, center: DVec3, half_extent: f64, spacing: f64, color: Color) {
        assert!(spacing > 0.0, "grid spacing must be positive");
        #[allow(clippy::cast_possible_truncation)] // grids have a sane number of lines
        let n = (half_extent / spacing).floor() as i64;
        #[allow(clippy::cast_precision_loss)]
        let extent = n as f64 * spacing;
        for i in -n..=n {
            #[allow(clippy::cast_precision_loss)]
            let offset = i as f64 * spacing;
            self.line(
                center + DVec3::new(offset, 0.0, -extent),
                center + DVec3::new(offset, 0.0, extent),
                color,
            );
            self.line(
                center + DVec3::new(-extent, 0.0, offset),
                center + DVec3::new(extent, 0.0, offset),
                color,
            );
        }
    }

    /// Adds a circle in the XZ plane approximated by `segments` straight pieces.
    pub fn circle_xz(&mut self, center: DVec3, radius: f64, segments: u32, color: Color) {
        let point = |i: u32| {
            let angle = std::f64::consts::TAU * f64::from(i) / f64::from(segments.max(3));
            let (sin, cos) = creeer_math::det::sin_cos(angle);
            center + DVec3::new(cos * radius, 0.0, sin * radius)
        };
        let segments = segments.max(3);
        for i in 0..segments {
            self.line(point(i), point(i + 1), color);
        }
    }

    /// Writes all segments as camera-relative vertex pairs into `out` (cleared first).
    pub fn write_vertices(&self, camera: DVec3, out: &mut Vec<LineVertex>) {
        out.clear();
        out.reserve(self.segments.len() * 2);
        for &(a, b, color) in &self.segments {
            for p in [a, b] {
                out.push(LineVertex {
                    position: to_render_space(p, camera).to_array(),
                    color,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: Color = [1.0; 4];

    #[test]
    fn grid_has_two_lines_per_step() {
        let mut lines = DebugLines::default();
        lines.grid_xz(DVec3::ZERO, 10.0, 1.0, WHITE);
        // -10..=10 is 21 positions, each with an X-parallel and a Z-parallel line.
        assert_eq!(lines.len(), 42);
    }

    #[test]
    fn vertices_are_camera_relative() {
        let camera = DVec3::new(0.0, 500_000.0, 0.0);
        let mut lines = DebugLines::default();
        lines.line(
            camera + DVec3::X,
            camera + DVec3::new(0.0, 0.0, -2.0),
            WHITE,
        );
        let mut out = Vec::new();
        lines.write_vertices(camera, &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].position, [1.0, 0.0, 0.0]);
        assert_eq!(out[1].position, [0.0, 0.0, -2.0]);
    }

    #[test]
    fn circle_closes_and_keeps_its_radius() {
        let mut lines = DebugLines::default();
        lines.circle_xz(DVec3::ZERO, 5.0, 32, WHITE);
        assert_eq!(lines.len(), 32);
        let mut out = Vec::new();
        lines.write_vertices(DVec3::ZERO, &mut out);
        for v in &out {
            let r = (v.position[0].powi(2) + v.position[2].powi(2)).sqrt();
            assert!((r - 5.0).abs() < 1e-5);
        }
        let first = out.first().unwrap().position;
        let last = out.last().unwrap().position;
        for i in 0..3 {
            assert!((first[i] - last[i]).abs() < 1e-5, "circle is not closed");
        }
    }
}
