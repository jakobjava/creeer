//! The fingers (G1.4b, ADR-0027 decision 2): three bones a digit in the skin, posed from each
//! hand's frame. The ragdoll's hand is one segment; its digits are drawn only.
//!
//! - **Relaxed** as measured: fifteen men's hands hanging relaxed at their sides, the forearm
//!   neutral (Lee, Mo, Hwang, Wang and Jung, "Relaxed hand postures", Vicon, 26 markers): the
//!   fingers' knuckle (MCP), middle (PIP) and last (DIP) joints bent index 28.4°, 25.5°, 13.1°;
//!   middle 32.8°, 30.1°, 12.7°; ring 24.7°, 34.5°, 11.7°; little 16.6°, 32.1°, 15.6°; the
//!   thumb's knuckle 46.1° and last joint 8.5° (its base not measured: as modelled). The
//!   fingers together (the model's are spread as Blender Studio's base mesh is).
//! - **Gripping** a handle: each bone turned in from the knuckle out until it meets the handle
//!   (a cylinder as thick as the handle and the finger's half thickness), as a hand closes on
//!   what it holds; the thumb round the other side.

use creeer_art::model_skin::Bone;
use creeer_math::Frame64;
use creeer_math::glam::{DQuat, DVec3};
use creeer_sim::character::bones::{DIGITS, finger_bone};

/// The relaxed hand's joints, by digit from the thumb, from the hand out (degrees).
const RELAXED: [[f64; 3]; 5] = [
    [0.0, 46.1, 8.5],
    [28.4, 25.5, 13.1],
    [32.8, 30.1, 12.7],
    [24.7, 34.5, 11.7],
    [16.6, 32.1, 15.6],
];
/// The most each joint bends gripping (rad): a knuckle and the fingers' joints about a right
/// angle and a little more, their range.
const MOST: [f64; 3] = [1.75, 1.9, 1.4];
/// A finger's half thickness in its glove (m): what lies between its bones and a handle.
const FINGER: f64 = 0.011;
/// The steps a hand closes in (each joint bends at most a 400th of its range a step).
const CLOSING: u16 = 400;
/// The most the thumb's joints bend closing over a handle (rad): its base as modelled (turned,
/// it tore the skin at the thumb's root), its knuckle and last joint as the fingers' do.
const THUMB_MOST: [f64; 3] = [0.0, 1.0, 1.2];
/// The last bone's length, over the middle one's: its tip lies past its head by this share
/// of the middle bone (Hamilton and Dunsmuir: the middle joint to the last 1.3 times the last
/// to the tip).
const DISTAL: f64 = 1.3;

/// One digit at rest: each bone's head, its direction and the axis it bends about (positive
/// towards the palm), and the turn about the palm's normal that lays it beside the middle
/// finger.
#[derive(Clone, Debug)]
struct Digit {
    heads: [DVec3; 3],
    axes: [DVec3; 3],
    together: DQuat,
}

/// One hand at rest: its segment, its digits from the thumb, its palm's normal (out of the
/// palm) and where a handle lies across it.
#[derive(Clone, Debug)]
struct Hand {
    segment: usize,
    digits: [Digit; 5],
    palm: DVec3,
    across: DVec3,
    knuckles: DVec3,
    down: DVec3,
}

/// Each digit's bends, from the thumb, from the hand out (rad).
type Bends = [[f64; 3]; 5];

/// How a hand is held.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hold {
    /// Relaxed, as measured.
    Relaxed,
    /// Open: every joint straight, the fingers together (reaching, letting go).
    // Held open by G1.4d's reach and G1.4f's throws; until then by its test alone.
    #[cfg_attr(not(test), allow(dead_code))]
    Open,
    /// Closed on a handle of this radius (m) across the palm.
    Grip(f64),
}

/// Both hands' digits at rest, from the model's bones.
#[derive(Clone, Debug)]
pub struct Hands {
    hands: [Hand; 2],
    /// The grips worked out so far: by side and radius.
    grips: std::cell::RefCell<Vec<((usize, u64), Bends)>>,
}

