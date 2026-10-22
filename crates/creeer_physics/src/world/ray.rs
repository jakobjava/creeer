//! Rays through the world (G1.2): what a straight line from a point first meets, and where:
//! the ground, fixed geometry, or a body's shape. For a character's legs finding the ground
//! under them, a tool finding what it strikes, a player's eye finding what it looks at.

use super::World;
use super::shape::{Shape, StaticShape};
use crate::structure::NodeId;
use creeer_math::glam::{DQuat, DVec3};

/// What a ray met.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// How far along the ray (m).
    pub distance: f64,
    /// Where (world).
    pub point: DVec3,
    /// The surface's outward normal there.
    pub normal: DVec3,
    /// The body and node it belongs to, or none for the ground and fixed geometry.
    pub body: Option<(usize, NodeId)>,
}

/// How close to the ground's surface a ray's march stops (m).
const SURFACE: f64 = 1e-5;
/// The longest step a march over the ground takes (m): the density is a distance only
/// to first order.
const MARCH: f64 = 1.0;

/// Where a ray (origin, unit direction) enters the slab |n·x − c| ≤ h, as (entry, exit)
/// along it, or `None` if it misses.
fn slab(o: f64, d: f64, half: f64) -> Option<(f64, f64)> {
    if d.abs() < 1e-15 {
        return (o.abs() <= half).then_some((f64::NEG_INFINITY, f64::INFINITY));
    }
    let (a, b) = ((-half - o) / d, (half - o) / d);
    Some((a.min(b), a.max(b)))
}

/// A ray against a box of `half` extents at `centre`, turned by `turn`: distance and normal.
fn hit_box(o: DVec3, d: DVec3, centre: DVec3, turn: DQuat, half: DVec3) -> Option<(f64, DVec3)> {
    let inv = turn.conjugate();
    let (lo, ld) = (inv * (o - centre), inv * d);
    let (mut near, mut far, mut axis) = (f64::NEG_INFINITY, f64::INFINITY, 0);
    for k in 0..3 {
        let (a, b) = slab(lo[k], ld[k], half[k])?;
        if a > near {
            (near, axis) = (a, k);
        }
        far = far.min(b);
    }
    if near > far || far < 0.0 || near < 0.0 {
        return None;
    }
    let mut n = DVec3::ZERO;
    n[axis] = -ld[axis].signum();
    Some((near, turn * n))
}

/// A ray against a convex hull (its faces' planes, in its frame at `centre`, `turn`).
fn hit_hull(
    o: DVec3,
    d: DVec3,
    centre: DVec3,
    turn: DQuat,
    hull: &super::hull::Hull,
) -> Option<(f64, DVec3)> {
    let inv = turn.conjugate();
    let (lo, ld) = (inv * (o - centre), inv * d);
    let (mut near, mut far, mut normal) = (f64::NEG_INFINITY, f64::INFINITY, DVec3::ZERO);
    for face in hull.faces() {
        let (dn, on) = (face.normal.dot(ld), face.normal.dot(lo) - face.offset);
        if dn.abs() < 1e-15 {
            if on > 0.0 {
                return None;
            }
            continue;
        }
        let t = -on / dn;
        if dn < 0.0 {
            if t > near {
                (near, normal) = (t, face.normal);
            }
        } else {
            far = far.min(t);
        }
    }
    (near <= far && near >= 0.0).then(|| (near, turn * normal))
}

/// A ray against a ball.
fn hit_ball(o: DVec3, d: DVec3, centre: DVec3, radius: f64) -> Option<(f64, DVec3)> {
    let m = o - centre;
    let (b, c) = (m.dot(d), m.length_squared() - radius * radius);
    let disc = b * b - c;
    if c > 0.0 && b > 0.0 || disc < 0.0 {
        return None;
    }
    let t = (-b - disc.sqrt()).max(0.0);
    Some((t, (m + d * t).normalize_or_zero()))
}

impl Shape {
    /// Where the ray from `origin` along unit `direction` enters this shape at `centre`,
    /// turned by `turn`: how far along it, and the surface's outward normal there.
    pub fn ray(
        &self,
        centre: DVec3,
        turn: DQuat,
        origin: DVec3,
        direction: DVec3,
    ) -> Option<(f64, DVec3)> {
        match self {
            Self::Sphere { radius } => hit_ball(origin, direction, centre, *radius),
            Self::Box { half } => hit_box(origin, direction, centre, turn, *half),
            Self::Hull(hull) => hit_hull(origin, direction, centre, turn, hull),
        }
    }

    /// How far `point` lies outside this shape at `centre`, turned by `turn` (m; negative
    /// inside): exact for a ball and a box; for a hull its farthest face's plane, exact inside
    /// and within its rounding of the corners outside.
    pub fn distance(&self, centre: DVec3, turn: DQuat, point: DVec3) -> f64 {
        let local = turn.conjugate() * (point - centre);
        match self {
            Self::Sphere { radius } => local.length() - radius,
            Self::Box { half } => {
                let q = local.abs() - *half;
                q.max(DVec3::ZERO).length() + q.max_element().min(0.0)
            }
            Self::Hull(hull) => (hull.faces().iter())
                .map(|f| f.normal.dot(local) - f.offset)
                .fold(f64::NEG_INFINITY, f64::max),
        }
    }
}

