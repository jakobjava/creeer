//! F0 spike (a): a minimal XPBD truss solver, used to answer two questions for ADR-0002:
//!
//! 1. **Speed:** can about 5,000 beams run at 60 Hz × 8+ substeps inside a 4 ms budget?
//! 2. **Accuracy:** how close to real steel does XPBD get at that substep count?
//!
//! XPBD ("extended position-based dynamics", Macklin et al. 2016/2019) moves particles
//! directly to satisfy constraints. A beam is a distance constraint whose *compliance*
//! (inverse stiffness, α = L / (E·A)) comes from the real material. With many small substeps
//! and one constraint pass each ("small steps"), XPBD is unconditionally stable. Very stiff
//! materials can end up *effectively softer* than they should be, though, and the spike
//! measures by how much.
//!
//! This is a prototype: the real solver (F2) adds bending/torsion, rigid bodies, contacts,
//! plasticity and fracture.

use creeer_math::glam::DVec3;
use rayon::prelude::*;

/// Standard gravity (m/s²), pointing down.
pub const GRAVITY: DVec3 = DVec3::new(0.0, -9.806_65, 0.0);

/// Elastic material properties.
#[derive(Clone, Copy, Debug)]
pub struct Material {
    /// Young's modulus E (Pa).
    pub youngs_modulus: f64,
    /// Density ρ (kg/m³).
    pub density: f64,
}

/// Structural steel (S235/S355 class).
pub const STEEL: Material = Material {
    youngs_modulus: 200.0e9,
    density: 7850.0,
};

#[derive(Clone, Copy, Debug)]
struct Beam {
    a: usize,
    b: usize,
    rest_length: f64,
    /// α = L / (E·A), in m/N.
    compliance: f64,
}

/// One connected structure (a simulation island).
#[derive(Clone, Debug)]
pub struct Truss {
    position: Vec<DVec3>,
    previous: Vec<DVec3>,
    velocity: Vec<DVec3>,
    /// Per-axis inverse mass. A zero component fixes the node along that axis, which is how
    /// supports are modelled: zero on all axes is a pin, zero on some axes is a roller.
    inv_mass: Vec<DVec3>,
    /// Lumped node mass (kg): half of every member attached to the node.
    mass: Vec<f64>,
    rest: Vec<DVec3>,
    beams: Vec<Beam>,
    /// Section indices used to measure midspan sag.
    midspan_nodes: [usize; 4],
    /// Viscous velocity damping (1/s). Zero for dynamics; used to reach static equilibrium.
    damping: f64,
}

/// Geometry of a box-truss bridge.
#[derive(Clone, Copy, Debug)]
pub struct BridgeSpec {
    /// Number of bays along the span.
    pub bays: usize,
    /// Length of one bay (m).
    pub bay_length: f64,
    /// Height and width of the square cross-section (m).
    pub depth: f64,
    /// Cross-sectional area of every member (m²).
    pub member_area: f64,
    /// Member material.
    pub material: Material,
}

impl BridgeSpec {
    /// The spike's reference bridge: 48 m span, 2 m deep, steel tubes of 10 cm².
    pub const REFERENCE: Self = Self {
        bays: 24,
        bay_length: 2.0,
        depth: 2.0,
        member_area: 1.0e-3,
        material: STEEL,
    };

    /// Span between the supports (m).
    pub fn span(&self) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let bays = self.bays as f64;
        bays * self.bay_length
    }
}

