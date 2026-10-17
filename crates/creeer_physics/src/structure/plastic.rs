//! Plasticity and fracture of members (plastic hinges, hardening, strain rate).
//!
//! Plastic flow happens inside the implicit step: a member's energy is the incremental
//! elasto-plastic potential
//!
//! ψ(s) = min over p of ½·(s − p)ᵀ·K·(s − p) + ½·γ·(s − p − ŝ)ᵀ·K·(s − p − ŝ) + Σ Dᵢ(|pᵢ|),
//!
//! with s the member's measures less its permanent deformation, ŝ the same at the step's
//! velocity reference (`Work::s_hat`), γ = c/β the material damping per step, and Dᵢ the
//! plastic work of an increment of each measure (`flow.rs`): its capacity, hardened by
//! the increment and lifted by its strain rate. The capacities at the step's start come from
//! the flow stress σ after the member's plastic strain so far (`Material::flow_stress`):
//! axial N_p = σ·A, torque T_p = τ·Z_t (τ = σ/√3, von Mises), end moments M_p = σ·Z about
//! each axis (plastic section modulus). ψ is convex and smooth, and its gradient
//! K·(s − p*) + γ·K·(s − p* − ŝ) is the member force (elastic plus damping) capped at the
//! capacity, so plastic hinges form and run within the step. After the step the minimiser
//! p* becomes permanent deformation.
//!
//! The damper acts on the elastic strain rate, in series with the plastic slider (as
//! internal friction does in steel): it damps vibration, but no strain rate, however high,
//! lifts the member force above its capacity. A dashpot in parallel would carry a crash
//! load of its own, and the crash energy would go into viscosity instead of plastic work.
//! Completing the square, ψ is the plain elasto-plastic split of v = s − γ/(1+γ)·ŝ with
//! the plastic work divided by 1 + γ.
//!
//! Axial force and bending interact as (N/N_p)² + |M/M_p| ≤ 1 (plastic hinge of a
//! rectangular section): the moment capacities are reduced by the axial force at the start
//! of the step. The two bending axes are capped independently.
//!
//! The accumulated plastic strain (axial strain, hinge rotation spread over the section
//! depth, torsional shear strain over √3) breaks the member once it exceeds the material's
//! elongation at break. Necking is not modelled: past its ultimate strength a member flows
//! on at it until it breaks, where a real bar would lose strength at its narrowest point.

use super::Structure;
use super::element::{Element, MEASURES, Stiffness};
use super::flow::Flow;
use rayon::prelude::*;

/// What happened to the members in one check.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Yielding {
    /// Members that deformed plastically.
    pub yielded: usize,
    /// Members that broke.
    pub broke: usize,
    /// Members whose plastic strain grew by more than [`FLOWING`] (still deforming, not
    /// just touching their capacity).
    pub flowing: usize,
    /// Energy dissipated by plastic flow (J).
    pub work: f64,
}

/// A plastic strain increment per step above which a member counts as flowing.
pub const FLOWING: f64 = 1.0e-6;

impl Element {
    /// How each measure (force or moment) flows plastically in a step of `h` seconds, for
    /// elastic measures `s` at its start: capacities from the flow stress after the plastic
    /// strain so far (`Material::flow_stress`), hardening and strain rate (`flow.rs`).
    pub(crate) fn capacities(&self, s: &[f64; MEASURES], h: f64) -> [Flow; MEASURES] {
        let (sec, sy) = (
            &self.section,
            self.material.flow_stress(self.plastic_strain),
        );
        let n_p = sy * sec.area;
        let axial = self.stiffness.apply(s)[0];
        let room = (1.0 - (axial / n_p) * (axial / n_p)).max(0.0);
        let (my, mz) = (
            sy * sec.plastic_modulus[0] * room,
            sy * sec.plastic_modulus[1] * room,
        );
        let caps = [n_p, sy / 3f64.sqrt() * sec.plastic_torsion, my, my, mz, mz];
        let per = self.per_unit();
        std::array::from_fn(|m| Flow::new(caps[m], &self.material, self.plastic_strain, per[m], h))
    }

    /// Equivalent plastic strain per unit of each measure: axial strain; torsional shear
    /// strain over √3; a hinge rotation θ strains the outer fibre by about θ/2 over a hinge
    /// length of the section depth.
    fn per_unit(&self) -> [f64; MEASURES] {
        let radius = self.section.extent[0].max(self.section.extent[1]);
        let l = self.rest_length;
        [1.0 / l, radius / l / 3f64.sqrt(), 0.5, 0.5, 0.5, 0.5]
    }

