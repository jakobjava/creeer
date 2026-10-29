//! The feet on a body they stand on (G2.10, ADR-0023): what their soles carry to it, and the
//! loads they put on it for a step.
//!
//! - **The legs' push** is their strut, solved in the step with the body (`world::strut`), its
//!   point on the body moving with the body's turn to first order. Where the body's face is
//!   steeper than the soles' friction holds, the foot slides and presses along its friction
//!   cone's edge: the strut turned that way ([`slides`]).
//! - **The walk** is the strut's damper along the ground, solved in the step too ([`walks`]):
//!   the soles grip the body they walk on, within their friction.
//! - **The balance's torque** passes to the body only through the patch where the soles meet it
//!   ([`carried`]): as the pressure's centre moves within it (about the axes in its plane: the
//!   normal force N with its centre r off the point under the hips is a moment N·r × n) and as
//!   the soles' friction turns it (about its normal: at most μ·N times the patch's mean
//!   distance from that centre, Coulomb's law over it as `twist.rs`'s over Hertz's circle). The
//!   patch is the face of the body's shape the legs' ray met, within the feet's reach of that
//!   point: half a foot's length, 0.152 of the stature (Winter, *Biomechanics and Motor Control
//!   of Human Movement*, 2009, table 4.1). The rest of the torque is not the feet's to give: a
//!   balance that pushes harder than its feet can carry pushes against nothing. A ball under
//!   the feet is met at a point and carries none; a face that holds the whole of the feet
//!   holds them as the ground does, the torque whole. (Clipped to the feet's reach there too,
//!   the balance on a crate was a quarter of the ground's 600 N·m, and players in *crowd*
//!   landing on crates fell over where they stood on the ground, G2.10.)
//!
//! Fixed ground under the feet keeps all three as they were (ADR-0023): its balance reaches
//! beyond the feet (600 N·m, `docs/future-improvements.md`), and its walk is a load, steady on
//! what does not yield. (Put whole at a 1.6 m spruce pole's centre, the balance's torque of a
//! person leaning a few milliradians on it, 7 N·m about the pole's own axis of 7.85·10⁻⁴
//! kg·m², spun it at 9,000 rad/s², G2.10.)

use creeer_math::det;
use creeer_math::glam::DVec3;
use creeer_physics::world::World;
use creeer_physics::world::ray::Hit;
use creeer_physics::world::shape::Shape;
use creeer_physics::world::strut::Walk;

/// How far from the point under the hips the feet press (m): half a foot's length of a
/// 1.75 m person (Winter: 0.152 of the stature).
const REACH: f64 = 0.5 * 0.152 * 1.75;
/// How far off a face's plane the ray's point may lie and be on it (m).
const ON_FACE: f64 = 1e-3;

/// The part of the balance's `torque` (N·m, on the character) that the feet carry to the body
/// `hit` found under them, pressing it with `pressing` (N) at friction `mu`; the whole of it
/// on fixed ground.
pub(super) fn carried(
    world: &World,
    hit: &Hit,
    (pressing, mu): (f64, f64),
    torque: DVec3,
) -> DVec3 {
    let Some((b, node)) = hit.body else {
        return torque;
    };
    let s = &world.bodies[b].structure;
    let (x, q) = (s.position[node], s.orientation[node]);
    let Some(shape) = (world.bodies[b].shapes.iter())
        .find(|(n, _)| *n == node)
        .map(|(_, shape)| shape)
    else {
        return torque;
    };
    let n = hit.normal;
    if pressing <= 0.0 {
        return DVec3::ZERO;
    }
    // The patch in the face's plane about the point under the hips, (u, v) along `axes`.
    let (u, v) = n.any_orthonormal_pair();
    let face = face_under(shape, q.conjugate() * (hit.point - x), q.conjugate() * n);
    let polygon: Vec<[f64; 2]> = face
        .iter()
        .map(|&p| {
            let d = x + q * p - hit.point;
            [d.dot(u), d.dot(v)]
        })
        .collect();
    let feet = octagon(REACH);
    // A face that holds the whole of the feet holds them as the ground does.
    if polygon.len() >= 3 && feet.iter().all(|&p| inside(&polygon, p)) {
        return torque;
    }
    let patch = clip(&polygon, &feet);
    if patch.len() < 3 {
        return DVec3::ZERO;
    }
    // The pressure's centre the torque asks for (r = n × τ / N), moved into the patch.
    let ask = n.cross(torque) / pressing;
    let centre = nearest(&patch, [ask.dot(u), ask.dot(v)]);
    let r = u * centre[0] + v * centre[1];
    let along = pressing * r.cross(n);
    let twist = torque.dot(n);
    let most = mu * pressing * mean_distance(&patch, centre);
    along + n * twist.clamp(-most, most)
}

