//! What nailing asks of a structure (`nails.rs`): how thick a member is along a nail, a joint's
//! twist read back, and a joint given more nails where it is.

use super::section::STEEL_S235;
use super::shapes::cantilever;
use super::*;
use super::{JointLaw, Support};

const ONE_NAIL: JointLaw = JointLaw {
    axial: 5.0e6,
    bearing: 20.0e3,
    withdrawal: 600.0,
    lateral: 7.0e5,
    lateral_yield: 1000.0,
    lateral_peak: 1300.0,
    slip_at_peak: 0.008,
    slip_at_failure: 0.015,
    twist: 50.0,
    twist_yield: 2.0,
    interaction: 1.0,
    damping: 0.002,
};

/// A 150 × 75 mm beam lying along x: on edge (its 150 mm up, along y) it reaches 75 mm up and
/// 37.5 mm across; a round 100 mm pole 50 mm whichever way.
#[test]
fn a_member_reaches_its_half_section_along_a_nail() {
    let beam = cantilever(2.0, 2, Section::rectangle(0.15, 0.075), STEEL_S235);
    let f = beam.member_frame(0).expect("a member");
    let up = f.y;
    assert!((f.reach(up) - 0.075).abs() < 1e-12, "{}", f.reach(up));
    assert!((f.reach(f.z) - 0.0375).abs() < 1e-12);
    assert!(!f.round);
    let pole = cantilever(2.0, 2, Section::solid_round(0.1), STEEL_S235);
    let f = pole.member_frame(0).expect("a member");
    assert!(f.round);
    let oblique = (f.y + f.z).normalize();
    assert!((f.reach(oblique) - 0.05).abs() < 1e-12);
}

/// A bar and a second one 5 cm above its tip, joined there by `law`: the structure, the
/// joint's index, and the second bar's node at the joint.
fn joined(law: &JointLaw) -> (Structure, usize, usize) {
    let mut s = cantilever(1.0, 2, Section::solid_round(0.05), STEEL_S235);
    let tip = s.node_count() - 1;
    let mut other = cantilever(0.5, 1, Section::solid_round(0.05), STEEL_S235);
    for p in &mut other.position {
        *p += s.position[tip] + DVec3::Z * 0.05;
    }
    other.set_support(0, Support::Free);
    let at = s.append(&other);
    let joint = s.join(tip, at, law);
    (s, joint, at)
}

/// A joint's twist reads back as its law gave it; refastened with the law of two nails, it is
/// the joint of two made where it was (loaded alike, the two structures move alike), and keeps
/// two nails' capacities.
#[test]
fn a_joint_given_a_second_nail_holds_as_the_law_of_two() {
    let two = JointLaw {
        withdrawal: 2.0 * ONE_NAIL.withdrawal,
        lateral: 2.0 * ONE_NAIL.lateral,
        lateral_yield: 2.0 * ONE_NAIL.lateral_yield,
        lateral_peak: 2.0 * ONE_NAIL.lateral_peak,
        twist: 4.0 * ONE_NAIL.twist,
        ..ONE_NAIL
    };
    let (mut s, joint, at) = joined(&ONE_NAIL);
    let twist = s.joint_twist(joint).expect("a joint");
    assert!((twist / ONE_NAIL.twist - 1.0).abs() < 1e-9, "{twist}");
    assert!(s.joint_twist(0).is_none(), "a member is no joint");
    // Pushed across and twisted at the second bar's node, weightless (the free bar's weight
    // would twist the joint past its yield).
    let loaded = |s: &Structure| {
        let mut s = s.clone();
        s.gravity = DVec3::ZERO;
        s.load[at] = DVec3::Y * 200.0;
        s.torque[at] = DVec3::Z * 0.5;
        statics::nonlinear(&mut s, 50);
        s
    };
    let one = loaded(&s);
    s.refasten(joint, &two);
    assert!((s.joint_twist(joint).expect("a joint") / two.twist - 1.0).abs() < 1e-9);
    assert_eq!(
        s.joint_withdrawal(joint),
        Some(two.withdrawal),
        "its capacities the new law's"
    );
    let refastened = loaded(&s);
    let (fresh, _, _) = joined(&two);
    let fresh = loaded(&fresh);
    // The second bar turned against the first's tip (which the push bends round by 1.6 mrad).
    let tip = at - 1;
    let turned =
        |s: &Structure| crate::so3::log(s.orientation[tip].conjugate() * s.orientation[at]).z;
    println!(
        "turned by 0.5 N·m: one nail {:.4} rad, refastened with two {:.4}, joined with two {:.4}",
        turned(&one),
        turned(&refastened),
        turned(&fresh)
    );
    for n in 0..s.node_count() {
        assert!((refastened.position[n] - fresh.position[n]).length() < 1e-12);
    }
    // T/k about the joint: 0.5 N·m over 50, then over 200 N·m/rad.
    assert!(
        (turned(&one) / 0.01 - 1.0).abs() < 0.05,
        "one nail turns by T/k: {}",
        turned(&one)
    );
    assert!(
        (turned(&refastened) / turned(&one) - 0.25).abs() < 0.02,
        "two nails four times as stiff against turning"
    );
}
