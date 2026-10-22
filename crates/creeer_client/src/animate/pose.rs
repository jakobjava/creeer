//! A pose before its bones (G1.4a): the pelvis, each joint's turn and each ankle with its sole's
//! normal, which every way of moving (standing, stepping, in the air, in the water) makes and
//! which the legs' inverse kinematics turn into bones. Kept in the frame of the body's feet, so
//! a change between two ways of moving is smoothed by inertialization (`inertial.rs`) without
//! the body's own motion counting as an offset.

use super::inertial::{BLEND, Curve, spin};
use super::skeleton;
use creeer_math::Frame64;
use creeer_math::glam::{DQuat, DVec3};
use creeer_sim::character::bones;

/// What the body is doing: a change between these is a transition.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Standing,
    Stepping,
    Airborne,
    Swimming,
}

/// A pose: the pelvis in the frame of the feet (`base`), the joints' turns, and each ankle with
/// the normal its sole is turned to in the world (the feet belong to the ground: carried in the
/// body's frame, a foot smoothed across a change would move with the body).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub root: Frame64,
    pub turns: [DQuat; 14],
    pub ankles: [(DVec3, DVec3); 2],
}

impl Pose {
    /// `world` (the pelvis and ankles in the world) in the frame `base`.
    pub fn local(
        base: &Frame64,
        root: Frame64,
        turns: [DQuat; 14],
        ankles: [(DVec3, DVec3); 2],
    ) -> Self {
        let back = base.rotation.inverse();
        Self {
            root: Frame64::new(back * (root.position - base.position), back * root.rotation),
            turns,
            ankles,
        }
    }

    /// The bones, the pose put in `base`, the legs reaching their ankles (`forward` and `up` the
    /// body's).
    pub fn bones(&self, base: &Frame64, forward: DVec3, up: DVec3) -> Vec<Frame64> {
        let root = Frame64::new(
            base.position + base.rotation * self.root.position,
            base.rotation * self.root.rotation,
        );
        let joints = bones::joints();
        let mut out = skeleton::posed(root, &self.turns);
        for side in 0..2 {
            let hip = root.transform_point(joints[skeleton::HIP[side]].2);
            let (ankle, n) = self.ankles[side];
            let n = n.normalize();
            let (leg, _) = skeleton::leg(side, hip, ankle, forward, up, n);
            out[skeleton::THIGH[side]] = leg[0];
            out[skeleton::SHANK[side]] = leg[1];
            out[skeleton::FOOT[side]] = leg[2];
        }
        out
    }
}

/// The offsets a transition is decaying.
#[derive(Clone, Debug)]
struct Offsets {
    root: (Curve, Curve),
    turns: [Curve; 14],
    ankles: [(Curve, Curve); 2],
    /// Time since the transition (s).
    since: f64,
}

impl Offsets {
    /// The offsets of `old` from `new` at a transition, each starting at its velocity: `old`
    /// going on from `was` in `before` seconds, less `new` going on to `next` in `after`.
    fn new(was: &Pose, old: &Pose, before: f64, new: &Pose, next: &Pose, after: f64) -> Self {
        let rate = |a: DVec3, b: DVec3, dt: f64| (b - a) / dt.max(1e-9);
        let vector = |(a, b): (DVec3, DVec3), (c, d): (DVec3, DVec3)| {
            Curve::vector(b - c, rate(a, b, before) - rate(c, d, after), BLEND)
        };
        let rotation = |(a, b): (DQuat, DQuat), (c, d): (DQuat, DQuat)| {
            Curve::rotation(b, c, spin(a, b, before) - spin(c, d, after), BLEND)
        };
        Self {
            root: (
                vector(
                    (was.root.position, old.root.position),
                    (new.root.position, next.root.position),
                ),
                rotation(
                    (was.root.rotation, old.root.rotation),
                    (new.root.rotation, next.root.rotation),
                ),
            ),
            turns: std::array::from_fn(|j| {
                rotation((was.turns[j], old.turns[j]), (new.turns[j], next.turns[j]))
            }),
            ankles: std::array::from_fn(|s| {
                let (w, o, n, x) = (was.ankles[s], old.ankles[s], new.ankles[s], next.ankles[s]);
                (
                    vector((w.0, o.0), (n.0, x.0)),
                    vector((w.1, o.1), (n.1, x.1)),
                )
            }),
            since: 0.0,
        }
    }
}