/// A force (and torque) put on another body for one step: taken off by [`release`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reaction {
    pub body: usize,
    pub node: usize,
    pub force: DVec3,
    pub torque: DVec3,
    /// Whether the body was stepped as held (`Body::held`) before it was pushed.
    pub held: bool,
    /// The body's mass after the step (a jetpack's gas spent in it), if it changes.
    pub mass: Option<f64>,
}

/// Takes the step's reactions off the bodies they were put on, and the legs' struts off.
pub fn release(world: &mut World, reactions: &mut Vec<Reaction>) {
    world.clear_struts();
    for r in reactions.drain(..) {
        if let Some(body) = world.bodies.get_mut(r.body) {
            body.take_load(r.node, r.force);
            body.structure.torque[r.node] -= r.torque;
            body.held = r.held;
            if let Some(mass) = r.mass {
                body.solver.set_mass(&mut body.structure, r.node, mass);
            }
        }
    }
}

/// A foot sliding on the body `hit` found under it (its face steeper than the friction holds)
/// presses along its friction cone's edge, `foot` (N, the force on the character as the step
/// begins): the legs' strut `leg`, the character at `x` moving at `v`, turned that way and its
/// rest set to give that force there, so that the push falls as the body is pushed away. None
/// is left for the step's load (`Some(ZERO)`); on fixed ground, nothing turned (`None`). (Put on
/// as a load for the step, the part of the legs' push the friction does not hold, 800–980 N
/// where a foot met a pole's side 43–51° steep near its end, kicked the pole for the whole step
/// whatever it did: 400–560 N·m about its middle, G2.10.)
pub(super) fn slides(
    world: &mut World,
    (leg, hit): (usize, &Hit),
    (x, v): (DVec3, DVec3),
    foot: DVec3,
) -> Option<DVec3> {
    let (b, node) = hit.body?;
    let push = foot.length();
    let d = foot.try_normalize()?;
    let s = &world.bodies[b].structure;
    let point_v = s.velocity[node] + s.angular_velocity[node].cross(hit.point - s.position[node]);
    let strut = world.strut_mut(leg);
    // k·(rest − length) + c·(its rate of shortening) = the force, as the step begins.
    let (length, shortening) = ((x - hit.point).dot(d), -(v - point_v).dot(d));
    strut.direction = d;
    strut.rest = length + (push - strut.damping * shortening) / strut.stiffness;
    Some(DVec3::ZERO)
}

