//! Where the hands hold a load (G2.10, ADR-0036), from what the server tells of it (the point
//! held, one hand or two: `creeer_net::entity::Carried`) and its shape as drawn:
//!
//! - **One hand**: the right, at the point held.
//! - **Two on a pole** (a load whose least moment is under a tenth of its largest,
//!   `carry::SLENDER`: the server's test): along its long axis, `carry::HANDS_APART` apart about
//!   the point held, kept on it, each closed round it as G1.4b closes a hand on a handle (laid on
//!   a pole thicker than a hand wraps, closed on as much of it).
//! - **Two on a box** (one: the right): on its sides across the body, the fingers curled under
//!   its bottom, as people hold a box without handles (`hands_rest.rs`); a box on the ground, or
//!   deeper below the point held than the forearms reach down its sides, flat on its sides at
//!   the point held's height.
//!
//! Each hand's thumb and index point the way the body's middle, front and up lie (overhand on
//! a pole across the body, the thumb up a pole held upright), its fingers on along the way the
//! arm reaches.

use super::hands::{HandShape, Hold};
use creeer_math::Frame64;
use creeer_math::glam::{DMat3, DQuat, DVec3};
use creeer_sim::carry::{HANDS_APART, SLENDER};

/// The thickest handle the fingers close round (m, its radius): 50 mm across, the thickest of
/// Kong and Lowe's handles (2005; the most comfortable 30–40 mm). A thicker pole has the hand
/// laid on it, closed on as much of it.
pub const WRAP_MOST: f64 = 0.025;
/// Half a gloved hand's breadth across the knuckles (m: men's hand breadth about 85–90 mm):
/// a hand stays that far within a load's ends.
const HAND_HALF: f64 = 0.045;
/// How far below the point held the hands go under a box's bottom (m): about a forearm's
/// reach down its sides from where the hands hold it; deeper, they hold its sides flat.
const UNDER: f64 = 0.3;

/// A load in the hands, as the client draws it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Load {
    /// Which it is (the caller's key: an entity).
    pub id: u32,
    /// Its frame as drawn (its node 0's).
    pub frame: Frame64,
    /// The box about it in that frame: its centre and half extents (m).
    pub centre: DVec3,
    pub half: DVec3,
    /// The point the hands hold (its frame), and in one hand (the right) or two.
    pub grip: DVec3,
    pub one_hand: bool,
}

/// Where a hand holds: how its fingers close, and its hold's frame in the world (where
/// `HandShape::rest`'s frame of that hold goes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reach {
    pub hold: Hold,
    pub frame: Frame64,
}

/// The body that reaches: where each arm reaches from (its shoulder; right, left), its up and
/// the way it faces.
#[derive(Clone, Copy, Debug)]
pub struct Reaching {
    pub from: [DVec3; 2],
    pub up: DVec3,
    pub forward: DVec3,
}

impl Load {
    /// The point held, in the world.
    pub fn held(&self) -> DVec3 {
        self.frame.transform_point(self.grip)
    }

    /// Its long axis (an index of its frame's axes), half length and section's radius (m), if
    /// it is slender: the moments of the box about it (as of any uniform body of its shape) the
    /// server's test.
    fn slender(&self) -> Option<(usize, f64, f64)> {
        let h = self.half.to_array();
        let square = |k: usize| h[k % 3] * h[k % 3];
        let moments = [0, 1, 2].map(|k| square(k + 1) + square(k + 2));
        let k = (0..3).fold(0, |m, k| if moments[k] < moments[m] { k } else { m });
        let largest = moments.iter().copied().fold(0.0, f64::max);
        (moments[k] < SLENDER * largest).then(|| (k, h[k], h[(k + 1) % 3].max(h[(k + 2) % 3])))
    }

    /// Where each hand holds it (right, left; the left none in one hand), `body` reaching, each
    /// hand shaped `hands`; `clear` says whether a point is clear of the ground beneath it (the
    /// fingers get under a box only when it is off the ground).
    pub fn reaches(
        &self,
        body: &Reaching,
        hands: &[HandShape; 2],
        clear: &dyn Fn(DVec3) -> bool,
    ) -> [Option<Reach>; 2] {
        match self.slender() {
            Some(pole) => self.on_pole(pole, body, hands),
            None => {
                let side = |s: usize| Some(self.on_side(s, body, &hands[s], clear));
                [side(0), if self.one_hand { None } else { side(1) }]
            }
        }
    }

