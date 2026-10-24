//! Asking the world what holds a body, and pushing a body from outside the step.

use super::World;
use super::contact::Side;
use super::ground;
use super::narrow::{pose, shapes, with_static};
use super::shape::RawContact;
use crate::structure::NodeId;
use creeer_math::glam::{DMat3, DVec3};

/// A contact that held a body in the last step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Support {
    /// Its normal, towards the body.
    pub normal: DVec3,
    /// Its normal force (N; the one friction used, at the start of the step).
    pub force: f64,
    /// Where it touches (world, at the start of the step).
    pub point: DVec3,
    /// The body and node on the other side, or none for fixed geometry and the ground.
    pub other: Option<(usize, NodeId)>,
}

impl World {
    /// What held body `index` in the last step: for telling whether a body stands on
    /// something, on what slope, and what it pushes against when it pushes off.
    pub fn supports(&self, index: usize) -> Vec<Support> {
        let mut out = Vec::new();
        for (bodies, contacts) in &self.groups {
            let Some(member) = bodies.iter().position(|&b| b == index) else {
                continue;
            };
            let on = |side: Side| matches!(side, Side::Node { member: m, .. } if m == member);
            for c in &contacts.list {
                let (normal, other) = if on(c.a) {
                    (c.normal, c.b)
                } else if on(c.b) {
                    (-c.normal, c.a)
                } else {
                    continue;
                };
                let (point, other) = match other {
                    Side::Fixed(p) => (p, None),
                    Side::Node {
                        member,
                        node,
                        local,
                    } => {
                        let body = bodies[member];
                        let s = &self.bodies[body].structure;
                        (
                            s.position[node] + s.orientation[node] * local,
                            Some((body, node)),
                        )
                    }
                };
                out.push(Support {
                    normal,
                    force: c.normal_force(),
                    point,
                    other,
                });
            }
        }
        out
    }

    /// Gives node `node` of body `index` the impulse `impulse` (N·s) at `point` (world): its
    /// velocity and spin change at once (along its free axes), the solver's history is
    /// forgotten (it moves on from the new state), and it wakes.
    pub fn apply_impulse(&mut self, index: usize, node: NodeId, impulse: DVec3, point: DVec3) {
        let body = &mut self.bodies[index];
        body.wake();
        let s = &mut body.structure;
        let mass = s.mass[node];
        if mass > 0.0 {
            s.velocity[node] += impulse / mass * s.free_translation[node];
        }
        let r = s.orientation[node];
        let inertia = DMat3::from_quat(r) * s.inertia[node] * DMat3::from_quat(r).transpose();
        if inertia.determinant() > 0.0 {
            let torque = (point - s.position[node]).cross(impulse);
            s.angular_velocity[node] += inertia.inverse() * torque * s.free_rotation[node];
        }
        body.solver.reset_history(&body.structure);
    }

    /// The mass node `node` of body `index` has against an impulse along unit `direction` at
    /// `point` (world; kg): 1/m = 1/M + (r × d)·I⁻¹(r × d), its free axes alone (a fixed node,
    /// unmoved by any impulse, infinite).
    pub fn effective_mass(
        &self,
        index: usize,
        node: NodeId,
        point: DVec3,
        direction: DVec3,
    ) -> f64 {
        let s = &self.bodies[index].structure;
        let mass = s.mass[node];
        let moved = if mass > 0.0 {
            (direction * s.free_translation[node]).dot(direction) / mass
        } else {
            0.0
        };
        let r = DMat3::from_quat(s.orientation[node]);
        let inertia = r * s.inertia[node] * r.transpose();
        let turned = if inertia.determinant() > 0.0 {
            let arm = (point - s.position[node]).cross(direction);
            arm.dot(inertia.inverse() * arm * s.free_rotation[node])
        } else {
            0.0
        };
        1.0 / (moved + turned)
    }
}

/// What a body touches now (found from where everything is, not from the last step).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Touch {
    /// The normal, towards the body.
    pub normal: DVec3,
    /// The gap (m; negative where they overlap).
    pub gap: f64,
    /// Where on the other side it touches (world).
    pub point: DVec3,
    /// The body and node on the other side, or none for fixed geometry and the ground.
    pub other: Option<(usize, NodeId)>,
}