/// On the body `hit` found under the feet, the walk is the legs' strut's (`world::strut::Walk`):
/// a damper of `c` (N·s/m) on the character's speed over the body's point under it, along the
/// ground (across `up`), towards `target` (m/s: the speed asked for, as near as the walk's
/// strength reaches from the speed as the step begins). Solved in the step, the soles grip the
/// body as they walk on it: what rolls under them carries the person. Within the soles'
/// friction `mu` on the body's face there, tilted φ from the level, under the legs' push `legs`
/// (N): the push along the vertical already holds legs·sin φ of friction uphill on the face, so
/// the walk's may add μ·legs·cos φ − legs·sin φ uphill and μ·legs·cos φ + legs·sin φ downhill,
/// and μ·legs·cos φ across: on a body slipping under the soles the friction is then μ·N against
/// the slip, as Coulomb's. None is left for the step's load (`Some(ZERO)`); on fixed ground,
/// nothing set (`None`).
///
/// (With the walk put on as a load from the speeds as the step began, a pole's top, which rolls
/// as a body of 0.94 kg, was pushed by a damper of 420 N·s/m that 60 Hz holds steady only on
/// 3.5 kg, and rolled at up to 76 rad/s, back and forth tick by tick. Taken through the pole's
/// mobility at the step's start, the soles held nothing within the step, and a person's weight
/// pressed off the pole's axis rolled it out at 12,000 rad/s² as if nothing held its top, where
/// the person it carries makes it about 110. With its grip μ·cos φ − sin φ both ways, a pole
/// slipping under the soles was held by μ·cos φ − 2·sin φ only, and one spun under a standing
/// player at 30–40 rad/s for 20 ticks, G2.10.)
pub(super) fn walks(
    world: &mut World,
    (leg, hit): (usize, &Hit),
    (up, target): (DVec3, DVec3),
    (c, legs, mu): (f64, f64, f64),
) -> Option<DVec3> {
    hit.body?;
    let n = hit.normal;
    let level = n.dot(up).clamp(0.0, 1.0);
    let tilt = (1.0 - level * level).sqrt();
    let (hold, sideways) = (mu * legs * level, legs * tilt);
    // Uphill on the face, along the ground; on a level face, any way.
    let uphill = (up * n.dot(up) - n)
        .try_normalize()
        .unwrap_or_else(|| up.any_orthonormal_vector());
    let axes = [uphill, up.cross(uphill)];
    let up_down = [-(hold + sideways).max(0.0), (hold - sideways).max(0.0)];
    // A walk asked for wakes the character and what it walks on (a load changed would; this
    // is no load).
    if target != DVec3::ZERO {
        let a = world.struts()[leg].a.0;
        world.bodies[a].wake();
        if let Some((b, _)) = hit.body {
            world.bodies[b].wake();
        }
    }
    world.strut_mut(leg).walk = Some(Walk {
        axes,
        speed: axes.map(|a| target.dot(a)),
        damping: c,
        most: [up_down, [-hold, hold]],
    });
    Some(DVec3::ZERO)
}

/// The corners of the face of `shape` that a point `at` with normal `normal` lies on (both in
/// the shape's frame), in order around it; none for a ball.
fn face_under(shape: &Shape, at: DVec3, normal: DVec3) -> Vec<DVec3> {
    match shape {
        Shape::Sphere { .. } => Vec::new(),
        Shape::Box { half } => {
            let k = normal.abs().max_position();
            let side = normal[k].signum();
            let (i, j) = ((k + 1) % 3, (k + 2) % 3);
            let corner = |a: f64, c: f64| {
                let mut p = DVec3::ZERO;
                p[k] = side * half[k];
                p[i] = a * half[i];
                p[j] = c * half[j];
                p
            };
            vec![
                corner(-1.0, -1.0),
                corner(1.0, -1.0),
                corner(1.0, 1.0),
                corner(-1.0, 1.0),
            ]
        }
        Shape::Hull(hull) => {
            let on = |f: &&creeer_physics::world::hull::Face| {
                (f.normal.dot(at) - f.offset).abs() < ON_FACE
            };
            let faces = hull.faces();
            let best = (faces.iter().filter(on))
                .max_by(|a, b| a.normal.dot(normal).total_cmp(&b.normal.dot(normal)))
                .or_else(|| {
                    (faces.iter())
                        .max_by(|a, b| a.normal.dot(normal).total_cmp(&b.normal.dot(normal)))
                });
            best.map_or_else(Vec::new, |f| {
                f.ring.iter().map(|&i| hull.vertices()[i]).collect()
            })
        }
    }
}

/// A regular octagon of inner radius `r` about the origin, counter-clockwise.
pub(super) fn octagon(r: f64) -> Vec<[f64; 2]> {
    let outer = r / det::cos(std::f64::consts::PI / 8.0);
    (0..8)
        .map(|k| {
            let (s, c) = det::sin_cos((f64::from(k) + 0.5) * std::f64::consts::FRAC_PI_4);
            [outer * c, outer * s]
        })
        .collect()
}

