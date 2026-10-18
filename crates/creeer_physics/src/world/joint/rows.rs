//! A joint measured at a state: each of its scalars (a component of the gap between its two
//! points, of the misalignment of a hinge's axes, a limit's reach, a component of an arm's
//! gap) with how it changes with each side's motion: its row, the derivative by the side's
//! translation and its (world-frame) turn, exact. Energies of these scalars then have exact
//! gradients (Σ coefficient·row) and Gauss–Newton blocks (Σ weight·row·rowᵀ).

use super::{ARM, Joint, Kind};
use crate::structure::group::MemberState;
use creeer_math::glam::{DMat3, DQuat, DVec3};

/// d(scalar)/d(translation, turn) of one side.
pub(crate) type Row = [f64; 6];

/// One scalar and its rows on both sides (B's unused for the world).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Scalar {
    pub value: f64,
    pub a: Row,
    pub b: Row,
}

const AXES: [DVec3; 3] = [DVec3::X, DVec3::Y, DVec3::Z];

fn row(translation: DVec3, turn: DVec3) -> Row {
    [
        translation.x,
        translation.y,
        translation.z,
        turn.x,
        turn.y,
        turn.z,
    ]
}

/// A joint measured at a state.
#[derive(Clone, Debug, Default)]
pub(crate) struct At {
    /// The gap between its two points, by world axis (m).
    pub point: [Scalar; 3],
    /// A hinge's axes' misalignment, A's axis × B's (by world axis).
    pub axis: Option<[Scalar; 3]>,
    /// Its limits' reach (≤ 0 within them; rad, about), as `Multipliers::limits`.
    pub limits: [Option<Scalar>; 3],
    /// Each arm's gap (A's point less B's, by world axis; m): the two sides' relative turn
    /// (for a joint with damping).
    pub arms: Option<[[Scalar; 3]; 3]>,
    /// Each of the drive's arms: where A wants B's point, less B's point.
    pub drive: Option<[[Scalar; 3]; 3]>,
    /// Each of the friction's arms: where its anchor holds B's point, less B's point.
    pub stick: Option<[[Scalar; 3]; 3]>,
}

