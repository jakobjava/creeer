//! The animation against its measures (G1.3g, `docs/g1-plan.md`): standing still it is the
//! model at rest; walking up and down slopes to the friction angle its soles lie on the ground
//! within 1 cm and a planted foot does not slide; its steps come at the measured cadence.

use super::feet::{self, Ground};
use super::*;
use creeer_math::det;

/// A plane through the origin of normal `normal`, and water beneath `sea` (m) if any.
struct Plane {
    normal: DVec3,
    sea: f64,
}

impl Ground for Plane {
    fn under(&self, at: DVec3, up: DVec3) -> Option<(DVec3, DVec3)> {
        let t = at.dot(self.normal) / up.dot(self.normal);
        (t.abs() < 2.0).then(|| (at - up * t, self.normal))
    }
    fn depth(&self, at: DVec3) -> f64 {
        self.sea - at.y
    }
}

/// How fast the walker sets off (m/s²).
const ACCELERATION: f64 = 2.5;

/// The sole's corners at rest under each foot (m): on its back and front edges, inside and out.
fn soles(side: usize) -> [DVec3; 4] {
    let c = bones::centres()[skeleton::FOOT[side]];
    let (x, z) = (0.05, [feet::HEEL_EDGE, feet::TOE_EDGE]);
    [
        DVec3::new(c.x - x, 0.0, z[0]),
        DVec3::new(c.x + x, 0.0, z[0]),
        DVec3::new(c.x - x, 0.0, z[1]),
        DVec3::new(c.x + x, 0.0, z[1]),
    ]
}

#[test]
fn standing_still_is_the_model_at_rest() {
    let flat = Plane {
        normal: DVec3::Y,
        sea: -100.0,
    };
    let centre = DVec3::new(2.0, creeer_sim::character::heights().0, -1.0);
    let body = Body::Standing(centre, DQuat::IDENTITY);
    let mut animator = Animator::default();
    for _ in 0..30 {
        animator.bones(1, &body, 1.0 / 60.0, &flat);
    }
    let bones = animator.bones(1, &body, 1.0 / 60.0, &flat);
    let rest = bones::standing_bones(centre, DQuat::IDENTITY);
    for (k, (b, r)) in bones.iter().zip(&rest).enumerate() {
        let off = (b.position - r.position).length() + b.rotation.angle_between(r.rotation);
        assert!(off < 1e-6, "bone {k} off by {off}");
    }
}

/// What a walk measured.
#[derive(Debug, Default)]
struct Walked {
    /// The farthest a planted foot's nearest corner lay from the ground, and the deepest any
    /// corner went into it (m).
    off: f64,
    under: f64,
    /// The most a corner touching the ground moved while it touched (m).
    slid: f64,
    /// Feet set down.
    steps: usize,
    /// How far the hips were lowered: at most, and its range over the last half (m).
    crouch: f64,
    bob: f64,
}