    /// The plastic part p* that minimises the elasto-plastic potential at measures `s`.
    pub(crate) fn plastic_split(
        &self,
        s: &[f64; MEASURES],
        flows: &[Flow; MEASURES],
    ) -> [f64; MEASURES] {
        let k = |m: usize, n: usize| self.stiffness.entry(m, n);
        let mut p = [0.0; MEASURES];
        for m in 0..2 {
            if k(m, m) > 0.0 {
                p[m] = flows[m].increment(s[m], k(m, m));
            }
        }
        // End-moment pairs (a, b) about each axis are coupled by k₁₂: coordinate descent,
        // contracting by (k₁₂/k₁₁)² ≤ ¼ per sweep.
        for pair in [2, 4] {
            let (k11, k12) = (k(pair, pair), k(pair, pair + 1));
            if k11 <= 0.0 {
                continue;
            }
            for _ in 0..24 {
                let a =
                    flows[pair].increment(s[pair] + k12 / k11 * (s[pair + 1] - p[pair + 1]), k11);
                let b = flows[pair + 1].increment(s[pair + 1] + k12 / k11 * (s[pair] - a), k11);
                #[allow(clippy::float_cmp)] // an exact fixed point: nothing changes any more
                let done = a == p[pair] && b == p[pair + 1];
                p[pair] = a;
                p[pair + 1] = b;
                if done {
                    break;
                }
            }
        }
        p
    }

    /// The plastic increment p* that minimises ψ (see the module docs) at measures `s`,
    /// with damping `gamma` = c/β relative to the measures `s_hat`, `scaled` the member's
    /// flows with their work divided by 1 + γ (`Flow::scaled`, `Work::scaled`).
    pub(crate) fn damped_split(
        &self,
        s: &[f64; MEASURES],
        s_hat: &[f64; MEASURES],
        gamma: f64,
        scaled: &[Flow; MEASURES],
    ) -> [f64; MEASURES] {
        let shift = gamma / (1.0 + gamma);
        let v = std::array::from_fn(|m| s[m] - shift * s_hat[m]);
        self.plastic_split(&v, scaled)
    }

    /// The potential ψ (elastic, damping and plastic work of the step), the stored elastic
    /// energy, and the member forces K·(s − p*) + γ·K·(s − p* − ŝ); `flows` and `scaled` the
    /// member's flows and the same divided by 1 + γ.
    pub(crate) fn elastoplastic(
        &self,
        s: &[f64; MEASURES],
        s_hat: &[f64; MEASURES],
        gamma: f64,
        (flows, scaled): (&[Flow; MEASURES], &[Flow; MEASURES]),
    ) -> (f64, f64, [f64; MEASURES]) {
        let p = self.damped_split(s, s_hat, gamma, scaled);
        let elastic: [f64; MEASURES] = std::array::from_fn(|m| s[m] - p[m]);
        let rate: [f64; MEASURES] = std::array::from_fn(|m| elastic[m] - s_hat[m]);
        let stored = self.stiffness.energy(&elastic, &elastic);
        let damping = gamma * self.stiffness.energy(&rate, &rate);
        // Only measures that flowed: 0·∞ would be NaN for members that never yield.
        let dissipated: f64 = p
            .iter()
            .zip(flows)
            .filter(|(p, _)| **p != 0.0)
            .map(|(p, f)| f.work(p.abs()))
            .sum();
        let (spring, dashpot) = (self.stiffness.apply(&elastic), self.stiffness.apply(&rate));
        let force = std::array::from_fn(|m| spring[m] + gamma * dashpot[m]);
        (stored + damping + dissipated, stored, force)
    }

    /// Plastic strain represented by plastic increments `p`.
    pub(crate) fn strain_of(&self, p: &[f64; MEASURES]) -> f64 {
        p.iter()
            .zip(self.per_unit())
            .map(|(p, g)| (p * g).abs())
            .sum()
    }
}

