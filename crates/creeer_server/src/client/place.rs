//! Placing what the hands hold (G2.10, ADR-0036; the user, 2026-10-30): in first person, the
//! held part rests flush against what the crosshair meets within the placing reach (the ground,
//! another part, a tree, a hole's floor), its side to the surface and the point aimed at where
//! they touch; meeting nothing, it hangs before the eyes. Its long axis lies across the view
//! (turned by `yaw`) or stands up (`tilt`: R lies it down or stands it up, Q with the mouse
//! turns it finer). No grid, no ghost, no snapping: the aim is the precision.

use super::Connection;
use super::hold::heading;
use creeer_math::det;
use creeer_math::glam::{DQuat, DVec3};
use creeer_net::entity::{EntityId, Kind, Shape as NetShape};
use creeer_net::message::Hold;
use creeer_sim::reach;

/// How far before the eyes a held part hangs when the view meets nothing within reach (m).
const HANGS: f64 = 1.0;
/// How far beside the aimed ray two more are cast to find the surface's normal (m).
const SPAN: f64 = 0.02;
/// Stood up (its long axis this near upright: the cosine), how far below where it is aimed its
/// foot is let down at most (m).
const STANDING: f64 = 0.8;
const DROPS: f64 = 1.5;
/// How far above a hole's lowest point a post's foot is let down (m).
const FOOT_ABOVE: f64 = 0.03;
/// How far beyond the placing reach the crosshair looks (m): into a hole a few metres off, its
/// far wall lies just beyond the reach; the hands then hold the part as near as they can.
const LOOKS_BEYOND: f64 = 1.0;
/// Turned more than `TURNING` from how it is meant (rad), it is lifted clear of where it goes,
/// wholly (`CLEAR` m beyond half its length) once turned `TURNED` off, to turn in the air:
/// between, by the share. (All or nothing at 10°, a 27 kg pole swaying ±15° as it was lowered
/// into its hole was lifted 1.6 m and let down again over and over, G2.10.)
const TURNING: f64 = 0.17;
const TURNED: f64 = 0.5;
const CLEAR: f64 = 0.1;
/// Lying, with something to rest on within this much below where it is aimed (m), it is let
/// down onto what lies under it, found by this many rays along its length from this far above.
const RESTS_WITHIN: f64 = 0.3;
/// Let down onto what it rests on, held this far above it (m: within the 3 cm a nail or lashing
/// draws parts together): let down onto it exactly, both hands pressed a beam into two set
/// posts' tops, and when one hand held it while the other nailed, the posts sprang back and
/// pushed it 7 cm off them (G2.10).
const REST_ABOVE: f64 = 0.01;
const SAMPLES: u32 = 8;
const ABOVE: f64 = 1.0;

/// What the crosshair meets: the point, and the surface's normal there (towards the eyes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surface {
    pub point: DVec3,
    pub normal: DVec3,
}

impl Connection {
    /// The surface the view `view` meets from the avatar's eyes within the placing reach,
    /// passing through what the hands hold; `also` casts at what this connection does not know
    /// (standing trees: from, along, at most; how far along it meets one). Its normal is found
    /// from two rays beside the aimed one.
    pub fn surface_aimed(
        &self,
        view: DQuat,
        also: impl Fn(DVec3, DVec3, f64) -> Option<f64>,
    ) -> Option<Surface> {
        let avatar = self.avatar()?;
        let eye = reach::eye(avatar.position, avatar.position.normalize());
        let skip = self.own_load();
        let along = view * DVec3::NEG_Z;
        let hit = |from: DVec3| {
            let body = self.aim_skipping(from, along, reach::PLACING + LOOKS_BEYOND, skip);
            let near = body.map_or(reach::PLACING + LOOKS_BEYOND, |a| a.distance);
            let t = also(from, along, near)
                .filter(|t| *t < near)
                .or(body.map(|a| a.distance))?;
            Some(from + along * t)
        };
        let point = hit(eye)?;
        let beside = (
            hit(eye + view * DVec3::X * SPAN),
            hit(eye + view * DVec3::Y * SPAN),
        );
        let normal = match beside {
            (Some(a), Some(b)) => (a - point).cross(b - point).try_normalize(),
            _ => None,
        }
        .map_or(-along, |n| if n.dot(along) > 0.0 { -n } else { n });
        Some(Surface { point, normal })
    }

