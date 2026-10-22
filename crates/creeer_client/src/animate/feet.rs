//! The feet (G1.3g): each foot's turn on the ground and in the air through a stride. On the
//! ground a foot stays where it landed, planted, so it never slides; in the air it travels on
//! an arc from where it lifted to where it will land, which is found ahead of the body by
//! its speed and the time left (Johansen 2009's "footbase" placed on the ground the leg finds).

use super::gait::Gait;
use creeer_math::det;
use creeer_math::glam::DVec3;

/// How high a foot lifts at the top of its swing, walking and running (m).
const LIFT: (f64, f64) = (0.09, 0.16);

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
}

impl Foot {
    /// A foot standing at `at` on ground of normal `n`.
    pub fn planted(at: DVec3, n: DVec3) -> Self {
        Self {
            plant: at,
            normal: n,
            lifted: at,
            lifted_normal: n,
            down: true,
        }
    }

    /// The foot at its point of the stride `along` (0 where it lands, the gait's duty where it
    /// lifts) aiming to land at `landing` (on ground of normal `n`): its sole's point under the
    /// ankle and the normal it is turned to.
    pub fn step(
        &mut self,
        along: f64,
        gait: &Gait,
        landing: (DVec3, DVec3),
        up: DVec3,
    ) -> (DVec3, DVec3) {
        if along < gait.duty {
            if !self.down {
                // Down where it was aimed.
                (self.plant, self.normal) = landing;
                self.down = true;
            }
            return (self.plant, self.normal);
        }
        if self.down {
            (self.lifted, self.lifted_normal) = (self.plant, self.normal);
            self.down = false;
        }
        let s = ((along - gait.duty) / (1.0 - gait.duty)).clamp(0.0, 1.0);
        let eased = s * s * (3.0 - 2.0 * s);
        let lift = LIFT.0 + (LIFT.1 - LIFT.0) * gait.running;
        let at =
            self.lifted.lerp(landing.0, eased) + up * lift * det::sin(std::f64::consts::PI * s);
        let n = self.lifted_normal.lerp(landing.1, eased).normalize_or(up);
        (at, n)
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
/// of normal `n`, the person facing `forward`, at `along` through its stride: rolling on its
/// heel as it lands, flat, then rising onto its toe until it leaves the ground, and in the air
/// turning from toe-down to toe-up.
pub fn rolled(
    side: usize,
    point: DVec3,
    n: DVec3,
    forward: DVec3,
    along: f64,
    gait: &Gait,
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
    if along >= gait.duty {
        // In the air: from as it left the ground (on its toe) to as it will meet it (on its heel).
        let s = (along - gait.duty) / (1.0 - gait.duty);
        let eased = s * s * (3.0 - 2.0 * s);
        let (left, left_n) = pitched(-TOE_OFF, TOE_EDGE);
        let (meets, meets_n) = pitched(HEEL_STRIKE, HEEL_EDGE);
        return (
            left.lerp(meets, eased),
            left_n.lerp(meets_n, eased).normalize_or(n),
        );
    }
    let u = along / gait.duty;
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
/// steps up and down steep ground, and the stride hurries to keep up).
pub fn shortened(
    ground_at: &impl Fn(DVec3) -> (DVec3, DVec3),
    spot: DVec3,
    ahead: DVec3,
    up: DVec3,
) -> (DVec3, DVec3) {
    let full = ground_at(spot + ahead);
    if (full.0 - spot).dot(up).abs() <= STAIR {
        return full;
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..8 {
        let mid = 0.5 * (lo + hi);
        if (ground_at(spot + ahead * mid).0 - spot).dot(up).abs() <= STAIR {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    ground_at(spot + ahead * lo)
}

/// The most a step rises or falls from where its foot would stand (m): a stair's rise.
const STAIR: f64 = 0.18;
