//! The arms reaching what the hands hold (G2.10, ADR-0036), on the engineer's hands as
//! exported: each hand within a centimetre of where it holds, in one hand or two, on a pole and
//! on a box, and low before the feet with the body stooped to it; beyond reach, the arm as
//! straight as it goes and nothing undefined; reaching out without a jump.

use super::super::feet::Ground;
use super::super::grasp::Load;
use super::super::hands::{HandShape, Hands, Hold};
use super::super::skeleton::{FOOT, SHOULDER};
use super::super::{Animator, Body};
use super::*;
use creeer_art::Palette;
use creeer_art::model::Model;
use creeer_math::glam::DQuat;
use creeer_sim::{carry, character};

/// The ground at y = 0.
struct Flat;

impl Ground for Flat {
    fn under(&self, at: DVec3, up: DVec3) -> Option<(DVec3, DVec3)> {
        Some((at - up * at.y, DVec3::Y))
    }
    fn depth(&self, _: DVec3) -> f64 {
        -100.0
    }
}

const KEY: u32 = 1;
const DT: f64 = 1.0 / 60.0;

/// The engineer's hands, as the game loads them.
fn engineer() -> Hands {
    let data = crate::content::data();
    let palette = Palette::from_data(&data).expect("palette");
    let bytes = data.binary("models/engineer.glb").expect("the engineer");
    Hands::new(&Model::read(bytes, &palette).expect("model").bones).expect("fingers")
}

/// Ada standing at `centre` facing +z: her body.
fn centre() -> DVec3 {
    DVec3::new(0.0, character::heights().0, 0.0)
}

/// A load whose point `grip` (its frame) is held at `at` (the world), turned `turn`.
fn load(half: DVec3, grip: DVec3, at: DVec3, turn: DQuat, one_hand: bool) -> Load {
    Load {
        id: 7,
        frame: Frame64::new(at - turn * grip, turn),
        centre: DVec3::ZERO,
        half,
        grip,
        one_hand,
    }
}

/// Holding `load` for `frames`: the animator, the last bones, and each hand's distance from
/// where it holds (m; none for a hand that does not).
fn hold(load: Load, frames: usize) -> (Animator, Vec<Frame64>, [Option<f64>; 2]) {
    let mut animator = Animator::default();
    animator.set_hands(engineer().shapes());
    let body = Body::Standing(centre(), DQuat::IDENTITY);
    animator.bones(KEY, &body, DT, &Flat);
    let mut bones = Vec::new();
    for _ in 0..frames {
        animator.holding(KEY, Some(load));
        bones = animator.bones(KEY, &body, DT, &Flat);
    }
    let state = &animator.states[&KEY];
    let off = std::array::from_fn(|side| {
        let grasp = state.grasps[side]?;
        let rest = state.hands[side].rest(grasp.hold);
        let drawn = bones[[5, 11][side]] * rest;
        Some((drawn.position - (load.frame * grasp.frame).position).length())
    });
    (animator, bones, off)
}

/// The engineer's hold at elbow height, a forearm before him (`carry::held`), so far to his
/// right (m).
fn before(right: f64) -> DVec3 {
    carry::held(centre(), DVec3::Y, DVec3::Z) - DVec3::X * right
}

#[test]
fn one_hand_holds_a_stick_where_it_is_held() {
    // A 1.2 m stick, 30 mm round, held near its end on its top, pointing ahead.
    let (half, grip) = (DVec3::new(0.6, 0.015, 0.015), DVec3::new(-0.45, 0.015, 0.0));
    let turn = DQuat::from_rotation_y(-std::f64::consts::FRAC_PI_2);
    let stick = load(half, grip, before(0.15), turn, true);
    let (animator, bones, off) = hold(stick, 60);
    let right = off[0].expect("the right hand holds");
    assert!(off[1].is_none(), "one hand");
    assert_eq!(animator.holds(KEY), [Hold::Grip(0.015), Hold::Relaxed]);
    // Its handle's axis amid the fingers on the stick's axis under the point held, a radius
    // from it.
    let hand = bones[5] * animator.states[&KEY].hands[0].rest(Hold::Grip(0.015));
    let from_grip = (hand.position - stick.held()).length();
    println!(
        "one hand: {:.1} mm from where it holds, {:.1} mm from the point held",
        right * 1e3,
        from_grip * 1e3
    );
    assert!(right < 0.01, "{right} m");
    assert!(from_grip < 0.015 + 0.01, "{from_grip} m");
    // Its handle along the stick.
    let along = (hand.rotation * DVec3::Y).dot(turn * DVec3::X).abs();
    assert!(along > 0.999, "the handle {along} along the stick");
}

