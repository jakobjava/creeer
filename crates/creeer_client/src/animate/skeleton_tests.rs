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
