//! The beam element: a straight member between two nodes that resists stretching, twisting
//! and bending (3D Timoshenko beam, co-rotational form).
//!
//! Every node has a position and an orientation. The element measures six deformations:
//!
//! | # | measure | formula |
//! |---|---|---|
//! | 0 | stretch (m) | l − L |
//! | 1 | twist (rad) | ½·(y_a × y_b + z_a × z_b)·e |
//! | 2, 3 | bending about the local y axis at a, b (rad) | (e × t_i)·y_i |
//! | 4, 5 | bending about the local z axis at a, b (rad) | (e × t_i)·z_i |
//!
//! Here e is the current chord direction, and t_i, y_i, z_i are the member's rest axes
//! carried along by node i's rotation. The end rotations are measured relative to the
//! chord, so any rigid motion of the element measures exactly zero, however large.
//! The elastic energy is ½·sᵀ·K·s with the classic beam stiffnesses (axial EA/L,
//! torsion GJ/L, and per bending plane the end-rotation pair k₁₁ = (4 + Φ)·EI / (L·(1 + Φ)),
//! k₁₂ = (2 − Φ)·EI / (L·(1 + Φ)), Φ = 12·EI / (κ·G·A·L²) for shear deformation).
//! For small deformations this is exactly the textbook 12×12 frame element (tested).
//!
//! Rotations stay valid up to about ±90° of deformation *within one element*, far beyond
//! where steel would have yielded (about 0.1 % strain).

use super::section::{Material, Section};
use creeer_math::glam::{DMat3, DVec3};

/// Number of deformation measures per element.
pub(crate) const MEASURES: usize = 6;

/// Gradient of one measure with respect to the element's degrees of freedom: position of
/// node a, rotation of a, position of b, rotation of b (rotations as small world-frame
/// rotation vectors).
pub(crate) type Gradient = [DVec3; 4];

/// Stiffness of one element against each of its deformation measures.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Stiffness {
    pub(super) axial: f64,
    pub(super) torsion: f64,
    /// (k₁₁, k₁₂) for bending about local y and about local z.
    pub(super) bend: [(f64, f64); 2],
}

impl Stiffness {
    pub(super) fn new(section: &Section, material: &Material, length: f64) -> Self {
        let (e, g) = (material.youngs_modulus, material.shear_modulus);
        let bend = |i: f64, kappa: f64| {
            let phi = 12.0 * e * i / (kappa * g * section.area * length * length);
            let scale = e * i / (length * (1.0 + phi));
            ((4.0 + phi) * scale, (2.0 - phi) * scale)
        };
        Self {
            axial: e * section.area / length,
            torsion: g * section.torsion / length,
            // Bending about y bends the member along z, so shear along z goes with it.
            bend: [
                bend(section.i_y, section.shear_factor[1]),
                bend(section.i_z, section.shear_factor[0]),
            ],
        }
    }

    /// No stiffness at all (a broken member).
    pub(crate) fn zeroed() -> Self {
        Self {
            axial: 0.0,
            torsion: 0.0,
            bend: [(0.0, 0.0); 2],
        }
    }

    /// Entry (m, n) of the block-diagonal 6×6 stiffness matrix K.
    pub(crate) fn entry(&self, m: usize, n: usize) -> f64 {
        match (m, n) {
            (0, 0) => self.axial,
            (1, 1) => self.torsion,
            (2..=5, 2..=5) if (m - 2) / 2 == (n - 2) / 2 => {
                let (k11, k12) = self.bend[(m - 2) / 2];
                if m == n { k11 } else { k12 }
            }
            _ => 0.0,
        }
    }

    /// Generalised stresses K·s: axial force (N), torque (N·m) and end moments (N·m).
    pub(crate) fn apply(&self, s: &[f64; MEASURES]) -> [f64; MEASURES] {
        let [(y11, y12), (z11, z12)] = self.bend;
        [
            self.axial * s[0],
            self.torsion * s[1],
            y11 * s[2] + y12 * s[3],
            y12 * s[2] + y11 * s[3],
            z11 * s[4] + z12 * s[5],
            z12 * s[4] + z11 * s[5],
        ]
    }

