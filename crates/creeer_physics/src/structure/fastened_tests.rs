//! Fastened joints (`fastened.rs`) against their laws: the equivalent member slips, turns,
//! bears and pulls out as the law says, yields and breaks where it says, and saves.

use super::*;
use super::{NodeId, StructureBuilder, Support};

const DT: f64 = 1.0 / 60.0;
/// Between the two members' axes (m).
const LENGTH: f64 = 0.1;

/// A joint of a few nails into softwood (orders of EN 1995-1-1's values).
const NAILS: JointLaw = JointLaw {
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

/// The joint alone: its first node clamped, its second (at +y, the fastener's way) held by
/// `support` and carrying 10 kg.
fn joint(law: &JointLaw, support: Support) -> (Structure, NodeId) {
    let mut b = StructureBuilder::new();
    let (a, n) = (b.node(DVec3::ZERO), b.node(DVec3::Y * LENGTH));
    b.support(a, Support::Clamped);
    b.support(n, support);
    b.fasten(a, n, law);
    b.mass(n, 10.0, DMat3::from_diagonal(DVec3::splat(0.01)));
    b.mass(a, 1.0, DMat3::from_diagonal(DVec3::splat(0.01)));
    let mut s = b.build();
    s.gravity = DVec3::ZERO;
    (s, n)
}

/// Its node free to move, kept from turning: the members it joins are stiff.
const MOVES: Support = Support::Custom {
    translation: DVec3::ONE,
    rotation: DVec3::ZERO,
};

#[test]
fn a_joint_slips_by_its_slip_modulus_and_bears_by_its_axial_stiffness() {
    let (mut s, n) = joint(&NAILS, MOVES);
    s.load[n] = DVec3::new(200.0, -500.0, 0.0);
    let (d, _) = statics::linear(&s).expect("held");
    let slip = d[n].x * NAILS.lateral / 200.0;
    assert!((slip - 1.0).abs() < 1e-9, "slip over F/K {slip}");
    let squeeze = -d[n].y * NAILS.axial / 500.0;
    assert!((squeeze - 1.0).abs() < 1e-9, "squeeze over F/K {squeeze}");
}

#[test]
fn a_joint_turns_about_its_fasteners_by_its_twist_stiffness() {
    let (mut s, n) = joint(
        &NAILS,
        Support::Custom {
            translation: DVec3::ZERO,
            rotation: DVec3::Y,
        },
    );
    s.torque[n] = DVec3::Y * 1.0;
    let (_, r) = statics::linear(&s).expect("held");
    let turn = r[n].y * NAILS.twist;
    assert!((turn - 1.0).abs() < 1e-9, "turn over T/K {turn}");
}

/// A load ramped from nothing to `peak` (N, along `way`) over 2 s and held: the slip at each
/// tick, and the tick the joint broke at.
fn ramp(law: &JointLaw, way: DVec3, peak: f64) -> (Vec<f64>, Option<usize>) {
    let (mut s, n) = joint(law, MOVES);
    let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, DT, 1);
    let mut slips = Vec::new();
    for tick in 0..240 {
        let t = (tick as f64 * DT / 2.0).min(1.0);
        s.load[n] = way * peak * t;
        solver.step(&mut s);
        slips.push(s.displacement(n).dot(way));
        if s.elements[0].broken {
            return (slips, Some(tick));
        }
    }
    (slips, None)
}

#[test]
fn below_its_yield_a_joint_slips_back_elastically() {
    let (slips, broke) = ramp(&NAILS, DVec3::X, 0.9 * NAILS.lateral_yield);
    assert_eq!(broke, None);
    let elastic = 0.9 * NAILS.lateral_yield / NAILS.lateral;
    let last = *slips.last().expect("ticks");
    assert!(
        (last / elastic - 1.0).abs() < 0.02,
        "slip {last} m, elastic {elastic} m"
    );
}

