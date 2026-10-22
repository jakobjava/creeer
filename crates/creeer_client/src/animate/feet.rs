//! The feet (G1.3g): each foot's turn on the ground and in the air through a stride. On the
//! ground a foot stays where it landed, planted, so it never slides; in the air it travels on
//! an arc from where it lifted to where it will land, which is found ahead of the body by
//! its speed and the time left (Johansen 2009's "footbase" placed on the ground the leg finds).

use super::gait::Gait;
use creeer_math::det;
use creeer_math::glam::DVec3;

/// How high a foot lifts at the top of its swing, walking and running (m).
const LIFT: (f64, f64) = (0.09, 0.16);
/// How fast a swinging foot's aim follows the place ahead of it, per swing (a thirtieth of
/// a swing behind it at mid-swing).
const SETTLING: f64 = 30.0;

/// What the feet stand on.
pub trait Ground {
    /// The ground under `at` along `-up`, within a metre either side: its point and normal.
    fn under(&self, at: DVec3, up: DVec3) -> Option<(DVec3, DVec3)>;
    /// How deep `at` is under water (m; negative above it, or where there is none).
    fn depth(&self, at: DVec3) -> f64;
}

/// One foot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Foot {
    /// Where it stands (on the ground, under its ankle) and the ground's normal there.
    pub plant: DVec3,
    pub normal: DVec3,
    /// Where it lifted from, for its swing.
    pub lifted: DVec3,
    pub lifted_normal: DVec3,
    /// On the ground now.
    pub down: bool,
    /// Lifted flat, from standing (the first step setting off): it leaves the ground without
    /// rolling onto its toe, which a foot at rest has not done.
    pub from_rest: bool,
    /// The share of the stride it stands, as the gait was when it landed; where through the
    /// stride it lifted, and how high it swings, as the gait was then. Each step keeps the gait
    /// it began with: a change of gait (walking to running, and back) shows in the next step,
    /// as people change over a stride, not half-way through a step.
    pub duty: f64,
    pub lift_at: f64,
    pub lift: f64,
    /// Where its swing aims it, and how far through the swing it was last: it comes down
    /// there, where it was drawn going.
    pub aimed: (DVec3, DVec3),
    pub swung: f64,
}

/// Where a foot is through its step: standing (0 landing to 1 lifting) or swinging (0 lifting
/// to 1 landing).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Through {
    Stance(f64),
    Swing(f64),
}

impl Foot {
    /// A foot standing at `at` on ground of normal `n`, walking.
    pub fn planted(at: DVec3, n: DVec3) -> Self {
        Self {
            plant: at,
            normal: n,
            lifted: at,
            lifted_normal: n,
            down: true,
            from_rest: false,
            duty: super::gait::WALK_DUTY,
            lift_at: super::gait::WALK_DUTY,
            lift: LIFT.0,
            aimed: (at, n),
            swung: 0.0,
        }
    }

    /// The foot at its point of the stride `along` (0 where it lands) aiming to land at
    /// `landing` (on ground of normal `n`): its sole's point under the ankle, the normal it is
    /// turned to, and where it is through its step.
    pub fn step(
        &mut self,
        along: f64,
        gait: &Gait,
        landing: (DVec3, DVec3),
        up: DVec3,
    ) -> ((DVec3, DVec3), Through) {
        if self.down && along < self.duty {
            return (
                (self.plant, self.normal),
                Through::Stance(along / self.duty),
            );
        }
        if self.down {
            (self.lifted, self.lifted_normal) = (self.plant, self.normal);
            self.down = false;
            self.lift_at = along;
            self.lift = LIFT.0 + (LIFT.1 - LIFT.0) * gait.running;
            (self.aimed, self.swung) = (landing, 0.0);
        } else if along < self.lift_at {
            // The stride come round: down where it was aimed.
            (self.plant, self.normal) = self.aimed;
            self.down = true;
            self.from_rest = false;
            self.duty = gait.duty;
            return (
                (self.plant, self.normal),
                Through::Stance((along / self.duty).min(1.0)),
            );
        }
        let s = ((along - self.lift_at) / (1.0 - self.lift_at)).clamp(0.0, 1.0);
        // Where it lands is chosen through the first half of the swing and settled through the
        // second (people's next foot placement is mostly set by mid-swing: Wang and Srinivasan
        // 2014): the aim follows the place ahead less and less, not at all as it lands.
        let follow =
            1.0 - det::exp(-SETTLING * (s - self.swung).max(0.0) * (2.0 - 2.0 * s).min(1.0));
        self.aimed = (
            self.aimed.0.lerp(landing.0, follow),
            self.aimed.1.lerp(landing.1, follow).normalize_or(up),
        );
        self.swung = s;
        let landing = self.aimed;
        let eased = s * s * (3.0 - 2.0 * s);
        let at = self.lifted.lerp(landing.0, eased)
            + up * self.lift * det::sin(std::f64::consts::PI * s);
        let n = self.lifted_normal.lerp(landing.1, eased).normalize_or(up);
        ((at, n), Through::Swing(s))
    }
}

