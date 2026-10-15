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

/// Exact linear-elastic static midspan sag of the *same discrete truss*, solved with the
/// direct stiffness method (finite elements): assemble K, solve K·u = f.
///
/// This is the reference XPBD must converge to. Unlike beam theory it has no modelling
/// approximation (discrete bays, real diagonals, real supports) and no iteration error.
/// Dense Cholesky is fine for one bridge (300 unknowns); the production quasi-static solver
/// (F2) will use a sparse factorisation.
pub fn linear_static_sag(spec: BridgeSpec) -> f64 {
    let truss = Truss::box_bridge(DVec3::ZERO, spec);
    let n = 3 * truss.node_count();
    let ea = spec.material.youngs_modulus * spec.member_area;

    // Stiffness of a bar along unit vector e: (EA/L)·[e·eᵀ, −e·eᵀ; −e·eᵀ, e·eᵀ].
    let mut k = vec![0.0; n * n];
    for beam in &truss.beams {
        let e = (truss.rest[beam.a] - truss.rest[beam.b]) / beam.rest_length;
        let stiffness = ea / beam.rest_length;
        for (i, ei) in e.to_array().into_iter().enumerate() {
            for (j, ej) in e.to_array().into_iter().enumerate() {
                let kij = stiffness * ei * ej;
                for (row_node, col_node, sign) in [
                    (beam.a, beam.a, 1.0),
                    (beam.b, beam.b, 1.0),
                    (beam.a, beam.b, -1.0),
                    (beam.b, beam.a, -1.0),
                ] {
                    k[(3 * row_node + i) * n + 3 * col_node + j] += sign * kij;
                }
            }
        }
    }

    // Gravity on each node's own mass (zero inverse mass on fixed axes means "support").
    let mut f = vec![0.0; n];
    let mut fixed = vec![false; n];
    for (node, w) in truss.inv_mass.iter().enumerate() {
        for (axis, inv) in w.to_array().into_iter().enumerate() {
            fixed[3 * node + axis] = inv == 0.0;
        }
    }
    let half_masses = truss.beams.iter().map(|b| {
        let m = spec.material.density * spec.member_area * b.rest_length / 2.0;
        (b.a, b.b, m)
    });
    for (a, b, m) in half_masses {
        f[3 * a + 1] += m * GRAVITY.y;
        f[3 * b + 1] += m * GRAVITY.y;
    }

    // Supports: replace fixed rows/columns by identity (u = 0 there).
    for d in (0..n).filter(|&d| fixed[d]) {
        for j in 0..n {
            k[d * n + j] = 0.0;
            k[j * n + d] = 0.0;
        }
        k[d * n + d] = 1.0;
        f[d] = 0.0;
    }

    let u = cholesky_solve(&mut k, &mut f, n);
    truss
        .midspan_nodes
        .iter()
        .map(|&node| -u[3 * node + 1])
        .sum::<f64>()
        / 4.0
}