/// Walks the body `seconds` over ground sloping `slope` (rad) up its way at `speed`.
fn walk(slope: f64, speed: f64, seconds: f64) -> Walked {
    let (s, c) = det::sin_cos(slope);
    let ground = Plane {
        normal: DVec3::new(0.0, c, -s),
        sea: -100.0,
    };
    let along = DVec3::new(0.0, s, c);
    let height = creeer_sim::character::heights().0;
    let dt = 1.0 / 60.0;
    let mut animator = Animator::default();
    let mut out = Walked::default();
    let mut touching: [[Option<DVec3>; 4]; 2] = [[None; 4]; 2];
    let mut was_down = [true; 2];
    let (mut low, mut high) = (f64::INFINITY, 0.0f64);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = (seconds / dt) as usize;
    for frame in 0..frames {
        #[allow(clippy::cast_precision_loss)]
        let t = frame as f64 * dt;
        // The body upright, its feet's point on the slope, setting off at a brisk 2.5 m/s² (a
        // person's start) and going on at `speed`.
        let ramp = speed / ACCELERATION;
        let travelled = if t < ramp {
            0.5 * ACCELERATION * t * t
        } else {
            0.5 * speed * ramp + speed * (t - ramp)
        };
        let body = Body::Standing(along * travelled + DVec3::Y * height, DQuat::IDENTITY);
        let bones = animator.bones(7, &body, dt, &ground);
        let state = &animator.states[&7];
        out.crouch = out.crouch.max(state.drop);
        if t > 0.5 * seconds {
            (low, high) = (low.min(state.drop), high.max(state.drop));
        }
        let Some(feet_now) = state.feet else { continue };
        for side in 0..2 {
            let down = feet_now[side].down;
            if down && !was_down[side] {
                out.steps += 1;
            }
            was_down[side] = down;
            let foot = bones[skeleton::FOOT[side]];
            let heights = soles(side).map(|p| foot.transform_point(p).dot(ground.normal));
            out.under = out.under.max(-heights.iter().copied().fold(0.0, f64::min));
            if down {
                let nearest = heights.iter().copied().fold(f64::INFINITY, f64::min);
                out.off = out.off.max(nearest.abs());
            }
            // A corner on the ground stays where it touched down while it touches.
            for (k, p) in soles(side).into_iter().enumerate() {
                let at = foot.transform_point(p);
                if heights[k].abs() > 1e-5 {
                    touching[side][k] = None;
                    continue;
                }
                match touching[side][k] {
                    Some(first) => out.slid = out.slid.max((at - first).length()),
                    None => touching[side][k] = Some(at),
                }
            }
        }
    }
    out.bob = high - low;
    out
}

#[test]
fn feet_stand_on_slopes_to_the_friction_angle_and_never_slide() {
    // The boots' friction, 0.8: slopes to 38.7°, up and down.
    let steepest = det::atan(0.8);
    for slope in [0.0, 0.2, 0.45, steepest, -0.3, -steepest] {
        for speed in [0.8, 1.4, 3.0] {
            // Near the friction angle a person can only walk: the legs' push, at most μN, holds
            // them on the slope and little more (`character::steer`).
            if slope.abs() > 0.5 && speed > gait::transition_speed() {
                continue;
            }
            let w = walk(slope, speed, 6.0);
            println!(
                "slope {:+.1}°, {speed} m/s: a planted sole {:.1} mm off the ground, {:.1} mm into it, \
                 slid {:.2} mm, {} steps, hips down {:.0} mm at most, bobbing {:.0} mm",
                slope.to_degrees(),
                w.off * 1e3,
                w.under * 1e3,
                w.slid * 1e3,
                w.steps,
                w.crouch * 1e3,
                w.bob * 1e3
            );
            assert!(w.off < 0.01, "slope {slope}, {speed} m/s: {w:?}");
            assert!(w.under < 0.01, "slope {slope}, {speed} m/s: {w:?}");
            assert!(w.slid < 1e-3, "slope {slope}, {speed} m/s: {w:?}");
            assert!(w.steps > 4, "{w:?}");
        }
    }
}

#[test]
fn it_steps_at_the_measured_cadence() {
    // Walking: 114 steps a minute at 1.4 m/s, 0.74 m each (the walk ratio).
    let g = gait::at(1.4);
    assert!(
        (g.cadence * 60.0 - 113.7).abs() < 0.1,
        "{}",
        g.cadence * 60.0
    );
    assert!((g.step_length(1.4) - 0.739).abs() < 0.001);
    // Running at 3.5 m/s: 2 × (75.0 + 3.0 × 3.5) = 171 steps a minute; the change at Fr 0.5.
    let run = gait::at(3.5);
    assert!((run.cadence * 60.0 - 171.1).abs() < 0.5);
    assert!(run.running > 0.999 && gait::at(1.8).running < 1e-9);
    let t = gait::transition_speed();
    assert!((t * t / (9.806_65 * gait::leg_length()) - 0.5).abs() < 1e-12);
    // Walked: the feet land as often as the gait says.
    let steps = walk(0.0, 1.4, 10.0).steps;
    let expected = 113.7 / 60.0 * (10.0 - 0.5);
    #[allow(clippy::cast_precision_loss)]
    let steps = steps as f64;
    assert!(
        (steps - expected).abs() <= 2.0,
        "{steps} steps, {expected} expected"
    );
}