/// Past its peak the joint slips on at its peak force and fails at its last slip.
#[test]
fn an_overloaded_joint_fails_at_its_last_slip() {
    let (slips, broke) = ramp(&NAILS, DVec3::X, 1.5 * NAILS.lateral_peak);
    let tick = broke.expect("fails");
    let at = slips[tick.saturating_sub(1)];
    assert!(
        (at / NAILS.slip_at_failure - 1.0).abs() < 0.15,
        "failed at {at} m of slip, its law {} m",
        NAILS.slip_at_failure
    );
}

#[test]
fn pulled_past_its_withdrawal_a_joint_lets_go_and_pressed_it_holds() {
    assert_eq!(ramp(&NAILS, DVec3::Y, 0.9 * NAILS.withdrawal).1, None);
    assert!(ramp(&NAILS, DVec3::Y, 1.1 * NAILS.withdrawal).1.is_some());
    assert_eq!(ramp(&NAILS, DVec3::NEG_Y, 10.0 * NAILS.withdrawal).1, None);
}

/// Pull and slip together, each at 0.6 of its capacity: smooth nails' linear interaction
/// lets go (1.2 ≥ 1), the quadratic one of other fasteners holds (0.72).
#[test]
fn withdrawal_and_slip_interact_as_the_code_says() {
    let both = DVec3::new(0.6 * NAILS.lateral_peak, 0.6 * NAILS.withdrawal, 0.0);
    let (length, way) = (both.length(), both.normalize());
    assert!(ramp(&NAILS, way, length).1.is_some(), "linear");
    let quadratic = JointLaw {
        interaction: 2.0,
        ..NAILS
    };
    assert_eq!(ramp(&quadratic, way, length).1, None, "quadratic");
}

#[test]
fn a_joint_has_no_mass_and_never_buckles() {
    let (s, n) = joint(&NAILS, MOVES);
    assert!((s.total_mass() - 11.0).abs() < 1e-12);
    assert!(
        s.inertia_of(n)
            .abs_diff_eq(DMat3::from_diagonal(DVec3::splat(0.01)), 1e-15)
    );
    let e = &s.elements[0];
    assert!(e.is_joint() && !e.wants_split(&[-1.0, 0.0, 0.0, 0.0, 0.0, 0.0]));
}

#[test]
fn a_joint_saves_and_loads_as_it_was_and_members_as_before() {
    let (s, _) = joint(&NAILS, MOVES);
    let mut w = creeer_math::codec::Writer::new();
    w.put(&s.elements[0]);
    let bytes = w.finish();
    let mut r = creeer_math::codec::Reader::new(&bytes);
    let back: element::Element = r.get().expect("reads");
    assert_eq!(back.fastened, s.elements[0].fastened);
    assert_eq!(back.stiffness, s.elements[0].stiffness);
    // A member's flags byte is a bool's, as it was before joints.
    let mut member = back;
    member.fastened = None;
    member.crooked = true;
    let mut w = creeer_math::codec::Writer::new();
    w.put(&member);
    assert_eq!(w.finish().last(), Some(&1));
}

/// Swinging across and along the joint within its yield, undamped, the joint stores and
/// gives back: the energy stays. Its node held from turning, every measure is linear in the
/// ends' offset, and trapezoidal Newmark keeps a linear spring's energy exactly: what is left
/// is the solve's tolerance. A joint soft enough for the step to resolve (ω·h ≈ 0.75 across,
/// 0.6 along); stiffer ones ring faster than any step resolves, and the step's energy guard
/// takes their ringing, as it does stiff members' (ADR-0033).
#[test]
fn an_elastic_joint_keeps_its_energy() {
    let soft = JointLaw {
        damping: 0.0,
        lateral: 2.0e4,
        axial: 1.3e4,
        ..NAILS
    };
    let (mut s, n) = joint(&soft, MOVES);
    s.velocity[n] = DVec3::new(0.4, 0.2, 0.3);
    let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, DT, 1);
    let start = s.energy();
    for _ in 0..240 {
        solver.step(&mut s);
        assert_eq!(solver.last_yielding.yielded, 0, "within its yield");
        let drift = (s.energy() - start).abs() / start;
        assert!(drift < 1e-6, "energy drifted by {drift}");
    }
}