    /// ½·aᵀ·K·b (with a = b: the elastic energy).
    pub(crate) fn energy(&self, a: &[f64; MEASURES], b: &[f64; MEASURES]) -> f64 {
        0.5 * self
            .apply(a)
            .iter()
            .zip(b)
            .map(|(ka, b)| ka * b)
            .sum::<f64>()
    }
}

/// A beam between nodes `a` and `b`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Element {
    pub a: usize,
    pub b: usize,
    pub rest_length: f64,
    /// Rest axes: along the member, and the section's y and z (right-handed). Nodes start
    /// unrotated, so these are also the axes in each node's own frame.
    pub axis: DVec3,
    pub y: DVec3,
    pub z: DVec3,
    pub stiffness: Stiffness,
    pub section: Section,
    pub material: Material,
    /// Permanent (plastic) part of each deformation measure; the measures returned are the
    /// elastic remainder.
    pub plastic: [f64; MEASURES],
    /// Rate of `plastic` in the last step (per second): the damping's velocity reference
    /// continues the plastic flow as the scheme continues the motion.
    pub flow_rate: [f64; MEASURES],
    /// Accumulated equivalent plastic strain.
    pub plastic_strain: f64,
    /// Broken members carry nothing.
    pub broken: bool,
    /// A half of a split member: its share of the member's crookedness is already in its
    /// rest shape (`buckling.rs`).
    pub crooked: bool,
}

impl Element {
    /// A beam from `pa` to `pb` whose section y axis points as close to `up` as possible.
    pub(crate) fn new(
        (a, pa): (usize, DVec3),
        (b, pb): (usize, DVec3),
        up: DVec3,
        section: Section,
        material: Material,
    ) -> Self {
        let d = pb - pa;
        let rest_length = d.length();
        let axis = d / rest_length;
        let mut y = up - axis * axis.dot(up);
        if y.length_squared() < 1e-12 {
            // `up` is along the member: any perpendicular will do.
            y = axis.any_orthonormal_vector();
        }
        let y = y.normalize();
        Self {
            a,
            b,
            rest_length,
            axis,
            y,
            z: axis.cross(y),
            stiffness: Stiffness::new(&section, &material, rest_length),
            section,
            material,
            plastic: [0.0; MEASURES],
            flow_rate: [0.0; MEASURES],
            plastic_strain: 0.0,
            broken: false,
            crooked: false,
        }
    }

    /// Mass of the member (kg).
    pub(crate) fn mass(&self) -> f64 {
        self.material.density * self.section.area * self.rest_length
    }

    /// Rotational inertia this member adds to each of its end nodes (rest frame, kg·m²):
    /// the member's own torsional inertia about its axis, and across it the diagonal
    /// ("HRZ") lumping m·L²/78 of the consistent beam mass matrix plus the section's
    /// rotary inertia.
    pub(crate) fn node_inertia(&self) -> DMat3 {
        let half = 0.5 * self.material.density * self.rest_length;
        let (s, m, l) = (&self.section, self.mass(), self.rest_length);
        let outer = |v: DVec3| DMat3::from_cols(v * v.x, v * v.y, v * v.z);
        outer(self.axis) * (half * (s.i_y + s.i_z))
            + outer(self.y) * (m * l * l / 78.0 + half * s.i_y)
            + outer(self.z) * (m * l * l / 78.0 + half * s.i_z)
    }

    /// The six deformation measures for end positions `xa`, `xb` and end rotations `ra`,
    /// `rb` (as matrices).
    pub(crate) fn measures(&self, xa: DVec3, ra: DMat3, xb: DVec3, rb: DMat3) -> [f64; MEASURES] {
        let d = xb - xa;
        let l = d.length();
        let e = d / l;
        let (ta, ya, za) = (ra * self.axis, ra * self.y, ra * self.z);
        let (tb, yb, zb) = (rb * self.axis, rb * self.y, rb * self.z);
        let (pa, pb) = (e.cross(ta), e.cross(tb));
        let bend = [pa.dot(ya), pb.dot(yb), pa.dot(za), pb.dot(zb)];
        let raw = [
            l - self.rest_length + self.rest_length * bowing(&bend),
            0.5 * (ya.cross(yb) + za.cross(zb)).dot(e),
            bend[0],
            bend[1],
            bend[2],
            bend[3],
        ];
        std::array::from_fn(|m| raw[m] - self.plastic[m])
    }