impl World {
    /// What body `index` touches or nearly touches (a gap below `margin`) as things are
    /// now: the ground, fixed geometry, other bodies. For telling whether something stands
    /// on the ground in the state it was just put in (a prediction set back to a server's
    /// state), where the last step's contacts would tell of another state.
    pub fn touching(&self, index: usize, margin: f64) -> Vec<Touch> {
        let gap = |c: &RawContact| {
            (c.point_a - c.point_b).dot(c.normal) - c.radius_a - c.radius_b - c.inset
        };
        let body = &self.bodies[index];
        let s = &body.structure;
        let mut out = Vec::new();
        // A face's patch (four corners in a row, the last with index 3) touches where it
        // presses: its pressing polygon's middle, on B, at the polygon's least gap. Its corners
        // are its rectangle's, which may lie far beyond (a pole's face lying across another
        // pole: a corner 0.8 m from where they cross).
        let mut corners: Vec<RawContact> = Vec::new();
        let mut keep = |c: RawContact, other| {
            let c = match &c.patch {
                None => c,
                Some(v) => {
                    corners.push(c.clone());
                    if v.index < 3 || corners.len() < 4 {
                        return;
                    }
                    let middle = pressed_middle(&corners[corners.len() - 4..]);
                    corners.clear();
                    middle
                }
            };
            let g = gap(&c);
            if g < margin {
                out.push(Touch {
                    normal: c.normal,
                    gap: g,
                    point: c.point_b,
                    other,
                });
            }
        };
        for (node, shape) in &body.shapes {
            let at = pose(s.position[*node], s.orientation[*node]);
            if let Some(ground) = &self.ground {
                for c in ground::contacts(ground.as_ref(), at, shape, margin) {
                    keep(c, None);
                }
            }
            for fixed in &self.statics {
                for c in with_static((at, shape), fixed, margin) {
                    keep(c, None);
                }
            }
            for (j, other) in self.bodies.iter().enumerate().filter(|(j, _)| *j != index) {
                let o = &other.structure;
                for (on, os) in &other.shapes {
                    let reach = shape.bounding_radius() + os.bounding_radius() + margin;
                    if (o.position[*on] - at.position).length() > reach {
                        continue;
                    }
                    for c in shapes(
                        (at, shape),
                        (pose(o.position[*on], o.orientation[*on]), os),
                        margin,
                    ) {
                        keep(c, Some((j, *on)));
                    }
                }
            }
        }
        out
    }
}

/// A patch's four corners as one contact: at its pressing polygon's middle (on both sides),
/// with the polygon's least gap.
fn pressed_middle(four: &[RawContact]) -> RawContact {
    let at = |points: [DVec3; 4], [s, t]: [f64; 2]| {
        points[0] * ((1.0 - s) * (1.0 - t))
            + points[1] * (s * (1.0 - t))
            + points[2] * (s * t)
            + points[3] * ((1.0 - s) * t)
    };
    let a: [DVec3; 4] = std::array::from_fn(|k| four[k].point_a);
    let b: [DVec3; 4] = std::array::from_fn(|k| four[k].point_b);
    let normal = four[0].normal;
    let domain = four[0].patch.as_ref().map(|v| v.domain.points().to_vec());
    let points = domain.unwrap_or_default();
    #[allow(clippy::cast_precision_loss)] // a few corners
    let n = points.len().max(1) as f64;
    let middle = points
        .iter()
        .fold([0.0, 0.0], |m, p| [m[0] + p[0] / n, m[1] + p[1] / n]);
    let least = points
        .iter()
        .map(|&p| normal.dot(at(a, p) - at(b, p)))
        .fold(f64::INFINITY, f64::min);
    let on_b = at(b, middle);
    RawContact {
        point_a: on_b + normal * least,
        point_b: on_b,
        patch: None,
        ..four[0].clone()
    }
}

#[cfg(test)]
#[path = "query_tests.rs"]
mod tests;
