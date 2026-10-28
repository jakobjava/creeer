//! The fingers against their measures (G1.4b), on the engineer as exported.

use super::*;
use creeer_art::Palette;
use creeer_art::model::Model;
use creeer_art::model_skin::Bone;
use creeer_sim::character::bones::{self, SEGMENT_NAMES};

/// The engineer's bones, as the game loads them.
fn engineer() -> Vec<Bone> {
    let data = crate::content::data();
    let palette = Palette::from_data(&data).expect("palette");
    let bytes = data.binary("models/engineer.glb").expect("the engineer");
    Model::read(bytes, &palette).expect("model").bones
}

/// The segments at rest (the model's own frame).
fn at_rest() -> Vec<Frame64> {
    vec![Frame64::IDENTITY; SEGMENT_NAMES.len()]
}

#[test]
fn the_engineer_has_three_bones_a_digit_in_each_hand() {
    let bones = engineer();
    for name in bones::finger_bones() {
        assert!(bones.iter().any(|b| b.name == name), "no bone {name}");
    }
    assert!(bones::skeleton_problems(&bones).is_empty());
    assert!(Hands::new(&bones).is_some());
}

#[test]
fn relaxed_the_joints_bend_as_measured_towards_the_palm_the_fingers_together() {
    let hands = Hands::new(&engineer()).expect("fingers");
    let frames = hands.pose(&at_rest(), [Hold::Relaxed; 2]);
    for (side, hand) in hands.hands.iter().enumerate() {
        for (k, digit) in hand.digits.iter().enumerate() {
            let f = &frames[side * 15 + k * 3..side * 15 + k * 3 + 3];
            // The middle and last joints: each bone turned from the one before by the measure.
            for j in 1..3 {
                let bend = f[j - 1].rotation.angle_between(f[j].rotation).to_degrees();
                assert!((bend - RELAXED[k][j]).abs() < 1e-6, "{k}.{j}: {bend}°");
            }
            // The tip moved towards the palm (the thumb towards the index).
            let straight = digit.points([0.0; 3], 0.02)[3];
            let bent = digit.points(RELAXED[k].map(f64::to_radians), 0.02)[3];
            let towards = if k == 0 {
                hand.digits[1].heads[0] - digit.heads[1]
            } else {
                hand.palm
            };
            assert!((bent - straight).dot(towards) > 0.0, "digit {k} bends back");
        }
        // The fingers' first bones lie beside the middle finger's, in the palm's plane.
        let flat = |d: DVec3| (d - hand.palm * d.dot(hand.palm)).normalize();
        let dir = |k: usize| {
            let p = hand.digits[k].points([0.0; 3], 0.0);
            flat(p[1] - p[0])
        };
        for k in [1, 3, 4] {
            let apart = dir(k).angle_between(dir(2)).to_degrees();
            assert!(apart < 0.5, "finger {k} {apart}° from the middle finger");
        }
    }
}

#[test]
fn gripping_the_fingers_close_on_the_handle() {
    let hands = Hands::new(&engineer()).expect("fingers");
    // A handle of 35 mm, the middle of the most comfortable (30–40 mm, n = 24).
    let radius = 0.0175;
    for hand in &hands.hands {
        let centre = hand.handle(radius);
        for (k, (digit, measured)) in hand.digits.iter().zip(RELAXED).enumerate().skip(1) {
            let bends = hand.wrapped(k, radius);
            let p = digit.points(bends, digit.tip());
            // Each bone meets the handle, none passes into it.
            for j in 0..3 {
                let d = distance_to_line(p[j], p[j + 1], centre, hand.across);
                assert!(d > radius + FINGER - 1e-3, "{k}.{j} in the handle by {d}");
                assert!(bends[j] < MOST[j] + 1e-9);
            }
            // Closed further than relaxed.
            let total: f64 = bends.iter().sum();
            let relaxed: f64 = measured.iter().map(|b| b.to_radians()).sum();
            assert!(
                total > relaxed,
                "finger {k}: {total} rad, relaxed {relaxed}"
            );
        }
    }
}

/// The fingers' share of the animation's budget (G1.3g: 0.05 ms of CPU a character a frame):
/// both hands posed, one relaxed and one gripping (its closing worked out once).
#[test]
fn the_fingers_are_posed_within_the_budget() {
    let hands = Hands::new(&engineer()).expect("fingers");
    let segments = at_rest();
    let hold = [Hold::Grip(0.0175), Hold::Relaxed];
    hands.pose(&segments, hold);
    let frames = 6000u32;
    let start = std::time::Instant::now();
    for _ in 0..frames {
        std::hint::black_box(hands.pose(&segments, hold));
    }
    let each = start.elapsed().as_secs_f64() * 1e3 / f64::from(frames);
    println!("both hands posed in {each:.4} ms");
    // Debug builds run slower; in release it is far within.
    assert!(each < 0.02, "{each} ms");
}

