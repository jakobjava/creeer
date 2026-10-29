//! The ground's surface found from a point (`world/ground.rs`): its distance estimated, the nearest
//! surface point and the normal there, and the point closest to a round body's centre. The
//! terrain's density is not a distance: where edits' rounds meet, its gradient is a weighted
//! mean of gradients pointing apart, and it does not fall along its steps as a distance does
//! (G2.10, ADR-0005).

use super::{BULGE, Ground};
use creeer_math::glam::DVec3;

/// Distance (m) from `p` to the ground's surface (negative inside), to first order, never
/// farther than the field guarantees ([`towards`]).
pub(in crate::world) fn estimate(ground: &dyn Ground, p: DVec3) -> Option<f64> {
    let (d, g) = ground.density_gradient(p);
    let length = g.length();
    (length > 1e-9).then(|| towards(d, length))
}

/// The step towards the surface from a point of density `d` whose gradient is `slope` long:
/// Newton's, d/|∇d|, where the field is as steep as a distance, and else the most the field
/// guarantees clear, d (Hart's sphere tracing, 1996). Every part of the field rises at least a
/// metre a metre: the generated ground's density is the height above it along the vertical
/// (its gradient 1 along it, more on a slope), a brush's its distance. A gradient less steep
/// is parts' slopes cancelling in an edit's round (a weighted mean of their gradients,
/// `creeer_terrain::edit_gradient`), where the field does not fall as its gradient says:
/// where a slot dug beside a hole met it, the gradient was 0.125 long 7 cm from the walls,
/// d/|∇d| reached 0.8 m along the slot to a point still in the air, and its tangent plane put
/// a face's point 9 cm into the ground (G2.10, ADR-0005).
fn towards(d: f64, slope: f64) -> f64 {
    d / slope.max(1.0)
}

/// Most steps of a projection where the field is shallower than a distance ([`towards`]), and
/// most moves sideways towards the nearest point there.
const MOST_STEPS: usize = 32;
const MOST_SLIDES: usize = 16;
/// A projection there ends once its step, or its move sideways, is shorter than this (m).
const PROJECTED: f64 = 1e-5;

/// The surface point nearest `p` and the outward normal there: two projection steps where the
/// field falls as a distance does ([`onto`]). Where it does not, as many as reach the surface,
/// and then alternately sideways to the foot of `p` on the tangent plane and back onto the
/// surface (Hartmann's foot point, 1999) until the normal passes through `p`: there the steps
/// follow the gradient as it turns, and the surface point they reach is not the nearest (where
/// a slot dug beside a hole met it, points 11 cm out in the air came to feet 14 cm away and
/// 8 cm to the side, and a face's curvature taken about those feet put them up to 8 cm into the
/// ground, G2.10, ADR-0005). Where they reach no surface, `p` is taken as clear of the ground,
/// or in it, by as much as the field guarantees ([`towards`]), along its gradient: on a ridge
/// of the field its gradient lies along the ridge and never turns off it (on the mid-plane of
/// a slot open to the sky the steps went up and down above its floor for good, every point
/// there in the air, the walls 14 cm to either side).
pub(in crate::world) fn project(ground: &dyn Ground, p: DVec3) -> Option<(DVec3, DVec3)> {
    let first = onto(ground, p, false)?;
    let (step, n) = first.start;
    if !first.reached {
        return Some((p - n * step, n));
    }
    let (mut q, mut normal) = (first.at, first.normal);
    if first.crooked {
        for _ in 0..MOST_SLIDES {
            let r = p - q;
            let sideways = r - normal * normal.dot(r);
            if sideways.length() < PROJECTED {
                break;
            }
            match onto(ground, q + sideways, true) {
                Some(on) if on.reached => (q, normal) = (on.at, on.normal),
                _ => break,
            }
        }
    }
    Some((q, normal))
}

/// A path onto the surface ([`onto`]).
struct Onto {
    /// The point reached and the normal there.
    at: DVec3,
    normal: DVec3,
    /// Whether the field fell otherwise than a distance on the way.
    crooked: bool,
    /// Whether it reached the surface.
    reached: bool,
    /// The first step's length and direction (against the gradient at the start).
    start: (f64, DVec3),
}

/// From `p` onto the surface. Two steps where the field falls as a distance does: steeper than
/// a distance ([`towards`]), the second step much shorter than the first (a tenth, and a
/// [`BULGE`]), as Newton's steps shorten. Else, or if `converge`, until the step is shorter
/// than [`PROJECTED`]; not reached if the density has not halved over four steps, or after
/// [`MOST_STEPS`]. (Above a slot dug into level ground, 1 cm off its axis, the gradient points
/// up to a wall that is not there, the slot being open: the first step went 14 cm up into the
/// air above the ground, the second 11 cm down to the level of the ground over the slot, and a
/// point 14 cm from the walls was taken as touching the ground, G2.10.)
fn onto(ground: &dyn Ground, p: DVec3, converge: bool) -> Option<Onto> {
    let mut path = Onto {
        at: p,
        normal: DVec3::Y,
        crooked: false,
        reached: false,
        start: (0.0, DVec3::Y),
    };
    let mut densities = [f64::INFINITY; 4];
    for k in 0..MOST_STEPS {
        let (d, g) = ground.density_gradient(path.at);
        let length = g.length();
        if length <= 1e-9 {
            return None;
        }
        path.normal = g / length;
        let step = towards(d, length);
        path.at -= path.normal * step;
        path.crooked |= length < 1.0;
        if k == 0 {
            path.start = (step, path.normal);
        } else {
            let newton =
                !(path.crooked || converge) && step.abs() < 0.1 * path.start.0.abs() + BULGE;
            if newton || step.abs() < PROJECTED {
                path.reached = true;
                return Some(path);
            }
            if d.abs() > 0.5 * densities[k % 4] {
                return Some(path);
            }
            path.crooked = true;
        }
        densities[k % 4] = d.abs();
    }
    Some(path)
}

/// The point of the ground closest to `c` and the outward normal there, starting the search
/// from the surface point `start`: alternately onto the surface along the gradient and
/// sideways to the foot of `c`. A round contact on the ground is measured with it at every
/// evaluation of its energy: the distance to the closest point is a smooth function of `c`
/// whose gradient is the normal there, so energy and force agree, and no geometry is frozen
/// for the step (a ball rolling at 3 m/s over 5 mm bumps saw its normal force swing from
/// 108 to 1,125 N against 284 N with the ground frozen at the start of each 60 Hz step,
/// measured in F5).
pub fn closest(ground: &dyn Ground, c: DVec3, start: DVec3) -> Option<(DVec3, DVec3)> {
    let mut q = start;
    let mut normal = DVec3::Y;
    let mut shallow = false;
    for k in 0..CLOSEST_STEPS + MOST_STEPS {
        let (d, g) = ground.density_gradient(q);
        let length = g.length();
        if length <= 1e-9 {
            return None;
        }
        normal = g / length;
        let step = towards(d, length);
        q -= normal * step;
        shallow |= length < 1.0;
        if k + 1 < CLOSEST_STEPS {
            let sideways = c - q;
            q += sideways - normal * normal.dot(sideways);
        } else if !shallow || step.abs() < PROJECTED {
            break;
        }
    }
    Some((q, normal))
}

/// Projection steps of [`closest`].
const CLOSEST_STEPS: usize = 4;