impl World {
    /// Where the ray from `origin` along `direction` (normalised here) first meets the ground
    /// within `length` (m), the bodies and fixed geometry left out: what stands between a
    /// character's eye and what it reaches for, whatever has moved since.
    pub fn ground_ray(&self, origin: DVec3, direction: DVec3, length: f64) -> Option<Hit> {
        ground_ray(self.ground.as_ref()?.as_ref(), origin, direction, length)
    }

    /// What the ray from `origin` along `direction` (normalised here) meets first within
    /// `length` (m), body `skip`'s shapes left out (a character's own).
    pub fn ray(
        &self,
        origin: DVec3,
        direction: DVec3,
        length: f64,
        skip: Option<usize>,
    ) -> Option<Hit> {
        let d = direction.normalize();
        let mut best: Option<Hit> = None;
        let offer =
            |best: &mut Option<Hit>, t: f64, normal: DVec3, body: Option<(usize, NodeId)>| {
                if t <= length && best.is_none_or(|b| t < b.distance) {
                    *best = Some(Hit {
                        distance: t,
                        point: origin + d * t,
                        normal,
                        body,
                    });
                }
            };
        for s in &self.statics {
            match s {
                StaticShape::Plane { normal, offset } => {
                    let dn = normal.dot(d);
                    if dn < 0.0 {
                        let t = (offset - normal.dot(origin)) / dn;
                        if t >= 0.0 {
                            offer(&mut best, t, *normal, None);
                        }
                    }
                }
                StaticShape::Box {
                    center,
                    rotation,
                    half,
                } => {
                    if let Some((t, n)) = hit_box(origin, d, *center, *rotation, *half) {
                        offer(&mut best, t, n, None);
                    }
                }
            }
        }
        for (b, body) in self.bodies.iter().enumerate() {
            if Some(b) == skip {
                continue;
            }
            let s = &body.structure;
            for (node, shape) in &body.shapes {
                let (c, q) = (s.position[*node], s.orientation[*node]);
                // Too far from the line to reach it, or behind or beyond it.
                let along = (c - origin).dot(d);
                let reach = shape.bounding_radius();
                if along < -reach
                    || along > length + reach
                    || ((c - origin) - d * along).length() > reach
                {
                    continue;
                }
                if let Some((t, n)) = shape.ray(c, q, origin, d) {
                    offer(&mut best, t, n, Some((b, *node)));
                }
            }
        }
        if let Some(ground) = &self.ground {
            let within = best.map_or(length, |b| b.distance);
            if let Some((t, n)) = march(ground.as_ref(), origin, d, within) {
                offer(&mut best, t, n, None);
            }
        }
        best
    }
}

/// Where the ray from `origin` along `direction` (normalised here) first meets `ground` within
/// `length` (m).
pub fn ground_ray(
    ground: &dyn super::ground::Ground,
    origin: DVec3,
    direction: DVec3,
    length: f64,
) -> Option<Hit> {
    let d = direction.normalize();
    march(ground, origin, d, length).map(|(t, normal)| Hit {
        distance: t,
        point: origin + d * t,
        normal,
        body: None,
    })
}

/// A ray marched over the ground's density (a distance to first order) within `length`:
/// where it crosses the surface, and the normal there.
fn march(
    ground: &dyn super::ground::Ground,
    o: DVec3,
    d: DVec3,
    length: f64,
) -> Option<(f64, DVec3)> {
    let density = |t: f64| ground.density_gradient(o + d * t);
    // Newton along the ray (the density falls by g·d a metre), kept within the bracket of
    // the last point outside and the first inside, bisecting where it would leave it: two
    // or three evaluations where the density is near a distance (marched in steps and
    // bisected, a character's legs took 25 a tick on the terrain).
    let (mut lo, mut hi) = (0.0, f64::INFINITY);
    let mut t = 0.0;
    for _ in 0..64 {
        let (v, g) = density(t);
        if v <= 0.0 {
            hi = t;
            if t == 0.0 {
                return Some((0.0, g.normalize_or_zero()));
            }
        } else {
            lo = t;
        }
        if hi - lo <= SURFACE || v.abs() <= SURFACE * g.length() {
            return (v <= SURFACE * g.length()).then(|| (t, g.normalize_or_zero()));
        }
        let slope = g.dot(d);
        let newton = (slope < 0.0).then(|| t - v / slope);
        t = match newton {
            Some(n) if n > lo && n < hi.min(length + SURFACE) => n,
            // Leaving the bracket: halve it; before any point inside, step on by the
            // distance (at most `MARCH`) up to the ray's end.
            _ if hi.is_finite() => 0.5 * (lo + hi),
            _ => (lo + (0.9 * v / g.length().max(1e-9)).clamp(SURFACE, MARCH)).min(length),
        };
        if !hi.is_finite() && lo >= length {
            return None;
        }
    }
    hi.is_finite()
        .then(|| (hi, density(hi).1.normalize_or_zero()))
}

#[cfg(test)]
mod tests;