impl Hands {
    /// The hands of a model whose bones are `bones` (the right hand's segment 5, the left's
    /// 11); none if it has no finger bones.
    pub fn new(bones: &[Bone]) -> Option<Self> {
        let head = |name: &str| {
            let b = bones.iter().find(|b| b.name == name)?;
            Some(DVec3::from(b.head.map(f64::from)))
        };
        let side = |tag: char, segment: usize, outward: f64| -> Option<Hand> {
            let heads: Vec<[DVec3; 3]> = DIGITS
                .iter()
                .map(|d| Some([1, 2, 3].map(|k| head(&finger_bone(d, k, tag)))))
                .map(|h| h.and_then(|h| Some([h[0]?, h[1]?, h[2]?])))
                .collect::<Option<_>>()?;
            let wrist = head(if tag == 'R' { "hand.R" } else { "hand.L" })?;
            // The palm's normal, out of it: across the knuckles and down the hand, turned to
            // face the body's middle (the palms are to the thighs at rest).
            let across = (heads[1][0] - heads[4][0]).normalize();
            let down = (heads[2][0] - wrist).normalize();
            let mut palm = across.cross(down).normalize();
            if palm.x * outward > 0.0 {
                palm = -palm;
            }
            let along = |h: &[DVec3; 3]| [(h[1] - h[0]).normalize(), (h[2] - h[1]).normalize()];
            let middle = along(&heads[2])[0];
            let digits = std::array::from_fn(|k| {
                let h = heads[k];
                let [d0, d1] = along(&h);
                // The thumb bends across the palm towards the index; the fingers into the palm.
                let towards = if k == 0 {
                    let to = heads[1][0] - h[1];
                    (to - d1 * to.dot(d1)).normalize()
                } else {
                    palm
                };
                let flat = |d: DVec3| (d - palm * d.dot(palm)).normalize();
                let together = if k == 0 {
                    DQuat::IDENTITY
                } else {
                    DQuat::from_rotation_arc(flat(d0), flat(middle))
                };
                let axis = |d: DVec3| d.cross(towards).normalize();
                Digit {
                    heads: h,
                    // The first bone bends about its axis laid beside the middle finger; the
                    // others about their own, carried by it.
                    axes: [together * axis(d0), axis(d1), axis(d1)],
                    together,
                }
            });
            let knuckles = (1..5).map(|k| heads[k][0]).sum::<DVec3>() / 4.0;
            Some(Hand {
                segment,
                digits,
                palm,
                across,
                knuckles,
                down,
            })
        };
        Some(Self {
            hands: [side('R', 5, -1.0)?, side('L', 11, 1.0)?],
            grips: std::cell::RefCell::default(),
        })
    }

    /// The finger bones' frames (`bones::finger_bones`' order) for the segments' `segments`
    /// (the ragdoll's order), the hands held `hold` (right, left).
    pub fn pose(&self, segments: &[Frame64], hold: [Hold; 2]) -> Vec<Frame64> {
        let mut out = Vec::with_capacity(30);
        for (side, (hand, hold)) in self.hands.iter().zip(hold).enumerate() {
            let base = segments[hand.segment];
            let bends = match hold {
                Hold::Relaxed => RELAXED.map(|d| d.map(f64::to_radians)),
                Hold::Open => [[0.0; 3]; 5],
                Hold::Grip(radius) => self.gripping(side, radius),
            };
            for (digit, bends) in hand.digits.iter().zip(bends) {
                out.extend(digit.bones(base, bends));
            }
        }
        out
    }

    /// Hand `side`'s bends gripping a handle of `radius` (m), worked out once a radius (the
    /// closing takes a few hundred steps a finger).
    fn gripping(&self, side: usize, radius: f64) -> Bends {
        let key = (side, radius.to_bits());
        let mut cache = self.grips.borrow_mut();
        if let Some(bends) = cache.iter().find(|(k, _)| *k == key) {
            return bends.1;
        }
        let hand = &self.hands[side];
        let bends = std::array::from_fn(|k| {
            if k == 0 {
                hand.thumb_over(radius)
            } else {
                hand.wrapped(k, radius)
            }
        });
        cache.push((key, bends));
        bends
    }
}

impl Digit {
    /// Its last bone's length (m), from its head to the tip.
    fn tip(&self) -> f64 {
        (self.heads[2] - self.heads[1]).length() / DISTAL
    }