#[test]
fn open_the_fingers_lie_straight_and_together() {
    let hands = Hands::new(&engineer()).expect("fingers");
    let frames = hands.pose(&at_rest(), [Hold::Open; 2]);
    for side in 0..2 {
        for k in 0..5 {
            let f = &frames[side * 15 + k * 3..side * 15 + k * 3 + 3];
            // Each bone in line with the one before it; the thumb as modelled.
            for j in 1..3 {
                assert!(f[j - 1].rotation.angle_between(f[j].rotation) < 1e-9);
            }
            if k == 0 {
                assert!(f[0].rotation.angle_between(DQuat::IDENTITY) < 1e-9);
            }
        }
    }
}

#[test]
fn a_held_tool_lies_along_the_handle_the_fingers_close_on() {
    let hands = Hands::new(&engineer()).expect("fingers");
    let radius = 0.016;
    let grip = hands.grip(0, radius);
    let hand = &hands.hands[0];
    // Its origin on the handle's axis, its handle (+y) across the palm out past the index.
    assert!((grip.position - hand.handle(radius)).length() < 1e-12);
    assert!((grip.rotation * DVec3::Y).dot(hand.across) > 1.0 - 1e-9);
    // Its working end (+z) towards the fingertips, square to the handle.
    let ahead = grip.rotation * DVec3::Z;
    assert!(ahead.dot(hand.down) > 0.5 && ahead.dot(hand.across).abs() < 1e-9);
    // The gripping fingers wrap it: each finger's first and last bones bear on it, and its tip
    // lies round the handle's far side from the palm.
    for k in 1..5 {
        let digit = &hand.digits[k];
        let p = digit.points(hand.wrapped(k, radius), digit.tip());
        for j in [0, 2] {
            let d = distance_to_line(p[j], p[j + 1], grip.position, hand.across);
            assert!(
                d < radius + FINGER + 1e-3,
                "{k}.{j}: {d} m from the handle's axis"
            );
        }
        let round = (p[3] - grip.position).dot(hand.palm);
        assert!(
            round > 0.0,
            "finger {k}'s tip {round} m short of the handle's far side"
        );
    }
}

/// Each hand's shape at rest gives the frames its arm is placed by (G2.10): gripping, the
/// tool's (`Hands::grip`); under a box's edge, the fingers curled under its bottom, none in it.
#[test]
fn under_an_edge_the_fingers_curl_under_it_and_stay_out_of_the_box() {
    let hands = Hands::new(&engineer()).expect("fingers");
    let shapes = hands.shapes();
    for (side, hand) in hands.hands.iter().enumerate() {
        let grip = hands.grip(side, 0.02);
        let rest = shapes[side].rest(Hold::Grip(0.02));
        assert!((rest.position - grip.position).length() < 1e-12);
        for axis in [DVec3::X, DVec3::Y, DVec3::Z] {
            assert!((rest.rotation * axis - grip.rotation * axis).length() < 1e-12);
        }
        // The palm faces along the hold's x as `palm_sign` says.
        let x = rest.rotation * DVec3::X;
        assert!((x.dot(hand.palm) - shapes[side].palm_sign()).abs() < 1e-9);
        let edge = shapes[side].rest(Hold::Edge).position;
        let bends = hands.under_edge(side);
        let mut deepest = f64::MIN;
        for (k, (digit, bends)) in hand.digits.iter().zip(bends).enumerate().skip(1) {
            let p = digit.points(bends, digit.tip());
            for q in &p[1..] {
                // Under the box (its side of the face the palm is on): how far up into it.
                let q = *q - edge;
                if q.dot(hand.palm) > 0.0 {
                    deepest = deepest.max(-q.dot(hand.down));
                }
            }
            assert!(
                (p[3] - edge).dot(hand.palm) > 0.0,
                "finger {k} not under it"
            );
        }
        println!(
            "under an edge: the fingers' bones {:.1} mm below the bottom at the nearest",
            -deepest * 1e3
        );
        // Each bone's axis no nearer the bottom than about a finger's half thickness.
        assert!(deepest < -FINGER + 2e-3, "{deepest} m into the box");
    }
}