/// Solves K·u = f in place for a symmetric positive-definite K (row-major n×n).
///
/// # Panics
/// If K is not positive definite (e.g. the structure is a mechanism).
fn cholesky_solve(k: &mut [f64], f: &mut [f64], n: usize) -> Vec<f64> {
    // Factor K = L·Lᵀ, storing L in the lower triangle.
    for j in 0..n {
        let mut diag = k[j * n + j];
        for p in 0..j {
            diag -= k[j * n + p] * k[j * n + p];
        }
        assert!(
            diag > 0.0,
            "stiffness matrix not positive definite at dof {j}"
        );
        let ljj = diag.sqrt();
        k[j * n + j] = ljj;
        for i in j + 1..n {
            let mut sum = k[i * n + j];
            for p in 0..j {
                sum -= k[i * n + p] * k[j * n + p];
            }
            k[i * n + j] = sum / ljj;
        }
    }
    // Forward substitution L·y = f, then back substitution Lᵀ·u = y.
    for i in 0..n {
        let mut sum = f[i];
        for p in 0..i {
            sum -= k[i * n + p] * f[p];
        }
        f[i] = sum / k[i * n + i];
    }
    for i in (0..n).rev() {
        let mut sum = f[i];
        for p in i + 1..n {
            sum -= k[p * n + i] * f[p];
        }
        f[i] = sum / k[i * n + i];
    }
    f.to_vec()
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
mod tests {
    use super::*;

    const DT: f64 = 1.0 / 60.0;

    #[test]
    fn spike_scene_has_about_five_thousand_beams() {
        let world = TrussWorld::bridges(16);
        assert_eq!(world.trusses[0].beam_count(), 317);
        assert_eq!(world.beam_count(), 5072);
    }

    #[test]
    fn supports_hold_along_their_fixed_axes_only() {
        let mut truss = Truss::box_bridge(DVec3::ZERO, BridgeSpec::REFERENCE);
        let pins: Vec<usize> = (0..truss.node_count())
            .filter(|&i| truss.inv_mass[i] == DVec3::ZERO)
            .collect();
        let rollers: Vec<usize> = (0..truss.node_count())
            .filter(|&i| truss.inv_mass[i].x > 0.0 && truss.inv_mass[i].y == 0.0)
            .collect();
        assert_eq!((pins.len(), rollers.len()), (2, 2));
        for _ in 0..120 {
            truss.step(DT, 20);
        }
        for i in pins {
            assert_eq!(truss.position[i], truss.rest[i], "pinned node {i} moved");
        }
        for i in rollers {
            let (p, r) = (truss.position[i], truss.rest[i]);
            assert_eq!((p.y, p.z), (r.y, r.z), "roller {i} left its rail");
            // Under load the bottom chord lengthens, so the roller slides outwards.
            assert!(p.x > r.x, "roller {i} did not slide: {} -> {}", r.x, p.x);
        }
    }

    #[test]
    fn exact_fem_agrees_with_timoshenko_beam_theory() {
        // Validates the FEM reference itself. A discrete 24-bay truss is not exactly a
        // continuous beam, so a few percent of difference is expected (measured: 5.1 %).
        let spec = BridgeSpec::REFERENCE;
        let truss = Truss::box_bridge(DVec3::ZERO, spec);
        let beam_theory = analytic_midspan_sag(&spec, truss.total_mass(&spec));
        let fem = linear_static_sag(spec);
        let ratio = fem / beam_theory;
        assert!(
            (0.93..=1.0).contains(&ratio),
            "FEM/beam theory = {ratio:.4}"
        );
    }

    #[test]
    fn xpbd_converges_to_the_exact_solution_with_enough_substeps() {
        let exact = linear_static_sag(BridgeSpec::REFERENCE);
        let ratio = measure_static_sag(100) / exact;
        // Measured 1.047 at 100 substeps (1.012 at 200).
        assert!(
            (1.0..1.08).contains(&ratio),
            "XPBD/FEM at 100 substeps = {ratio:.4}"
        );
    }

    /// Characterisation of the spike's key finding (ADR-0002): at the originally planned 8
    /// substeps, XPBD makes steel far too soft. If a solver change fixes this, update the
    /// ADR, then this test.
    #[test]
    fn xpbd_at_eight_substeps_is_much_too_soft_for_steel() {
        let exact = linear_static_sag(BridgeSpec::REFERENCE);
        let ratio = measure_static_sag(8) / exact;
        assert!(ratio > 5.0, "XPBD/FEM at 8 substeps = {ratio:.2}");
    }

    #[test]
    fn parallel_and_sequential_stepping_are_bit_identical() {
        let mut a = TrussWorld::bridges(4);
        let mut b = a.clone();
        for _ in 0..30 {
            a.step_sequential(DT, 8);
            b.step_parallel(DT, 8);
        }
        assert_eq!(a.state_hash(), b.state_hash());
    }

    /// Cross-platform determinism: the exact same bits on Windows, Linux and macOS.
    /// Generated on Windows x86-64; never regenerate it to make a failure go away.
    #[test]
    fn state_hash_is_identical_on_every_platform() {
        let mut world = TrussWorld::bridges(2);
        for _ in 0..120 {
            world.step_sequential(DT, 8);
        }
        let hash = world.state_hash();
        if std::env::var_os("CREEER_PRINT_GOLDEN").is_some() {
            println!("truss golden hash: {hash:#018x}");
        }
        assert_eq!(hash, GOLDEN_TRUSS_HASH, "got {hash:#018x}");
    }

    const GOLDEN_TRUSS_HASH: u64 = 0x8d6e_356f_4f62_e490;
}