impl Structure {
    /// Makes the plastic flow of the last step permanent (the capacities, damping and
    /// velocity reference the step used are in `work`) and breaks members strained past the
    /// material's elongation at break.
    pub(super) fn yield_members(&mut self, work: &super::solver::Work) -> (Yielding, Vec<usize>) {
        let rotations: Vec<_> = self
            .orientation
            .iter()
            .map(|q| creeer_math::glam::DMat3::from_quat(*q))
            .collect();
        let position = &self.position;
        // Every member on its own, in parallel; the results are summed in member order.
        let splits = self.splits;
        let outcomes: Vec<(Yielding, bool)> = self
            .elements
            .par_iter_mut()
            .zip(
                work.caps
                    .par_iter()
                    .zip(work.scaled.par_iter().zip(&work.s_hat)),
            )
            .with_min_len(128)
            .map(|(e, (caps, (scaled, s_hat)))| {
                if e.broken {
                    return (Yielding::default(), false);
                }
                let s = e.measures(position[e.a], rotations[e.a], position[e.b], rotations[e.b]);
                let gamma = e.material.damping / work.beta;
                let p = e.damped_split(&s, s_hat, gamma, scaled);
                e.flow_rate = p.map(|p| p / work.h);
                // Compressed enough to be split (`buckling.rs`), on its elastic part.
                let wants = splits && e.wants_split(&std::array::from_fn(|m| s[m] - p[m]));
                if p.iter().all(|v| *v == 0.0) {
                    return (Yielding::default(), wants);
                }
                for (plastic, dp) in e.plastic.iter_mut().zip(&p) {
                    *plastic += dp;
                }
                let strain = e.strain_of(&p);
                e.plastic_strain += strain;
                let broke = e.plastic_strain > e.material.elongation;
                if broke {
                    e.broken = true;
                    e.stiffness = Stiffness::zeroed();
                }
                let y = Yielding {
                    yielded: 1,
                    broke: usize::from(broke),
                    flowing: usize::from(strain > FLOWING),
                    // Only measures that flowed: 0·∞ would be NaN.
                    work: p
                        .iter()
                        .zip(caps)
                        .filter(|(p, _)| **p != 0.0)
                        .map(|(p, f)| f.work(p.abs()))
                        .sum(),
                };
                (y, wants && !broke)
            })
            .collect();
        let wanted = (0..outcomes.len()).filter(|&i| outcomes[i].1).collect();
        let total = outcomes
            .iter()
            .fold(Yielding::default(), |r, (y, _)| Yielding {
                yielded: r.yielded + y.yielded,
                broke: r.broke + y.broke,
                flowing: r.flowing + y.flowing,
                work: r.work + y.work,
            });
        (total, wanted)
    }
}

impl Structure {
    /// How much of its plastic capacity each member uses, for overlays: 0 unloaded, 1 at
    /// yield (max over its ends of (N/N_p)² + |M/M_p|, and |T/T_p|). Broken members: 1.
    pub fn utilization(&self) -> Vec<f64> {
        self.elements
            .iter()
            .map(|e| {
                if e.broken {
                    return 1.0;
                }
                let r = |node: usize| creeer_math::glam::DMat3::from_quat(self.orientation[node]);
                let s = e.measures(self.position[e.a], r(e.a), self.position[e.b], r(e.b));
                let sigma = e.stiffness.apply(&s);
                let caps = e.capacities(&[0.0; MEASURES], 1.0).map(|f| f.capacity);
                let n = sigma[0] / caps[0];
                let ends = (0..2).map(|end| {
                    let (my, mz) = (
                        sigma[2 + end] / caps[2 + end],
                        sigma[4 + end] / caps[4 + end],
                    );
                    n * n + (my * my + mz * mz).sqrt()
                });
                ends.fold((sigma[1] / caps[1]).abs(), f64::max)
            })
            .collect()
    }

    /// Each member's end nodes, a radius to draw it with, and whether it is broken.
    pub fn members(&self) -> Vec<(usize, usize, f64, bool)> {
        self.elements
            .iter()
            .map(|e| {
                (
                    e.a,
                    e.b,
                    e.section.extent[0].max(e.section.extent[1]),
                    e.broken,
                )
            })
            .collect()
    }

    /// Adds a lumped mass (kg) with rotational inertia `inertia` (kg·m², rest frame) to
    /// `node`: cargo, an engine. Do it before making the body's solver.
    pub fn add_mass(&mut self, node: usize, mass: f64, inertia: creeer_math::glam::DMat3) {
        self.mass[node] += mass;
        self.inertia[node] += inertia;
    }

    /// Moves the whole structure, at rest, by `offset`.
    pub fn translate(&mut self, offset: creeer_math::glam::DVec3) {
        for p in self.position.iter_mut().chain(self.rest.iter_mut()) {
            *p += offset;
        }
    }
}