impl Truss {
    /// A simply supported box-truss bridge along +X starting at `origin`.
    ///
    /// Each cross-section is a braced square (4 nodes, 5 members); consecutive sections are
    /// joined by 4 chords and 4 face diagonals. Supports as on a real simply supported bridge:
    /// the two bottom nodes at the start are pinned; the two at the end sit on a roller that
    /// slides along the span. (Pinning both ends would stop the bottom chord from lengthening
    /// under load and turn the bridge into a much stiffer tied arch.)
    pub fn box_bridge(origin: DVec3, spec: BridgeSpec) -> Self {
        let half = spec.depth / 2.0;
        let corners = [
            DVec3::new(0.0, 0.0, -half),
            DVec3::new(0.0, 0.0, half),
            DVec3::new(0.0, spec.depth, half),
            DVec3::new(0.0, spec.depth, -half),
        ];
        let node = |section: usize, corner: usize| 4 * section + corner;

        let mut position = Vec::new();
        for section in 0..=spec.bays {
            #[allow(clippy::cast_precision_loss)]
            let x = section as f64 * spec.bay_length;
            for corner in corners {
                position.push(origin + DVec3::X * x + corner);
            }
        }

        let mut pairs = Vec::new();
        for s in 0..=spec.bays {
            for (a, b) in [(0, 1), (1, 2), (2, 3), (3, 0), (0, 2)] {
                pairs.push((node(s, a), node(s, b)));
            }
            if s < spec.bays {
                for c in 0..4 {
                    pairs.push((node(s, c), node(s + 1, c)));
                }
                for (a, b) in [(0, 1), (3, 2), (0, 3), (1, 2)] {
                    pairs.push((node(s, a), node(s + 1, b)));
                }
            }
        }

        let ea = spec.material.youngs_modulus * spec.member_area;
        let mut mass = vec![0.0; position.len()];
        let beams = pairs
            .into_iter()
            .map(|(a, b)| {
                let rest_length = (position[a] - position[b]).length();
                let member_mass = spec.material.density * spec.member_area * rest_length;
                mass[a] += member_mass / 2.0;
                mass[b] += member_mass / 2.0;
                Beam {
                    a,
                    b,
                    rest_length,
                    compliance: rest_length / ea,
                }
            })
            .collect();

        let mut inv_mass: Vec<DVec3> = mass.iter().map(|m| DVec3::splat(1.0 / m)).collect();
        for pinned in [node(0, 0), node(0, 1)] {
            inv_mass[pinned] = DVec3::ZERO;
        }
        for roller in [node(spec.bays, 0), node(spec.bays, 1)] {
            inv_mass[roller] = DVec3::new(inv_mass[roller].x, 0.0, 0.0);
        }

        let mid = spec.bays / 2;
        Self {
            previous: position.clone(),
            velocity: vec![DVec3::ZERO; position.len()],
            rest: position.clone(),
            position,
            inv_mass,
            mass,
            beams,
            midspan_nodes: [node(mid, 0), node(mid, 1), node(mid, 2), node(mid, 3)],
            damping: 0.0,
        }
    }

    /// Number of beams (constraints).
    pub fn beam_count(&self) -> usize {
        self.beams.len()
    }

    /// Number of nodes (particles).
    pub fn node_count(&self) -> usize {
        self.position.len()
    }

    /// Total mass of all members (kg), including pinned nodes.
    pub fn total_mass(&self, spec: &BridgeSpec) -> f64 {
        self.beams
            .iter()
            .map(|b| spec.material.density * spec.member_area * b.rest_length)
            .sum()
    }

    /// Downward displacement of the midspan cross-section (m, positive = sagging).
    pub fn midspan_sag(&self) -> f64 {
        self.midspan_nodes
            .iter()
            .map(|&i| self.rest[i].y - self.position[i].y)
            .sum::<f64>()
            / 4.0
    }

    /// Advances by `dt` seconds using `substeps` XPBD substeps with one constraint pass each.
    pub fn step(&mut self, dt: f64, substeps: u32) {
        let h = dt / f64::from(substeps);
        for _ in 0..substeps {
            self.integrate(h);
            self.solve_beams(h);
            self.update_velocities(h);
        }
    }

    fn integrate(&mut self, h: f64) {
        for i in 0..self.position.len() {
            self.previous[i] = self.position[i];
            // Gravity acts only along axes the node is free to move in.
            let free = DVec3::select(self.inv_mass[i].cmpgt(DVec3::ZERO), DVec3::ONE, DVec3::ZERO);
            self.velocity[i] += GRAVITY * free * h;
            self.position[i] += self.velocity[i] * h;
        }
    }

    fn solve_beams(&mut self, h: f64) {
        let inv_h2 = 1.0 / (h * h);
        for beam in &self.beams {
            let (wa, wb) = (self.inv_mass[beam.a], self.inv_mass[beam.b]);
            let delta = self.position[beam.a] - self.position[beam.b];
            let length = delta.length();
            if length < 1e-12 {
                continue;
            }
            let normal = delta / length;
            // Generalised inverse mass along the beam, nᵀ·W·n, summed over both ends.
            let w = (normal * normal).dot(wa + wb);
            if w == 0.0 {
                continue;
            }
            let violation = length - beam.rest_length;
            // XPBD update with λ reset every substep: Δλ = −C / (w + α/h²).
            let dlambda = -violation / (w + beam.compliance * inv_h2);
            self.position[beam.a] += wa * normal * dlambda;
            self.position[beam.b] -= wb * normal * dlambda;
        }
    }