    /// The measures and their gradients (derivations in the comments; checked against
    /// finite differences in the tests).
    pub(crate) fn kinematics(
        &self,
        xa: DVec3,
        ra: DMat3,
        xb: DVec3,
        rb: DMat3,
    ) -> ([f64; MEASURES], [Gradient; MEASURES]) {
        let d = xb - xa;
        let l = d.length();
        let e = d / l;
        // de = P·dd / l with P = I − e·eᵀ and dd = dxb − dxa.
        let chord = |v: DVec3| (v - e * e.dot(v)) / l;
        let (ta, ya, za) = (ra * self.axis, ra * self.y, ra * self.z);
        let (tb, yb, zb) = (rb * self.axis, rb * self.y, rb * self.z);
        let mut s = [0.0; MEASURES];
        let mut g = [[DVec3::ZERO; 4]; MEASURES];

        s[0] = l - self.rest_length;
        g[0] = [-e, DVec3::ZERO, e, DVec3::ZERO];

        // Twist: d((y_a × y_b)·e) = δθ_a·(y_a × (y_b × e)) + δθ_b·(y_b × (e × y_a))
        //                          + (y_a × y_b)·de, and the same for z.
        let w = ya.cross(yb) + za.cross(zb);
        s[1] = 0.5 * w.dot(e);
        let c = chord(w) * 0.5;
        g[1] = [
            -c,
            (ya.cross(yb.cross(e)) + za.cross(zb.cross(e))) * 0.5,
            c,
            (yb.cross(e.cross(ya)) + zb.cross(e.cross(za))) * 0.5,
        ];

        // Bending at node i about n: d((e × t)·n) = (t × n)·de
        //   + δθ_i·(t × (n × e) + n × (e × t)).
        for (m, (t, n, at_b)) in [
            (ta, ya, false),
            (tb, yb, true),
            (ta, za, false),
            (tb, zb, true),
        ]
        .into_iter()
        .enumerate()
        {
            s[m + 2] = e.cross(t).dot(n);
            let c = chord(t.cross(n));
            let turn = t.cross(n.cross(e)) + n.cross(e.cross(t));
            g[m + 2] = if at_b {
                [-c, DVec3::ZERO, c, turn]
            } else {
                [-c, turn, c, DVec3::ZERO]
            };
        }
        // The bowing of the stretch: L·Σ (2·θa² − θa·θb + 2·θb²)/30 over both planes.
        let bend = [s[2], s[3], s[4], s[5]];
        s[0] += self.rest_length * bowing(&bend);
        for plane in 0..2 {
            let (a, b) = (bend[2 * plane], bend[2 * plane + 1]);
            let (wa, wb) = (
                self.rest_length * (4.0 * a - b) / 30.0,
                self.rest_length * (4.0 * b - a) / 30.0,
            );
            // The bending gradients are [−c, turn, c, 0] at a and [−c, 0, c, turn] at b: their
            // blend slot by slot (the same sums, half the products).
            let (ga, gb) = (g[2 * plane + 2], g[2 * plane + 3]);
            let chord = ga[2] * wa + gb[2] * wb;
            g[0][0] -= chord;
            g[0][1] += ga[1] * wa;
            g[0][2] += chord;
            g[0][3] += gb[3] * wb;
        }
        for (s, p) in s.iter_mut().zip(&self.plastic) {
            *s -= p;
        }
        (s, g)
    }
}

/// How much longer than its chord the member's axis is, per unit of rest length, for end
/// rotations `bend` = [θa, θb] about y then about z (relative to the chord): ½·∫w′² of the
/// cubic deflection, Σ (2·θa² − θa·θb + 2·θb²)/30. In the stretch, the axial force times
/// it is the member's geometric stiffness (P-δ): a bent member under compression bends
/// further, and a member of a few elements buckles at Euler's load (one element of the
/// chord alone only had P-Δ between nodes: two elements buckled 24 % late, F5).
fn bowing(bend: &[f64; 4]) -> f64 {
    let plane = |a: f64, b: f64| (2.0 * a * a - a * b + 2.0 * b * b) / 30.0;
    plane(bend[0], bend[1]) + plane(bend[2], bend[3])
}