fn cross(o: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// Convex `polygon` clipped by convex `by` (Sutherland–Hodgman), both counter-clockwise or
/// both clockwise; `polygon`'s winding is kept.
pub(super) fn clip(polygon: &[[f64; 2]], by: &[[f64; 2]]) -> Vec<[f64; 2]> {
    // A face's corners seen against its normal from (u, v) may wind either way: the clipper's
    // edges are taken in the polygon's.
    let area: f64 = (0..polygon.len())
        .map(|k| cross([0.0; 2], polygon[k], polygon[(k + 1) % polygon.len()]))
        .sum();
    let mut by = by.to_vec();
    if area < 0.0 {
        by.reverse();
    }
    let sign = area.signum();
    let mut out = polygon.to_vec();
    for k in 0..by.len() {
        let (a, b) = (by[k], by[(k + 1) % by.len()]);
        let inside = |p: [f64; 2]| sign * cross(a, b, p) >= 0.0;
        let input = std::mem::take(&mut out);
        for m in 0..input.len() {
            let (p, q) = (input[m], input[(m + 1) % input.len()]);
            if inside(p) {
                out.push(p);
            }
            if inside(p) != inside(q) {
                let (dp, dq) = (cross(a, b, p), cross(a, b, q));
                let t = dp / (dp - dq);
                out.push([p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t]);
            }
        }
        if out.is_empty() {
            break;
        }
    }
    out
}

/// Whether `p` lies within convex `polygon` (either winding).
fn inside(polygon: &[[f64; 2]], p: [f64; 2]) -> bool {
    let n = polygon.len();
    let area: f64 = (0..n)
        .map(|k| cross([0.0; 2], polygon[k], polygon[(k + 1) % n]))
        .sum();
    (0..n).all(|k| area.signum() * cross(polygon[k], polygon[(k + 1) % n], p) >= 0.0)
}

/// The point of convex `polygon` nearest `p`: `p` itself if inside.
pub(super) fn nearest(polygon: &[[f64; 2]], p: [f64; 2]) -> [f64; 2] {
    let n = polygon.len();
    if inside(polygon, p) {
        return p;
    }
    let mut best = (f64::INFINITY, p);
    for k in 0..n {
        let (a, b) = (polygon[k], polygon[(k + 1) % n]);
        let (ab, ap) = ([b[0] - a[0], b[1] - a[1]], [p[0] - a[0], p[1] - a[1]]);
        let length = ab[0] * ab[0] + ab[1] * ab[1];
        let t = if length > 0.0 {
            ((ab[0] * ap[0] + ab[1] * ap[1]) / length).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let q = [a[0] + ab[0] * t, a[1] + ab[1] * t];
        let d = (q[0] - p[0]) * (q[0] - p[0]) + (q[1] - p[1]) * (q[1] - p[1]);
        if d < best.0 {
            best = (d, q);
        }
    }
    best.1
}

/// The mean distance of convex `polygon`'s area from `o` (inside or on it): ∫|r|·dA / A, in
/// fans from `o`, each edge's ∫∫r²·dr·dθ = d³/6·[g(t)] with g(t) = t·√(1+t²) + asinh t, d the
/// edge's distance from `o` and t the distance along it from the nearest point over d.
pub(super) fn mean_distance(polygon: &[[f64; 2]], o: [f64; 2]) -> f64 {
    // (asinh t = ln(t + √(1+t²)), odd: taken for |t| to keep its digits.)
    let g = |t: f64| {
        let root = (1.0 + t * t).sqrt();
        t * root + t.signum() * det::ln(t.abs() + root)
    };
    let n = polygon.len();
    let (mut moment, mut area) = (0.0, 0.0);
    for k in 0..n {
        let (a, b) = (polygon[k], polygon[(k + 1) % n]);
        let twice = cross(o, a, b);
        area += 0.5 * twice;
        let edge = [b[0] - a[0], b[1] - a[1]];
        let length = (edge[0] * edge[0] + edge[1] * edge[1]).sqrt();
        if length == 0.0 || twice == 0.0 {
            continue;
        }
        let d = twice / length;
        let along = |p: [f64; 2]| ((p[0] - o[0]) * edge[0] + (p[1] - o[1]) * edge[1]) / length;
        let (sa, sb) = (along(a), along(b));
        moment += d.abs() * d * d / 6.0 * (g(sb / d.abs()) - g(sa / d.abs())) * d.signum();
    }
    if area.abs() < 1e-12 {
        0.0
    } else {
        moment / area
    }
}