#[test]
fn two_hands_hold_a_box_under_its_sides_and_a_pole_along_it() {
    // A 0.28 m box held at its top's middle, off the ground: the fingers under its bottom edges.
    let half = DVec3::splat(0.14);
    let held = load(
        half,
        DVec3::new(0.0, 0.14, 0.0),
        before(0.0),
        DQuat::IDENTITY,
        false,
    );
    let (animator, bones, off) = hold(held, 60);
    let [r, l] = off.map(|o| o.expect("both hands"));
    assert_eq!(animator.holds(KEY), [Hold::Edge; 2]);
    // On its two sides across the body, at its bottom.
    let at =
        |s: usize| (bones[[5, 11][s]] * animator.states[&KEY].hands[s].rest(Hold::Edge)).position;
    let bottom = held.frame.position.y - 0.14;
    assert!(
        (at(0).x + 0.14).abs() < 0.01 && (at(1).x - 0.14).abs() < 0.01,
        "{} {}",
        at(0),
        at(1)
    );
    assert!(
        (at(0).y - bottom).abs() < 0.02,
        "{} at the bottom {bottom}",
        at(0)
    );
    // A 2 m pole, 60 mm round, held across the body at its middle: the hands 0.4 m apart on it.
    let pole = load(
        DVec3::new(1.0, 0.03, 0.03),
        DVec3::new(0.0, 0.03, 0.0),
        before(0.0),
        DQuat::IDENTITY,
        false,
    );
    let (animator, bones, on_pole) = hold(pole, 60);
    let [pr, pl] = on_pole.map(|o| o.expect("both hands"));
    let rest = animator.states[&KEY]
        .hands
        .map(|h| h.rest(Hold::Grip(0.025)));
    let apart = ((bones[5] * rest[0]).position - (bones[11] * rest[1]).position).length();
    println!(
        "two hands: box {:.1} and {:.1} mm, pole {:.1} and {:.1} mm from where they hold, {apart:.3} m apart",
        r * 1e3,
        l * 1e3,
        pr * 1e3,
        pl * 1e3
    );
    for o in [r, l, pr, pl] {
        assert!(o < 0.01, "{o} m");
    }
    assert!((apart - carry::HANDS_APART).abs() < 0.01, "{apart} m apart");
}

#[test]
fn held_low_before_the_feet_the_body_stoops_and_squats_to_it() {
    // The box on the ground 0.45 m before the feet, held at its top (the server's hands reach
    // it from shoulders stooped 0.7 m down and 0.35 m on): flat on its sides, the fingers not
    // under it on the ground.
    let half = DVec3::splat(0.14);
    let on_ground = load(
        half,
        DVec3::new(0.0, 0.14, 0.0),
        DVec3::new(0.0, 0.28, 0.45),
        DQuat::IDENTITY,
        false,
    );
    let (animator, bones, off) = hold(on_ground, 90);
    let [r, l] = off.map(|o| o.expect("both hands"));
    assert_eq!(animator.holds(KEY), [Hold::Open; 2]);
    let state = &animator.states[&KEY];
    let (lean, drop) = (state.stoop[0], state.stoop[2]);
    let standing = bones::joints()[SHOULDER[0]].2.y;
    let shoulders = bones[1].transform_point(bones::joints()[SHOULDER[0]].2).y;
    // The soles on the ground: each foot's ankle at its height.
    let ankles = FOOT.map(|f| bones[f].transform_point(bones::joints()[f - 1].2).y);
    println!(
        "low: hands {:.1} and {:.1} mm from where they hold; leant {:.0}°, hips {:.2} m down, shoulders {:.2} m down; ankles at {:.3} and {:.3} m",
        r * 1e3,
        l * 1e3,
        lean.to_degrees(),
        drop,
        standing - shoulders,
        ankles[0],
        ankles[1]
    );
    for o in [r, l] {
        assert!(o < 0.01, "{o} m");
    }
    assert!(
        lean > 0.3 && drop > 0.3,
        "stooped {lean} rad, squatted {drop} m"
    );
    for a in ankles {
        assert!((a - 0.111).abs() < 0.01, "an ankle {a} m up");
    }
}