    fn update_velocities(&mut self, h: f64) {
        let keep = 1.0 / (1.0 + self.damping * h);
        for i in 0..self.position.len() {
            self.velocity[i] = (self.position[i] - self.previous[i]) / h * keep;
        }
    }

    /// FNV-1a hash over the exact bits of all positions and velocities, for determinism tests.
    pub fn state_hash(&self) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for v in self.position.iter().chain(&self.velocity) {
            for component in v.to_array() {
                for byte in component.to_bits().to_le_bytes() {
                    hash ^= u64::from(byte);
                    hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
                }
            }
        }
        hash
    }
}

/// Static midspan deflection of the bridge under its own weight, treating the truss as an
/// equivalent Timoshenko beam (bending + shear), simply supported, uniform load w:
///
/// - bending: δ_b = 5·w·L⁴ / (384·E·I), with four chords of area A at d/2 from the neutral
///   axis giving I = A·d²;
/// - shear:   δ_s = w·L² / (8·GA), where the shear stiffness comes from the diagonals of
///   the two vertical faces: per face GA = E·A·sin²φ·cosφ (φ = diagonal angle).
pub fn analytic_midspan_sag(spec: &BridgeSpec, total_mass: f64) -> f64 {
    let span = spec.span();
    let e = spec.material.youngs_modulus;
    let w = total_mass * GRAVITY.length() / span;
    let i = spec.member_area * spec.depth * spec.depth;
    let bending = 5.0 * w * span * span * span * span / (384.0 * e * i);
    let diagonal = creeer_math::det::hypot(spec.bay_length, spec.depth);
    let (sin, cos) = (spec.depth / diagonal, spec.bay_length / diagonal);
    let shear_stiffness = 2.0 * e * spec.member_area * sin * sin * cos;
    let shear = w * span * span / (8.0 * shear_stiffness);
    bending + shear
}

/// Several independent trusses: the spike's 5,000-beam scene.
#[derive(Clone, Debug)]
pub struct TrussWorld {
    /// Independent islands.
    pub trusses: Vec<Truss>,
}

impl TrussWorld {
    /// `count` reference bridges side by side.
    pub fn bridges(count: usize) -> Self {
        let trusses = (0..count)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let z = i as f64 * 5.0;
                Truss::box_bridge(DVec3::new(0.0, 0.0, z), BridgeSpec::REFERENCE)
            })
            .collect();
        Self { trusses }
    }

    /// Total beams across all islands.
    pub fn beam_count(&self) -> usize {
        self.trusses.iter().map(Truss::beam_count).sum()
    }

    /// Steps every island on the calling thread.
    pub fn step_sequential(&mut self, dt: f64, substeps: u32) {
        for truss in &mut self.trusses {
            truss.step(dt, substeps);
        }
    }

    /// Steps islands in parallel (rayon). Islands share no nodes, so no synchronisation is
    /// needed and the result is bit-identical to the sequential version.
    pub fn step_parallel(&mut self, dt: f64, substeps: u32) {
        self.trusses
            .par_iter_mut()
            .for_each(|truss| truss.step(dt, substeps));
    }

    /// Combined hash of all islands.
    pub fn state_hash(&self) -> u64 {
        self.trusses
            .iter()
            .fold(0, |acc, t| acc.rotate_left(5) ^ t.state_hash())
    }
}

/// Simulates one reference bridge released from its unloaded shape until it comes to rest
/// and returns its static midspan sag (m).
pub fn measure_static_sag(substeps: u32) -> f64 {
    const DT: f64 = 1.0 / 60.0;
    let mut truss = Truss::box_bridge(DVec3::ZERO, BridgeSpec::REFERENCE);
    // Damping removes the oscillation so the bridge comes to rest; it does not move the
    // equilibrium, which only depends on stiffness and load.
    truss.damping = 3.0;
    for _ in 0..15 * 60 {
        truss.step(DT, substeps);
    }
    truss.midspan_sag()
}

#[cfg(test)]
mod compare_tests;
mod dynamics;
mod fem;
mod implicit;
mod lattice;
#[cfg(test)]
mod tests;
mod vbd;

pub use dynamics::{Oscillation, explicit_reference_response, oscillation, rayleigh_frequency};
pub use fem::{linear_static_displacement, linear_static_sag};
pub use implicit::{ImplicitTruss, Linearisation, Scheme, sparse_static_displacement};
pub use vbd::VbdTruss;