/// The foot's pitch at heel strike, toe up, and at toe-off, heel up (rad): about 17° and 46°
/// between the sole and the ground, as the foot meets and leaves it in walking (Perry's
/// rockers: the heel's, the ankle's with the foot flat, then the forefoot's).
const HEEL_STRIKE: f64 = 0.3;
const TOE_OFF: f64 = 0.8;
/// The sole's back and front edges, along the foot at rest (m): what a stiff work boot rolls
/// on.
pub const HEEL_EDGE: f64 = -0.111;
pub const TOE_EDGE: f64 = 0.143;

/// The ankle and the sole's normal for a foot whose point under the ankle is `point`, on ground
/// of normal `n`, the person facing `forward`, `through` its step: rolling on its
/// heel as it lands, flat, then rising onto its toe until it leaves the ground, and in the air
/// turning from toe-down to toe-up (lifted `from_rest`, from flat).
pub fn rolled(
    side: usize,
    point: DVec3,
    n: DVec3,
    forward: DVec3,
    through: Through,
    from_rest: bool,
) -> (DVec3, DVec3) {
    use creeer_math::glam::DQuat;
    let a = creeer_sim::character::bones::joints()[super::skeleton::ANKLE[side]].2;
    let under = DVec3::new(a.x, 0.0, a.z);
    let ahead = (forward - n * forward.dot(n)).normalize_or(forward);
    let side_axis = n.cross(ahead);
    // The foot flat: x along its side, y the ground's normal, z ahead.
    let flat = |v: DVec3| side_axis * v.x + n * v.y + ahead * v.z;
    // Pitched by `pitch` about its sole's edge at `edge` along it: toe up turns ahead towards
    // the normal, about the foot's −x.
    let pitched = |pitch: f64, edge: f64| {
        let pivot = DVec3::new(a.x, 0.0, edge);
        let turn = DQuat::from_axis_angle(-side_axis, pitch);
        (
            point + flat(pivot - under) + turn * flat(a - pivot),
            turn * n,
        )
    };
    let u = match through {
        Through::Stance(u) => u,
        Through::Swing(s) => {
            // In the air: from as it left the ground (on its toe) to as it will meet it (on its
            // heel).
            let eased = s * s * (3.0 - 2.0 * s);
            let (left, left_n) = if from_rest {
                pitched(0.0, 0.0)
            } else {
                pitched(-TOE_OFF, TOE_EDGE)
            };
            let (meets, meets_n) = pitched(HEEL_STRIKE, HEEL_EDGE);
            return (
                left.lerp(meets, eased),
                left_n.lerp(meets_n, eased).normalize_or(n),
            );
        }
    };
    if u < 0.12 {
        pitched(HEEL_STRIKE * (1.0 - u / 0.12), HEEL_EDGE)
    } else if u < 0.5 {
        pitched(0.0, 0.0)
    } else {
        let x = (u - 0.5) / 0.5;
        pitched(-TOE_OFF * x * x, TOE_EDGE)
    }
}

/// Where a foot standing at `spot` lands, `ahead` of it on the ground: shortened, on ground
/// steeper than a stair, until it lands within a stair's rise of the spot (people take shorter
/// steps up and down steep ground, and the stride hurries to keep up). `ground_at` is the
/// ground within reach of a point, if any; with none even under the spot, the level there.
pub fn shortened(
    ground_at: &impl Fn(DVec3) -> Option<(DVec3, DVec3)>,
    spot: DVec3,
    ahead: DVec3,
    up: DVec3,
) -> (DVec3, DVec3) {
    let within = |p: DVec3| ground_at(p).filter(|(g, _)| (*g - spot).dot(up).abs() <= STAIR);
    if let Some(full) = within(spot + ahead) {
        return full;
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..8 {
        let mid = 0.5 * (lo + hi);
        if within(spot + ahead * mid).is_some() {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    within(spot + ahead * lo)
        .or_else(|| ground_at(spot))
        .unwrap_or((spot, up))
}

/// The most a step rises or falls from where its foot would stand (m): a stair's rise.
const STAIR: f64 = 0.18;
