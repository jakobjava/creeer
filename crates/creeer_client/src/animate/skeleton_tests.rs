use super::*;

fn close(a: &Frame64, b: &Frame64) -> bool {
    (a.position - b.position).length() < 1e-9 && a.rotation.abs_diff_eq(b.rotation, 1e-9)
}

#[test]
fn unturned_joints_pose_the_model_at_rest() {
    let root = Frame64::new(DVec3::new(3.0, -2.0, 1.0), DQuat::from_rotation_y(0.7));
    let out = posed(root, &[DQuat::IDENTITY; 14]);
    assert!(out.iter().all(|b| close(b, &root)));
}

#[test]
fn a_turned_joint_keeps_its_place() {
    // The left elbow bent: the elbow stays where it was, the forearm and hand turn about it.
    let mut turns = [DQuat::IDENTITY; 14];
    turns[ELBOW[1]] = DQuat::from_rotation_x(-1.2);
    let out = posed(Frame64::IDENTITY, &turns);
    let elbow = bones::joints()[ELBOW[1]].2;
    for k in [FOREARM[1], 11] {
        assert!((out[k].transform_point(elbow) - elbow).length() < 1e-12);
    }
    let hand = bones::joints()[ELBOW[1] + 1].2;
    assert!(
        out[FOREARM[1]].transform_point(hand).z > hand.z + 0.1,
        "bent forward"
    );
}

#[test]
fn the_leg_at_rest_is_the_model_at_rest() {
    let joints = bones::joints();
    for side in 0..2 {
        let (hip, ankle) = (joints[HIP[side]].2, joints[ANKLE[side]].2);
        let (leg, short) = leg(side, hip, ankle, DVec3::Z, DVec3::Y, DVec3::Y);
        assert!(short < 1e-9, "{short}");
        for bone in leg {
            assert!(close(&bone, &Frame64::IDENTITY), "{bone:?}");
        }
    }
}

#[test]
fn a_leg_reaches_what_it_can_and_bends_its_knee_forward() {
    let joints = bones::joints();
    let (hip, knee, ankle) = (joints[HIP[0]].2, joints[KNEE[0]].2, joints[ANKLE[0]].2);
    let forward = DVec3::new(0.6, 0.0, 0.8);
    for target in [
        ankle + DVec3::new(0.0, 0.25, 0.1),
        ankle + DVec3::new(0.2, 0.1, -0.3),
        hip + DVec3::new(0.05, -0.5, 0.3),
    ] {
        let (leg, short) = leg(0, hip, target, forward, DVec3::Y, DVec3::Y);
        assert!(short < 1e-6, "{short} m short of {target}");
        // The hip stays put, the shank meets the thigh at the knee, the foot the shank.
        assert!((leg[0].transform_point(hip) - hip).length() < 1e-9);
        let k = leg[0].transform_point(knee);
        assert!((leg[1].transform_point(knee) - k).length() < 1e-9);
        assert!((leg[2].transform_point(ankle) - target).length() < 1e-6);
        // Bent, the knee is ahead of the line from hip to ankle.
        let line = (target - hip).normalize();
        let off = (k - hip) - line * (k - hip).dot(line);
        assert!(off.dot(forward) > 0.0, "the knee bends back");
    }
    // Out of reach: as straight as it goes, pointing at it.
    let far = hip + DVec3::new(0.0, -2.0, 0.0);
    let (_, short) = leg(0, hip, far, DVec3::Z, DVec3::Y, DVec3::Y);
    assert!(
        (short - (2.0 - longest_leg() / 0.9999)).abs() < 1e-3,
        "{short}"
    );
}

/// The knee's bend solved exactly (G2.10) is the old search's (G1.3g: a ternary search for the
/// straightest, then bisection) to its precision, over every reach a leg has.
#[test]
fn the_exact_knee_is_the_searched_one() {
    let joints = bones::joints();
    for side in 0..2 {
        let (h, k, a) = (
            joints[HIP[side]].2,
            joints[KNEE[side]].2,
            joints[ANKLE[side]].2,
        );
        let span = |angle: f64| (k + DQuat::from_rotation_x(angle) * (a - k) - h).length();
        let searched = |reach: f64| {
            let (mut lo, mut hi) = (0.0f64, 0.6f64);
            for _ in 0..60 {
                let (a, b) = (lo + (hi - lo) / 3.0, hi - (hi - lo) / 3.0);
                if span(a) < span(b) {
                    lo = a;
                } else {
                    hi = b;
                }
            }
            let straightest = 0.5 * (lo + hi);
            if reach >= span(straightest) {
                return straightest;
            }
            let (mut lo, mut hi, growing) = if reach >= span(0.0) {
                (0.0, straightest, true)
            } else {
                (straightest, 2.4, false)
            };
            for _ in 0..50 {
                let mid = 0.5 * (lo + hi);
                if (span(mid) < reach) == growing {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            0.5 * (lo + hi)
        };
        let chain = Chain::new(h, k, a);
        assert!(
            (0.0..0.6).contains(&chain.straightest),
            "{}",
            chain.straightest
        );
        let (mut worst, mut reaches) = (0.0f64, 0.0f64);
        for i in 0..=400 {
            let reach = 0.2 + 0.7 * f64::from(i) / 400.0;
            let bend = knee_for(side, reach);
            worst = worst.max((bend - searched(reach)).abs());
            reaches = reaches.max((span(bend) - span(searched(reach))).abs());
            // Within its range it reaches exactly as far.
            if reach < chain.span(chain.straightest) && reach > chain.span(KNEE_MOST) {
                assert!((span(bend) - reach).abs() < 1e-9, "{reach}: {}", span(bend));
            }
        }
        println!(
            "leg {side}: the exact knee within {worst:.1e} rad of the searched, reaching within {reaches:.1e} m"
        );
        // (The search finds the straightest to the square root of the doubles' precision only,
        // 1.5e-8: the span is flat at its longest. Measured 2.1e-8 rad, the reach the same to
        // the doubles' precision.)
        assert!(worst < 5e-8, "{worst} rad");
        assert!(reaches < 1e-12, "{reaches} m");
    }
}
