//! What stands in one cell of the ground cover (`ground_cover.rs`), made on its worker thread
//! from the planet's seed and the cell alone.

use super::{Builder, Cell, Key, Placed, STEEPEST};
use creeer_art::ground::Shape;
use creeer_math::glam::DVec3;
use creeer_render::cover::CoverInstance;
use creeer_render::ground_pattern::{layer_mask, pattern_point};
use creeer_terrain::cube::CubeCoord;

/// SplitMix64: a small generator, the same on every machine.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    fn unit(&mut self) -> f64 {
        #[allow(clippy::cast_precision_loss)] // 53 bits
        let u = (self.next() >> 11) as f64 / (1u64 << 53) as f64;
        u
    }
}

fn snorm(x: f64) -> i8 {
    #[allow(clippy::cast_possible_truncation)] // clamped to ±127
    let v = (x.clamp(-1.0, 1.0) * 127.0).round() as i8;
    v
}

impl Builder {
    /// What stands in cell `key`.
    pub(super) fn cell(&self, (face, i, j): Key) -> Cell {
        let terrain = &*self.terrain;
        let planet = terrain.planet();
        let at = |u: f64, v: f64| CubeCoord {
            face,
            s: (f64::from(i) + u) * self.step,
            t: (f64::from(j) + v) * self.step,
        };
        let mut heights = terrain.heights().sampler(1.0);
        let datum = planet.datum;
        let ground = |heights: &mut creeer_terrain::tiles::HeightSampler<'_>, d: DVec3| {
            let up = d.normalize();
            let h = heights.height(up);
            (up * (datum.radius(up) + h), h)
        };
        let (origin, _) = ground(&mut heights, at(0.5, 0.5).direction());
        let up = origin.normalize();
        let Some(rock) = terrain.rock_at(origin - up * 0.05) else {
            return Cell::default();
        };
        let rock_colour =
            (planet.geology.rocks.rocks.get(usize::from(rock))).map_or([128; 3], |r| r.color);
        let reach = DVec3::splat(CELL_REACH);
        let edited = !terrain
            .edits()
            .affecting(origin - reach, origin + reach, 0.0)
            .is_empty();
        let side = self.step * planet.radius;
        let area = side * side;
        let key_hash = (u64::from(face) << 56)
            ^ (u64::from(i.cast_unsigned()) << 28)
            ^ u64::from(j.cast_unsigned());
        let mut rng =
            Rng(planet.seed ^ key_hash.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ 0x6772_6f75_6e64);
        let drawn = self.look.drawn();
        let mut placed = Vec::new();
        for d in &self.decoratives {
            let expected = d.per_m2 * area;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few hundred
            let n = (expected + rng.unit()).floor() as usize;
            let layer = d
                .layer
                .as_ref()
                .and_then(|l| self.look.names.iter().position(|n| n == l));
            let colour = d
                .token
                .as_ref()
                .and_then(|t| self.colours.get(t).copied())
                .unwrap_or(rock_colour);
            for _ in 0..n {
                let (u, v) = (rng.unit(), rng.unit());
                let (lean, tall, wide, turn, tone, keep) = (
                    rng.unit(),
                    rng.unit(),
                    rng.unit(),
                    rng.unit(),
                    rng.unit(),
                    rng.unit(),
                );
                let c = at(u, v);
                if c.s.abs() > 1.0 || c.t.abs() > 1.0 {
                    continue;
                }
                let (p, h) = ground(&mut heights, c.direction());
                let (_, height_gradient) = heights.height_gradient(p);
                let (_, radius_gradient) = datum.radius_gradient(p);
                let here = p.normalize();
                let normal = (here - radius_gradient - height_gradient).normalize_or(here);
                if normal.dot(here) < creeer_math::det::cos(STEEPEST)
                    || (edited && terrain.density(p).abs() > 0.02)
                {
                    continue;
                }
                if let Some(k) = layer {
                    let share = self.look.cover(p, normal, rock, h)[k];
                    let l = drawn[k];
                    let mask = layer_mask(pattern_point(p), k, (l.patch, l.density), share);
                    if keep >= f64::from(mask) {
                        continue;
                    }
                }
                let lerp = |(a, b): (f64, f64), t: f64| a + (b - a) * t;
                #[allow(clippy::cast_possible_truncation)] // sizes and offsets within a cell
                let instance = CoverInstance {
                    position: [0.0; 3],
                    height: lerp(d.height, tall) as f32,
                    width: lerp(d.width, wide) as f32,
                    up: [
                        snorm(normal.x),
                        snorm(normal.y),
                        snorm(normal.z),
                        snorm(turn * 2.0 - 1.0),
                    ],
                    colour: [colour[0], colour[1], colour[2], (tone * 255.0) as u8],
                    lean: lean as f32,
                };
                #[allow(clippy::cast_possible_truncation)] // within a cell
                let offset = (p - origin).as_vec3();
                placed.push(Placed {
                    offset,
                    instance,
                    shape: d.shape,
                });
            }
        }
        // Tufts' share of the ground is not grit's: keep both kinds apart for the renderer.
        placed.sort_by_key(|p| p.shape == Shape::Pebble);
        Cell { origin, placed }
    }
}

/// How far an edit may reach into a cell and change its ground (m).
const CELL_REACH: f64 = super::CELL * 1.5;
