//! Where each hand touches what it holds, in the model at rest (G2.10, ADR-0036): the frame its
//! arm is placed by (`arms.rs`), for each way of holding: a handle's axis amid the fingers
//! (`Hold::Grip`, as `Hands::grip`), an edge under the palm's end (`Hold::Edge`: the fingers
//! curled under a box's bottom, its side on the palm, as people grasp a box without handles: on
//! its sides, the fingers under its bottom, in a UFSCar study of industrial load handling), or
//! the palm's middle laid flat on a face (`Hold::Open`).

use super::{Bends, FINGER, Hand, Hands, Hold};
use creeer_math::Frame64;
use creeer_math::glam::{DMat3, DQuat, DVec3};
use creeer_sim::character::bones;

/// The key an edge hold's bends are kept by among the grips (no handle's radius has these
/// bits: they are a NaN's).
const EDGE: u64 = u64::MAX;

/// A hand's shape at rest (the model's frame): its wrist and knuckles, its palm's normal (out
/// of it), the way across it past the index finger, and down it towards the fingertips.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandShape {
    pub wrist: DVec3,
    pub knuckles: DVec3,
    pub palm: DVec3,
    pub across: DVec3,
    pub down: DVec3,
}

impl HandShape {
    /// The ragdoll's hand `side` (0 the right) without the model's fingers, from its segment:
    /// the knuckles at its middle, down it from the wrist, the palm to the body's middle (the
    /// animation's own tests; the game's are the model's, `Hands::shapes`).
    pub fn approximate(side: usize) -> Self {
        let wrist = bones::joints()[super::super::skeleton::ELBOW[side] + 1].2;
        let knuckles = bones::centres()[[5, 11][side]];
        let down = (knuckles - wrist).normalize();
        let across = DVec3::Z.reject_from(down).normalize();
        let palm = across.cross(down) * if side == 0 { 1.0 } else { -1.0 };
        Self {
            wrist,
            knuckles,
            palm,
            across,
            down,
        }
    }

    /// The frame `hold` holds by, at rest: its origin where it touches (a handle's axis amid
    /// the fingers; an edge just past the knuckles, a finger's thickness out of the palm; the
    /// palm's middle on its skin), its +y across the palm past the index finger, its +z down the
    /// hand (`Hands::grip`'s).
    pub fn rest(&self, hold: Hold) -> Frame64 {
        let y = self.across;
        let z = self.down.reject_from(y).normalize();
        let x = y.cross(z);
        let at = match hold {
            Hold::Grip(radius) => {
                self.knuckles - self.down * radius + self.palm * (radius + FINGER)
            }
            Hold::Edge => self.knuckles + (self.palm - self.down) * FINGER,
            Hold::Open | Hold::Relaxed => 0.5 * (self.wrist + self.knuckles) + self.palm * FINGER,
        };
        Frame64::new(at, DQuat::from_mat3(&DMat3::from_cols(x, y, z)))
    }

    /// Which way the palm faces along the hold's frame's x: +1 or −1 (the left hand is the
    /// right's mirror).
    pub fn palm_sign(&self) -> f64 {
        let y = self.across;
        let z = self.down.reject_from(y).normalize();
        self.palm.dot(y.cross(z)).signum()
    }
}

impl Hands {
    /// Both hands' shapes at rest (right, left).
    pub fn shapes(&self) -> [HandShape; 2] {
        self.hands.each_ref().map(|h| HandShape {
            wrist: h.wrist,
            knuckles: h.knuckles,
            palm: h.palm,
            across: h.across,
            down: h.down,
        })
    }

    /// Hand `side`'s bends under an edge: the fingers curled under it, the thumb along the face
    /// the palm is on.
    pub(super) fn under_edge(&self, side: usize) -> Bends {
        self.cached(side, EDGE, |hand| {
            std::array::from_fn(|k| if k == 0 { [0.0; 3] } else { hand.under(k) })
        })
    }
}

impl Hand {
    /// Digit `k` closed under an edge where `HandShape::rest` puts it (a finger's thickness out
    /// of the palm and past the knuckles), the face beyond it across the palm's end: each bone
    /// stops as it comes within a finger's half thickness of that face, beneath it.
    fn under(&self, k: usize) -> [f64; 3] {
        let edge = self.knuckles + (self.palm - self.down) * FINGER;
        self.closed(k, |p, i| {
            let q = p[i + 1] - edge;
            q.dot(self.palm) > 0.0 && q.dot(self.down) < FINGER
        })
    }
}