    /// The hold placing what the hands hold: its long axis turned `yaw` from across the view's
    /// heading, tilted `tilt` up from level and slid `slide` along itself (the wheel: to lay a
    /// beam from one post's top to the next), resting flush against `surface` (the point aimed
    /// at where they touch) or, with none, hanging before the eyes.
    pub fn place_hold(
        &self,
        view: DQuat,
        surface: Option<Surface>,
        (yaw, tilt, slide): (f64, f64, f64),
    ) -> Option<Hold> {
        let load = self.own_load()?;
        let avatar = self.avatar()?;
        let up = avatar.position.normalize();
        let corners = self.corners(load)?;
        let heading = heading(view, up);
        let (ys, yc) = det::sin_cos(yaw);
        let across = (heading * DVec3::X) * yc + (heading * DVec3::NEG_Z) * ys;
        let (ts, tc) = det::sin_cos(tilt);
        let axis = (across * tc + up * ts).normalize();
        let turn = DQuat::from_rotation_arc(long_axis(&corners), axis).normalize();
        let standing = axis.dot(up) > STANDING;
        // Stood up, it goes down from where it is aimed until its foot meets what lies below, as
        // a post is let down into its hole (seen from a few metres, a hole's floor is hidden by
        // its rim: aimed at its mouth, the post meets the floor).
        let surface = surface.map(|s| {
            // Let down from where its axis will be: off a hole's wall by its own half-width (cast
            // from the wall itself, the ray ran down the wall and stopped 15 cm in).
            let off = s.normal.reject_from(up).normalize_or_zero();
            let from = s.point + off * (support(&corners, turn, -off) + 0.01) + up * 0.05;
            let below = standing
                .then(|| self.aim_skipping(from, -up, DROPS, Some(load)))
                .flatten();
            // Let down to just above the floor: on a hole's round floor a square foot meets it
            // 2–3 cm above its lowest point, and let down to that point the hands pressed the post
            // into the floor at their full strength and pushed its holder 0.37 m back (G2.10).
            below.map_or(s, |b| Surface {
                point: b.point + up * FOOT_ABOVE,
                normal: up,
            })
        });
        let centre = match surface {
            // Lying, with something to rest on just under where it is aimed: let down onto the
            // highest thing under its whole length, as a beam is set on two posts' tops (flush
            // against the exact point aimed at, a narrow post's rounded top set it beside the
            // posts; its normal, from rays beside it, missed so narrow a top, G2.10). Else, as
            // against a tree's trunk, flush against what is aimed at.
            Some(s) => {
                let half = support(&corners, DQuat::IDENTITY, long_axis(&corners));
                let over = s.point + axis * slide;
                // Along its length, and under the point aimed at itself: a post's 10 cm top fell
                // between rays 20 cm apart, and the beam was laid beside it (G2.10).
                let aimed = (s.point - over).dot(axis);
                let highest = (0..=SAMPLES)
                    .map(|k| (f64::from(k) / f64::from(SAMPLES)).mul_add(2.0, -1.0) * half)
                    .chain([aimed])
                    .filter_map(|t| {
                        let from = over + axis * t + up * ABOVE;
                        let hit = self.aim_skipping(from, -up, ABOVE + DROPS, Some(load))?;
                        Some((hit.point - over).dot(up))
                    })
                    .fold(f64::MIN, f64::max);
                if !standing && highest > -RESTS_WITHIN {
                    over + up * (highest + support(&corners, turn, -up) + REST_ABOVE)
                } else {
                    s.point + s.normal * support(&corners, turn, -s.normal) + axis * slide
                }
            }
            None => reach::eye(avatar.position, up) + (view * DVec3::NEG_Z) * HANGS,
        };
        // Still turning (stood up from lying, or laid down), it is lifted clear to turn and let
        // down when turned, as a post is lifted, swung up and lowered in: turned where it lies,
        // its end met the ground and it never stood (G2.10).
        let drawn = self.poses().get(&load).map(|p| p.orientation);
        // (Its length's way only: a round pole rolls in the hands about its own axis.)
        let long = long_axis(&corners);
        let off = drawn.map_or(0.0, |q| det::acos((q * long).dot(axis).abs().min(1.0)));
        let share = ((off - TURNING) / (TURNED - TURNING)).clamp(0.0, 1.0);
        let lifted = up * ((support(&corners, DQuat::IDENTITY, long) + CLEAR) * share);
        // Gripped within the placing reach of the eyes, as the server's hands hold it
        // (`hands.rs`): the client draws it by their law (`steered.rs`).
        let eye = reach::eye(avatar.position, up);
        let grip = turn * self.own_carried().map_or(DVec3::ZERO, |c| c.grip);
        let centre = eye + (centre + lifted + grip - eye).clamp_length_max(reach::PLACING) - grip;
        Some(Hold {
            offset: centre - avatar.position,
            turn,
            steady: false,
        })
    }

    /// The corners of `id`'s shape about its drawn frame, as this connection knows it.
    fn corners(&self, id: EntityId) -> Option<Vec<DVec3>> {
        Some(match self.kinds.get(&id)? {
            Kind::Body(NetShape::Box(h), _) => (0..8)
                .map(|k| {
                    let s = |b: usize| if k & b == 0 { -1.0 } else { 1.0 };
                    DVec3::new(s(1) * h.x, s(2) * h.y, s(4) * h.z)
                })
                .collect(),
            Kind::Body(NetShape::Sphere(r), _) => DVec3::AXES
                .iter()
                .flat_map(|a| [*a * *r, -*a * *r])
                .collect(),
            Kind::Body(NetShape::Hull(corners), _) => corners.clone(),
            Kind::Structure(frame) => frame.nodes.clone(),
            _ => return None,
        })
    }
}

/// The axis (in the shape's frame) along which `corners` reach farthest about their middle.
fn long_axis(corners: &[DVec3]) -> DVec3 {
    #[allow(clippy::cast_precision_loss)] // corners
    let mean = corners.iter().sum::<DVec3>() / corners.len().max(1) as f64;
    let spread = |a: DVec3| {
        corners
            .iter()
            .map(|c| (*c - mean).dot(a).abs())
            .fold(0.0, f64::max)
    };
    [DVec3::X, DVec3::Y, DVec3::Z]
        .into_iter()
        .fold((DVec3::X, -1.0), |(best, s), a| {
            let t = spread(a);
            if t > s { (a, t) } else { (best, s) }
        })
        .0
}

/// How far the shape's `corners`, turned `turn`, reach along `way` from its centre (m).
fn support(corners: &[DVec3], turn: DQuat, way: DVec3) -> f64 {
    corners
        .iter()
        .map(|c| (turn * *c).dot(way))
        .fold(0.0, f64::max)
}