#[test]
fn beyond_reach_the_arm_stretches_towards_it_and_stops() {
    // Torn 1.5 m away at chest height, and held at the shoulder itself: nothing undefined.
    let half = DVec3::splat(0.14);
    let far = load(
        half,
        DVec3::new(0.0, 0.14, 0.0),
        DVec3::new(0.0, 1.3, 1.5),
        DQuat::IDENTITY,
        false,
    );
    let (_, bones, off) = hold(far, 60);
    let [r, l] = off.map(|o| o.expect("both hands"));
    assert!(
        bones
            .iter()
            .all(|b| b.position.is_finite() && b.rotation.is_finite())
    );
    // The upper arm and forearm in a line, pointing at it: short by about the gap.
    let joints = bones::joints();
    let (s, e, w) = (joints[2].2, joints[3].2, joints[4].2);
    let (sw, ew, ww) = (
        bones[3].transform_point(s),
        bones[4].transform_point(e),
        bones[4].transform_point(w),
    );
    let straight = (ew - sw).normalize().dot((ww - ew).normalize());
    println!("beyond reach: {r:.2} and {l:.2} m short, the arm straight to {straight:.4}");
    assert!(r > 0.5 && l > 0.5, "{r} {l}");
    assert!(straight > 0.99, "{straight}");
    let torso = Frame64::IDENTITY;
    let at_shoulder = Frame64::new(s, DQuat::IDENTITY);
    let hand = HandShape::approximate(0).rest(Hold::Edge);
    let (arm, short) = arm(0, &torso, (hand, at_shoulder), DVec3::Y, DVec3::Z);
    assert!(
        short.is_finite()
            && arm
                .iter()
                .all(|b| b.position.is_finite() && b.rotation.is_finite())
    );
}

#[test]
fn the_arms_reach_out_and_come_back_without_a_jump() {
    let half = DVec3::splat(0.14);
    let held = load(
        half,
        DVec3::new(0.0, 0.14, 0.0),
        before(0.0),
        DQuat::IDENTITY,
        false,
    );
    let mut animator = Animator::default();
    animator.set_hands(engineer().shapes());
    let body = Body::Standing(centre(), DQuat::IDENTITY);
    let hand = |b: Vec<Frame64>| b[5].transform_point(bones::centres()[5]);
    let mut last = hand(animator.bones(KEY, &body, DT, &Flat));
    let mut fastest = 0.0_f64;
    // Held for a second, then let go.
    for frame in 0..120 {
        animator.holding(KEY, (frame < 60).then_some(held));
        let at = hand(animator.bones(KEY, &body, DT, &Flat));
        fastest = fastest.max((at - last).length() / DT);
        last = at;
    }
    println!("reaching out and back the hand moves at {fastest:.2} m/s at most");
    // As fast as hands reach (peak speeds 1.5–2 m/s), never a jump.
    assert!(fastest < 2.0, "{fastest} m/s");
    assert_eq!(animator.holds(KEY), [Hold::Relaxed; 2]);
}

/// The cost of holding (G1.3g's budget: 0.05 ms of CPU a character a frame): walking with a box
/// in both hands against walking with none, on a plane (the planet's ground queries are timed in
/// `people_ground`).
#[test]
fn a_character_holding_a_box_animates_within_its_budget() {
    let frames = 6000u32;
    let time = |holding: bool| {
        let mut animator = Animator::default();
        animator.set_hands(engineer().shapes());
        let start = std::time::Instant::now();
        for frame in 0..frames {
            let t = f64::from(frame) * DT;
            let at = centre() + DVec3::Z * (1.4 * t);
            let grip = carry::held(at, DVec3::Y, DVec3::Z);
            let box_ = load(
                DVec3::splat(0.14),
                DVec3::new(0.0, 0.14, 0.0),
                grip,
                DQuat::IDENTITY,
                false,
            );
            animator.holding(KEY, holding.then_some(box_));
            std::hint::black_box(animator.bones(
                KEY,
                &Body::Standing(at, DQuat::IDENTITY),
                DT,
                &Flat,
            ));
        }
        start.elapsed().as_secs_f64() * 1e3 / f64::from(frames)
    };
    // The least of five rounds each, taken in turn (the machine's other work comes and goes).
    let (mut walking, mut holding) = (f64::INFINITY, f64::INFINITY);
    for _ in 0..5 {
        walking = walking.min(time(false));
        holding = holding.min(time(true));
    }
    println!(
        "walking {walking:.4} ms a frame, holding a box {holding:.4} ms: {:.4} ms more",
        holding - walking
    );
    // Debug builds run slower; the budget holds by a wide margin in release.
    assert!(holding < 0.05, "{holding} ms");
}

#[test]
fn an_arm_reaching_its_own_hand_at_rest_is_nearly_the_model_at_rest() {
    let hand = HandShape::approximate(0).rest(Hold::Grip(0.02));
    let (arm, short) = arm(0, &Frame64::IDENTITY, (hand, hand), DVec3::Y, DVec3::Z);
    let elbow = bones::joints()[3].2;
    let off = (arm[0].transform_point(elbow) - elbow).length();
    println!("at rest: the elbow {:.1} mm off the model's", off * 1e3);
    assert!(short < 1e-9, "{short}");
    assert!((arm[2].position - Frame64::IDENTITY.position).length() < 1e-9);
    assert!(off < 0.03, "the elbow {off} m off");
}