    /// Its bones' frames on the hand's `base`, bent `bends` (rad) from the hand out.
    fn bones(&self, base: Frame64, bends: [f64; 3]) -> [Frame64; 3] {
        let mut parent = base;
        std::array::from_fn(|j| {
            let mut turn = DQuat::from_axis_angle(self.axes[j], bends[j]);
            if j == 0 {
                turn = (turn * self.together).normalize();
            }
            let h = self.heads[j];
            parent = parent * Frame64::new(h - turn * h, turn);
            parent
        })
    }

    /// Where its bones' heads and tip lie bent `bends`, in the hand's rest frame.
    fn points(&self, bends: [f64; 3], tip: f64) -> [DVec3; 4] {
        let frames = self.bones(Frame64::IDENTITY, bends);
        let last = self.heads[2] - self.heads[1];
        [
            frames[0].transform_point(self.heads[0]),
            frames[1].transform_point(self.heads[1]),
            frames[2].transform_point(self.heads[2]),
            frames[2].transform_point(self.heads[2] + last.normalize() * tip),
        ]
    }
}

impl Hand {
    /// Where a handle of `radius` (m) lies held, in the hand's rest frame: against the palm,
    /// across it, a radius back from the knuckles (the fingers bent round its far side).
    fn handle(&self, radius: f64) -> DVec3 {
        self.knuckles - self.down * radius + self.palm * (radius + FINGER)
    }

    /// The bends of digit `k` closed on a handle of `radius` (m) across the palm, as a hand
    /// closes: its joints bending together, each in proportion to its range, until a bone meets
    /// the handle; that bone's joint and those before it stop there, and the joints beyond go on
    /// (the closing of grasp planners such as GraspIt's).
    fn wrapped(&self, k: usize, radius: f64) -> [f64; 3] {
        let digit = &self.digits[k];
        let centre = self.handle(radius);
        let reach = radius + FINGER;
        let tip = digit.tip();
        let meets = |b: &[f64; 3], i: usize| {
            let p = digit.points(*b, tip);
            distance_to_line(p[i], p[i + 1], centre, self.across) <= reach
        };
        let mut bends = [0.0; 3];
        let mut moving = 0;
        for step in 1..=CLOSING {
            if moving == 3 {
                break;
            }
            #[allow(clippy::cast_precision_loss)]
            let s = f64::from(step) / f64::from(CLOSING);
            let mut next = bends;
            for j in moving..3 {
                next[j] = MOST[j] * s;
            }
            match (moving..3).find(|&i| meets(&next, i)) {
                Some(i) => moving = i + 1,
                None => bends = next,
            }
        }
        bends
    }
}

impl Hand {
    /// The thumb's bends over a handle of `radius` (m) gripped: closing, its joints together,
    /// until its last bone meets the side of the index finger's middle bone, closed on the
    /// handle (the thumb lies over the fingers in a power grip).
    fn thumb_over(&self, radius: f64) -> [f64; 3] {
        let thumb = &self.digits[0];
        let index = &self.digits[1];
        let p = index.points(self.wrapped(1, radius), index.tip());
        let tip = thumb.tip();
        let mut bends = [0.0; 3];
        for step in 1..=CLOSING {
            let s = f64::from(step) / f64::from(CLOSING);
            let next = THUMB_MOST.map(|m| m * s);
            let q = thumb.points(next, tip);
            if segment_distance(q[2], q[3], p[1], p[2]) <= 2.0 * FINGER {
                break;
            }
            bends = next;
        }
        bends
    }
}

/// How near two segments come (m), sampled along the first.
fn segment_distance(a: DVec3, b: DVec3, c: DVec3, d: DVec3) -> f64 {
    let cd = d - c;
    (0..=16)
        .map(|i| {
            let p = a.lerp(b, f64::from(i) / 16.0);
            let x = ((p - c).dot(cd) / cd.length_squared().max(1e-12)).clamp(0.0, 1.0);
            (p - (c + cd * x)).length()
        })
        .fold(f64::INFINITY, f64::min)
}

/// How near the segment from `a` to `b` comes to the line through `c` along `axis`.
fn distance_to_line(a: DVec3, b: DVec3, c: DVec3, axis: DVec3) -> f64 {
    let flat = |p: DVec3| {
        let q = p - c;
        q - axis * q.dot(axis)
    };
    let (a, b) = (flat(a), flat(b));
    let ab = b - a;
    let x = (-a.dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
    (a + ab * x).length()
}

#[cfg(test)]
#[path = "hands_tests.rs"]
mod tests;