/// The budget (G1.3g): ≤ 0.05 ms of CPU a character a frame. Measured here on a plane (the
/// animation's own arithmetic); the planet's ground queries are timed in `people_ground`.
#[test]
fn a_character_animates_within_its_budget() {
    let ground = Plane {
        normal: DVec3::Y,
        sea: -100.0,
    };
    let height = creeer_sim::character::heights().0;
    let mut animator = Animator::default();
    let dt = 1.0 / 60.0;
    let frames = 6000u32;
    let start = std::time::Instant::now();
    for frame in 0..frames {
        let t = f64::from(frame) * dt;
        let body = Body::Standing(DVec3::new(0.0, height, 1.4 * t), DQuat::IDENTITY);
        std::hint::black_box(animator.bones(3, &body, dt, &ground));
    }
    let each = start.elapsed().as_secs_f64() * 1e3 / f64::from(frames);
    println!("a walking character animates in {each:.4} ms a frame");
    // Debug builds run slower; the budget holds by a wide margin in release.
    assert!(each < 0.05, "{each} ms");
}

#[test]
fn standing_up_blends_from_the_ragdoll() {
    let flat = Plane {
        normal: DVec3::Y,
        sea: -100.0,
    };
    let centre = DVec3::new(0.0, creeer_sim::character::heights().0, 0.0);
    // Lying: every bone turned on its side, half a metre down.
    let lying: Vec<Frame64> = bones::standing_bones(centre, DQuat::IDENTITY)
        .iter()
        .map(|b| Frame64::new(b.position - DVec3::Y * 0.5, DQuat::from_rotation_z(1.5)))
        .collect();
    let mut animator = Animator::default();
    let dt = 1.0 / 60.0;
    let first = animator.bones(2, &Body::Fallen(lying.clone()), dt, &flat);
    assert_eq!(first, lying, "fallen, the bones are the ragdoll's exactly");
    // Up: the first frame still nearly the ragdoll, then the standing pose by 0.8 s.
    let up = Body::Standing(centre, DQuat::IDENTITY);
    let start = animator.bones(2, &up, dt, &flat);
    assert!((start[0].position - lying[0].position).length() < 0.01);
    for _ in 0..60 {
        animator.bones(2, &up, dt, &flat);
    }
    let stood = animator.bones(2, &up, dt, &flat);
    let rest = bones::standing_bones(centre, DQuat::IDENTITY);
    assert!((stood[0].position - rest[0].position).length() < 1e-6);
}

#[test]
fn a_swing_raises_and_strikes_with_the_right_arm() {
    let flat = Plane {
        normal: DVec3::Y,
        sea: -100.0,
    };
    let centre = DVec3::new(0.0, creeer_sim::character::heights().0, 0.0);
    let body = Body::Standing(centre, DQuat::IDENTITY);
    let mut animator = Animator::default();
    let dt = 1.0 / 60.0;
    animator.bones(4, &body, dt, &flat);
    let hand = |animator: &mut Animator, progress| {
        animator.swing(4, progress);
        let b = animator.bones(4, &body, dt, &flat);
        b[5].transform_point(bones::centres()[5])
    };
    let raised = hand(&mut animator, Some(0.0));
    let struck = hand(&mut animator, Some(1.0));
    let still = hand(&mut animator, None);
    assert!(raised.y > centre.y + 0.6, "raised overhead: {raised}");
    assert!(struck.z > still.z + 0.2, "struck down in front: {struck}");
}