/// A side's pose: its node's position and orientation (the world's: at the origin, unturned).
fn pose(side: Option<(usize, usize)>, states: &[MemberState<'_>]) -> (DVec3, DQuat) {
    match side {
        Some((m, n)) => (states[m].position[n], states[m].orientation[n]),
        None => (DVec3::ZERO, DQuat::IDENTITY),
    }
}

/// The gap between A's point and B's, each given by its node's position and its arm from
/// it (world), by world axis: with its rows if `rows`.
fn gap(xa: DVec3, arm_a: DVec3, xb: DVec3, arm_b: DVec3, rows: bool) -> [Scalar; 3] {
    let d = (xa + arm_a) - (xb + arm_b);
    std::array::from_fn(|j| {
        let e = AXES[j];
        if rows {
            Scalar {
                value: d[j],
                a: row(e, arm_a.cross(e)),
                b: row(-e, -arm_b.cross(e)),
            }
        } else {
            Scalar {
                value: d[j],
                ..Scalar::default()
            }
        }
    })
}

/// The angle of `r_b` from `r_a` about `axis` (A's), with its derivatives by A's and B's
/// turns: atan2(axis·(r_a × r_b), r_a·r_b). `None` where it has no meaning (`r_b` along the
/// axis).
fn angle_about(axis: DVec3, r_a: DVec3, r_b: DVec3) -> Option<(f64, DVec3, DVec3)> {
    let s = axis.dot(r_a.cross(r_b));
    let c = r_a.dot(r_b);
    let norm = s * s + c * c;
    if norm < 1e-6 {
        return None;
    }
    // A's turn moves its axis and its reference; B's turn moves B's reference.
    let ds_a = axis.cross(r_a.cross(r_b)) + r_a.cross(r_b.cross(axis));
    let ds_b = r_b.cross(axis.cross(r_a));
    let (dc_a, dc_b) = (r_a.cross(r_b), r_b.cross(r_a));
    Some((
        creeer_math::det::atan2(s, c),
        (ds_a * c - dc_a * s) / norm,
        (ds_b * c - dc_b * s) / norm,
    ))
}

/// A limit's reach `g` (≤ 0 within it) with its turns' derivatives.
fn limit(g: f64, turn_a: DVec3, turn_b: DVec3) -> Scalar {
    Scalar {
        value: g,
        a: row(DVec3::ZERO, turn_a),
        b: row(DVec3::ZERO, turn_b),
    }
}

/// The two stops of an angle (low, high) at `angle` within `range`.
fn stops((lo, hi): (f64, f64), (angle, da, db): (f64, DVec3, DVec3)) -> [Scalar; 2] {
    [limit(lo - angle, -da, -db), limit(angle - hi, da, db)]
}

impl Joint {
    /// B's frame relative to A's at `states`.
    pub(crate) fn relative(&self, states: &[MemberState<'_>]) -> DQuat {
        let (pa, pb) = (pose(Some(self.a), states), pose(self.b, states));
        ((pa.1 * self.frame_a).conjugate() * (pb.1 * self.frame_b)).normalize()
    }

    /// The joint measured at `states` (its sides as members of the group), with every
    /// scalar's rows.
    pub(crate) fn at(&self, states: &[MemberState<'_>]) -> At {
        self.measured(states, true)
    }

    /// The joint measured at `states`: its scalars' rows only if `rows` (an energy alone
    /// needs the values: a line search's trials).
    pub(crate) fn measured(&self, states: &[MemberState<'_>], rows: bool) -> At {
        let ((xa, qa), (xb, qb)) = (pose(Some(self.a), states), pose(self.b, states));
        // Each frame's axes in the world, and the joint's centre on each side.
        let (fa, fb) = (
            DMat3::from_quat(qa * self.frame_a),
            DMat3::from_quat(qb * self.frame_b),
        );
        let (centre_a, centre_b) = (qa * self.at_a, qb * self.at_b);
        let (axis_a, axis_b) = (fa.x_axis, fb.x_axis);
        let (ref_a, ref_b) = (fa.y_axis, fb.y_axis);
        let arms = |a: DMat3| [a.x_axis * ARM, a.y_axis * ARM, a.z_axis * ARM];
        let arms_b = arms(fb);
        // Arms of A turned by `turn` relative to its frame (a drive's target, friction's
        // anchor), each against B's.
        let turned = |turn: DQuat| {
            let t = fa * DMat3::from_quat(turn);
            let arms_a = arms(t);
            std::array::from_fn(|i| gap(xa, centre_a + arms_a[i], xb, centre_b + arms_b[i], rows))
        };
        let mut at = At {
            point: gap(xa, centre_a, xb, centre_b, rows),
            ..At::default()
        };
        if self.damping > 0.0 {
            let arms_a = arms(fa);
            at.arms = Some(std::array::from_fn(|i| {
                gap(xa, centre_a + arms_a[i], xb, centre_b + arms_b[i], rows)
            }));
        }
        match self.kind {
            Kind::Hinge { range } => {
                let c = axis_a.cross(axis_b);
                at.axis = Some(std::array::from_fn(|j| {
                    let e = AXES[j];
                    Scalar {
                        value: c[j],
                        a: if rows {
                            row(DVec3::ZERO, axis_a.cross(axis_b.cross(e)))
                        } else {
                            Row::default()
                        },
                        b: if rows {
                            row(DVec3::ZERO, axis_b.cross(e.cross(axis_a)))
                        } else {
                            Row::default()
                        },
                    }
                }));
                if let Some(range) = range
                    && let Some(angle) = angle_about(axis_a, ref_a, ref_b)
                {
                    let [lo, hi] = stops(range, angle);
                    at.limits[0] = Some(lo);
                    at.limits[1] = Some(hi);
                }
            }
            Kind::Ball { cone, twist } => {
                if let Some(cone) = cone {
                    // cos(cone) − t_a·t_b over sin(cone): about the angle beyond the cone.
                    let scale = 1.0 / creeer_math::det::sin(cone).max(0.1);
                    let t = axis_a.cross(axis_b) * scale;
                    at.limits[0] = Some(limit(
                        (creeer_math::det::cos(cone) - axis_a.dot(axis_b)) * scale,
                        -t,
                        t,
                    ));
                }
                if let Some(range) = twist
                    && let Some(angle) = angle_about(axis_a, ref_a, ref_b)
                {
                    let [lo, hi] = stops(range, angle);
                    at.limits[1] = Some(lo);
                    at.limits[2] = Some(hi);
                }
            }
        }
        if self.friction > 0.0
            && let Some(anchor) = self.anchor
        {
            at.stick = Some(turned(anchor));
        }
        if let Some(drive) = self.drive {
            at.drive = Some(turned(drive.target));
        }
        at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every row against the scalar's change under a small motion of each side.
    #[test]
    fn rows_are_the_scalars_derivatives() {
        let joint = Joint {
            a: (0, 0),
            b: Some((1, 0)),
            at_a: DVec3::new(0.3, -0.1, 0.2),
            at_b: DVec3::new(-0.2, 0.4, 0.1),
            frame_a: DQuat::from_rotation_y(0.4) * DQuat::from_rotation_z(0.2),
            frame_b: DQuat::from_rotation_x(-0.3),
            kind: Kind::Hinge {
                range: Some((-0.5, 0.7)),
            },
            damping: 0.0,
            friction: 0.0,
            drive: Some(super::super::Drive {
                target: DQuat::from_rotation_x(0.3),
                stiffness: 1.0,
                strength: 1.0,
            }),
            collide: false,
            anchor: None,
            multipliers: super::super::Multipliers::default(),
        };
        let ball = Joint {
            kind: Kind::Ball {
                cone: Some(0.8),
                twist: Some((-0.4, 0.4)),
            },
            ..joint.clone()
        };
        let x = [DVec3::new(0.1, 0.2, 0.3), DVec3::new(0.5, -0.1, 0.0)];
        let q = [
            DQuat::from_rotation_x(0.3) * DQuat::from_rotation_y(-0.2),
            DQuat::from_rotation_z(0.5) * DQuat::from_rotation_x(0.1),
        ];
        let flat = |at: &At| {
            let mut v: Vec<Scalar> = at.point.to_vec();
            v.extend(at.axis.iter().flatten());
            v.extend(at.limits.iter().flatten());
            v.extend(at.arms.iter().flatten().flatten());
            v.extend(at.drive.iter().flatten().flatten());
            v
        };
        for joint in [joint, ball] {
            let states = |x: &[DVec3; 2], q: &[DQuat; 2]| {
                let at = joint.at(&[
                    MemberState {
                        position: &x[..1],
                        orientation: &q[..1],
                    },
                    MemberState {
                        position: &x[1..],
                        orientation: &q[1..],
                    },
                ]);
                flat(&at)
            };
            let base = states(&x, &q);
            let h = 1e-6;
            for side in 0..2 {
                for k in 0..6 {
                    let (mut x2, mut q2) = (x, q);
                    let e = AXES[k % 3] * h;
                    if k < 3 {
                        x2[side] += e;
                    } else {
                        q2[side] = (DQuat::from_scaled_axis(e) * q2[side]).normalize();
                    }
                    let moved = states(&x2, &q2);
                    for (b, m) in base.iter().zip(&moved) {
                        let numeric = (m.value - b.value) / h;
                        let exact = if side == 0 { b.a[k] } else { b.b[k] };
                        assert!(
                            (numeric - exact).abs() < 1e-5 * (1.0 + exact.abs()),
                            "side {side}, motion {k}: {numeric} against {exact}"
                        );
                    }
                }
            }
        }
    }
}