    /// On a pole along axis `k`, `half` long either side of its middle, `radius` round.
    fn on_pole(
        &self,
        (k, half, radius): (usize, f64, f64),
        body: &Reaching,
        hands: &[HandShape; 2],
    ) -> [Option<Reach>; 2] {
        let local = DVec3::AXES[k];
        let along = (self.grip - self.centre).dot(local);
        let room = (half - HAND_HALF).max(0.0);
        // Along it from its middle: the right hand at the point held; two apart about it, kept
        // on the pole (squeezed on a short one).
        let at: Vec<f64> = if self.one_hand {
            vec![along.clamp(-room, room)]
        } else {
            let apart = HANDS_APART.min(2.0 * room);
            let lo = (along - 0.5 * apart).clamp(-room, room - apart);
            vec![lo, lo + apart]
        };
        let world = |t: f64| self.frame.transform_point(self.centre + local * t);
        let axis = self.frame.rotation * local;
        let points: Vec<DVec3> = at.into_iter().map(world).collect();
        // Each hand to the nearer (two: the pairing the shoulders reach the less far for).
        let order = match points.as_slice() {
            [a, b] => {
                let d = |p: DVec3, s: usize| (p - body.from[s]).length();
                if d(*a, 0) + d(*b, 1) <= d(*b, 0) + d(*a, 1) {
                    [Some(*a), Some(*b)]
                } else {
                    [Some(*b), Some(*a)]
                }
            }
            [a] => [Some(*a), None],
            _ => [None, None],
        };
        let wrap = radius.min(WRAP_MOST);
        // (To the millimetre: the fingers' closing is worked out once a radius.)
        let wrap = (wrap * 1e3).round() * 1e-3;
        std::array::from_fn(|s| {
            let on_axis = order[s]?;
            let y = thumb_way(axis, s, body);
            let z = (on_axis - body.from[s])
                .reject_from(y)
                .try_normalize()
                .unwrap_or_else(|| {
                    (-body.up)
                        .reject_from(y)
                        .normalize_or(y.any_orthonormal_vector())
                });
            let x = y.cross(z);
            // The palm on the pole's surface, its axis that far on the palm's side.
            let palm = x * hands[s].palm_sign();
            Some(Reach {
                hold: Hold::Grip(wrap),
                frame: frame(on_axis - palm * (radius - wrap), x, y, z),
            })
        })
    }

    /// Hand `s` (0 the right) on the box's side that way across the body: under its bottom edge
    /// if that is within [`UNDER`] below the point held and off the ground (`clear`), else
    /// flat on the side at the point held's height (as people take a box up from the ground by
    /// its sides, before they get their fingers under it).
    fn on_side(
        &self,
        s: usize,
        body: &Reaching,
        hand: &HandShape,
        clear: &dyn Fn(DVec3) -> bool,
    ) -> Reach {
        let q = self.frame.rotation;
        let left = body.up.cross(body.forward).normalize_or(DVec3::X);
        let way = if s == 0 { -left } else { left };
        let face = |dir: DVec3, skip: Option<usize>| {
            let k = (0..3)
                .filter(|&k| Some(k) != skip)
                .max_by(|&a, &b| {
                    let d = |k: usize| (q * DVec3::AXES[k]).dot(dir).abs();
                    d(a).total_cmp(&d(b))
                })
                .unwrap_or(0);
            let sign = (q * DVec3::AXES[k]).dot(dir).signum();
            (k, if sign == 0.0 { 1.0 } else { sign })
        };
        let (i, si) = face(way, None);
        let (j, sj) = face(-body.up, Some(i));
        let k = 3 - i - j;
        let (c, h) = (self.centre.to_array(), self.half.to_array());
        let mut at = [0.0; 3];
        at[i] = c[i] + si * h[i];
        at[j] = c[j] + sj * h[j];
        // Along the edge: between the point held and the middle, a hand's half within its ends.
        let room = (h[k] - HAND_HALF).max(0.0);
        at[k] = c[k] + (0.5 * (self.grip[k] + c[k]) - c[k]).clamp(-room, room);
        let edge = self.frame.transform_point(DVec3::from_array(at));
        let (side, bottom) = (q * DVec3::AXES[i] * si, q * DVec3::AXES[j] * sj);
        // The palm into the box through its side, the fingers on down it.
        let x = -side * hand.palm_sign();
        let (z, y) = (bottom, bottom.cross(x));
        let depth = (self.held() - edge).dot(body.up);
        if depth <= UNDER && clear(edge) {
            return Reach {
                hold: Hold::Edge,
                frame: frame(edge, x, y, z),
            };
        }
        // Up the side from its bottom edge to the point held's height, kept a hand's half within
        // the side (its middle on a side shorter than a hand).
        let height = 2.0 * h[j];
        let up_side = depth.clamp(
            HAND_HALF.min(0.5 * height),
            (height - HAND_HALF).max(0.5 * height),
        );
        Reach {
            hold: Hold::Open,
            frame: frame(edge - bottom * up_side, x, y, z),
        }
    }
}

/// Which way hand `s`'s thumb and index point along a pole whose axis is `axis`: towards the
/// body's middle, front and up as near as the pole lets them.
fn thumb_way(axis: DVec3, s: usize, body: &Reaching) -> DVec3 {
    let left = body.up.cross(body.forward).normalize_or(DVec3::X);
    let middle = if s == 0 { left } else { -left };
    let towards = middle + body.forward + body.up;
    if axis.dot(towards) < 0.0 { -axis } else { axis }
}

/// The frame at `at` whose axes are `x`, `y` and `z`.
fn frame(at: DVec3, x: DVec3, y: DVec3, z: DVec3) -> Frame64 {
    Frame64::new(at, DQuat::from_mat3(&DMat3::from_cols(x, y, z)).normalize())
}