/// `last`, going on as it went from `before` (`between` seconds earlier), `dt` on, its ankles
/// carried `carry` more.
fn carried(before: &Pose, last: &Pose, between: f64, dt: f64, carry: DVec3) -> Pose {
    let on = |a: DVec3, b: DVec3| b + (b - a) * (dt / between.max(1e-9));
    let turn =
        |a: DQuat, b: DQuat| (DQuat::from_scaled_axis(spin(a, b, between) * dt) * b).normalize();
    Pose {
        root: Frame64::new(
            on(before.root.position, last.root.position),
            turn(before.root.rotation, last.root.rotation),
        ),
        turns: std::array::from_fn(|j| turn(before.turns[j], last.turns[j])),
        ankles: std::array::from_fn(|s| {
            let ((p0, n0), (p1, n1)) = (before.ankles[s], last.ankles[s]);
            (on(p0, p1) + carry, on(n0, n1).normalize_or(n1))
        }),
    }
}

/// Each character's poses smoothed across its transitions.
#[derive(Clone, Debug, Default)]
pub struct Smoother {
    /// The last two poses drawn, newest last, and the time between them.
    drawn: Option<(Pose, Pose, f64)>,
    /// A transition begun last frame: the pose drawn before it, the pose drawn at it (the old
    /// motion carried on) and the time between, and the new pose then.
    begun: Option<(Pose, Pose, f64, Pose)>,
    offsets: Option<Offsets>,
}

impl Smoother {
    /// The pose to draw: `pose`, and from a `transition` on, the old pose's offset from it
    /// decaying over [`BLEND`] (`dt` since the last). The offset starts at the old pose's
    /// velocity less the new one's (Holden 2021), so nothing changes speed at once: the new
    /// pose's velocity known a frame on, the old motion is carried on for that frame. The
    /// ankles are in the world, so a foot planted through the change has no offset to decay but
    /// its roll (onto its toe, or flat); leaving the ground, they are carried `carry` with the
    /// body as the change begins.
    pub fn draw(&mut self, pose: Pose, transition: bool, dt: f64, carry: DVec3) -> Pose {
        if transition && let Some((before, last, between)) = &self.drawn {
            let out = carried(before, last, *between, dt, carry);
            self.begun = Some((*last, out, dt, pose));
            self.offsets = None;
            self.drawn = Some((*last, out, dt));
            return out;
        }
        if let Some((was, old, before, new)) = self.begun.take() {
            let mut o = Offsets::new(&was, &old, before, &new, &pose, dt);
            o.since = dt;
            self.offsets = Some(o);
        }
        let mut out = pose;
        if let Some(o) = &mut self.offsets {
            let t = o.since;
            o.since += dt;
            out.root.position += o.root.0.offset(t);
            out.root.rotation = (o.root.1.turn(t) * out.root.rotation).normalize();
            for (turn, curve) in out.turns.iter_mut().zip(&o.turns) {
                *turn = (curve.turn(t) * *turn).normalize();
            }
            for (ankle, (p, n)) in out.ankles.iter_mut().zip(&o.ankles) {
                ankle.0 += p.offset(t);
                ankle.1 = (ankle.1 + n.offset(t)).normalize_or(ankle.1);
            }
            if o.since >= BLEND {
                self.offsets = None;
            }
        }
        let before = self.drawn.as_ref().map_or(out, |d| d.1);
        self.drawn = Some((before, out, dt));
        out
    }
}

/// The ankles with the legs free of the ground: each at `place(side, time)` from its place at
/// rest, in the body's frame `root`, the soles turned to `up`.
pub fn free(
    root: Frame64,
    place: impl Fn(f64, f64) -> DVec3,
    time: f64,
    up: DVec3,
) -> [(DVec3, DVec3); 2] {
    let joints = bones::joints();
    std::array::from_fn(|side| {
        #[allow(clippy::cast_precision_loss)] // 0 or 1
        let ankle =
            root.transform_point(joints[skeleton::ANKLE[side]].2 + place(side as f64, time));
        (ankle, up)
    })
}
