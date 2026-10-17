//! Members that buckle (`buckling.rs`): a single member under compression is split by
//! itself, and then bends and buckles as a column does.

use super::buckling::{CROOKEDNESS, KINK, SPLIT};
use super::section::STEEL_S235;
use super::*;
use super::{StructureBuilder, Support};

const DT: f64 = 1.0 / 60.0;

/// One member of `section` and `material`, `length` long, pinned at its foot and guided at
/// its head (free to slide along it and to turn), its head pushed down with `load(t)` (N)
/// for `time` seconds, or until it has folded (its head down by half its length). Returns the
/// structure and, every tick, the load, the side offset of the member's middle from its
/// chord (zero until it is split) and how far its head is down.
fn column(
    length: f64,
    section: Section,
    material: Material,
    load: impl Fn(f64) -> f64,
    time: f64,
) -> (Structure, Vec<(f64, f64, f64)>) {
    let (mut s, head) = pinned(length, section, material);
    let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, DT, 1);
    let mut samples = Vec::new();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few hundred
    let ticks = (time / DT) as u32;
    for tick in 0..ticks {
        let force = load(f64::from(tick) * DT);
        s.load[head] = DVec3::NEG_Y * force;
        solver.step(&mut s);
        let side = if s.node_count() > 2 { side(&s) } else { 0.0 };
        let down = length - s.position[head].y;
        samples.push((force, side, down));
        if down > 0.5 * length {
            break;
        }
    }
    (s, samples)
}

/// One member, weightless, `length` long, pinned at its foot and guided at its head; and its
/// head.
fn pinned(length: f64, section: Section, material: Material) -> (Structure, NodeId) {
    let mut b = StructureBuilder::new();
    let foot = b.node(DVec3::ZERO);
    let head = b.node(DVec3::Y * length);
    b.support(foot, Support::Pinned);
    b.support(
        head,
        Support::Custom {
            translation: DVec3::Y,
            rotation: DVec3::ONE,
        },
    );
    b.beam(foot, head, section, material);
    let mut s = b.build();
    s.gravity = DVec3::ZERO;
    (s, head)
}

/// The side offset of the middle joint of a column along y from its chord.
fn side(s: &Structure) -> f64 {
    let p = s.position[2];
    DVec3::new(p.x, 0.0, p.z).length()
}

/// Euler's load of a member pinned at both ends (N).
fn euler(length: f64, section: &Section) -> f64 {
    let pi2 = std::f64::consts::PI * std::f64::consts::PI;
    pi2 * STEEL_S235.youngs_modulus * section.i_y.min(section.i_z) / (length * length)
}

/// The middle's offset from the chord, per e₀, of a column pinned at both ends whose
/// crookedness is a kink of KINK·e₀ at mid-length, under `fraction` of Euler's load: each
/// odd sine mode n of the kink, 8·KINK/(π²·n²) of e₀, amplified by 1/(1 − fraction/n²).
fn kinked(fraction: f64) -> f64 {
    let pi2 = std::f64::consts::PI * std::f64::consts::PI;
    (1..20_000)
        .step_by(2)
        .map(|n| {
            let n2 = f64::from(n) * f64::from(n);
            8.0 * KINK / (pi2 * n2) / (1.0 - fraction / n2)
        })
        .sum()
}

/// An elastic column of one member, 3 m of 30 mm round bar, under a load ramped to
/// `fraction` of Euler's: it is split once the load passes half of Euler's. Returns its
/// middle's offset from its chord at the end, and how many elements it then has.
fn elastic(fraction: f64) -> (f64, usize) {
    let (l, sec) = (3.0, Section::solid_round(0.03));
    let p_e = euler(l, &sec);
    // Elastic (no yield) and well damped: the static shape, not its vibration.
    let steel = Material {
        yield_strength: f64::INFINITY,
        damping: 0.05,
        ..STEEL_S235
    };
    let (s, samples) = column(l, sec, steel, |t| p_e * fraction * (t / 4.0).min(1.0), 7.0);
    (samples.last().expect("ticks").1, s.element_count())
}

/// Its middle stands off its chord by its crookedness amplified as Perry and Robertson say:
/// the half sine's first mode (which the kink carries exactly) by 1/(1 − P/P_E), the rest of
/// the kink hardly at all.
#[test]
fn a_compressed_column_bends_as_perry_and_robertson_say() {
    let e0 = CROOKEDNESS * 3.0;
    for (fraction, within) in [(0.6, 0.01), (0.8, 0.02), (0.9, 0.1)] {
        let (side, elements) = elastic(fraction);
        assert_eq!(elements, 2, "split once");
        let want = e0 * kinked(fraction);
        // Close to Euler's load the two elements' P_cr (0.8 % above Euler's) shows.
        assert!(
            (side / want - 1.0).abs() < within,
            "{fraction}·P_E: {side:e} m, expected {want:e} m"
        );
    }
}

/// Past Euler's load it buckles: its middle swings out by a sizeable part of its length
/// (elastic post-buckling: 0.2·L at 1.05·P_E), and its halves split to follow its shape.
#[test]
fn a_column_past_euler_s_load_buckles() {
    let (side, elements) = elastic(1.05);
    assert!(side > 3.0 / 20.0, "only {side} m out");
    assert!(elements <= 8, "{elements} elements");
}

/// A member split under a load it already carries is in equilibrium at once (`settle.rs`):
/// its new joint stands where its kink puts it under that load (Perry and Robertson, as
/// above), and nothing starts to move (placed on its cubic alone, the joint was 2·N·β out of
/// balance, and rang).
#[test]
fn a_member_split_under_load_is_in_equilibrium_at_once() {
    let (l, sec) = (3.0, Section::solid_round(0.03));
    let p_e = euler(l, &sec);
    let steel = Material {
        yield_strength: f64::INFINITY,
        ..STEEL_S235
    };
    let (mut s, head) = pinned(l, sec, steel);
    s.load[head] = DVec3::NEG_Y * (0.6 * p_e);
    s.splits = false;
    statics::nonlinear(&mut s, 200);
    s.splits = true;
    let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, DT, 1);
    let want = CROOKEDNESS * l * kinked(0.6);
    for _ in 0..120 {
        solver.step(&mut s);
        assert_eq!(s.element_count(), 2, "split in the first step");
        let side = side(&s);
        assert!(
            (side / want - 1.0).abs() < 0.01,
            "{side:e} m, expected {want:e} m"
        );
        let fastest = s.velocity.iter().map(|v| v.length()).fold(0.0, f64::max);
        assert!(fastest < 1e-5, "moving at {fastest:e} m/s");
    }
}

/// Below half of Euler's load nothing is split: a frame that is never loaded much keeps
/// its members whole.
#[test]
fn a_lightly_compressed_member_stays_whole() {
    let (l, sec) = (3.0, Section::solid_round(0.03));
    let p_e = euler(l, &sec);
    let (s, _) = column(l, sec, STEEL_S235, |_| 0.9 * SPLIT * p_e, 2.0);
    assert_eq!(s.element_count(), 1);
}

/// The load at which a column of `l` m of 30 mm round S235 bar, pinned at both ends, whose
/// middle stands off its chord by e₀·`offset(N/P_E)`, forms a plastic hinge there:
/// N·e₀·offset = M_p·(1 − (N/N_p)²), the members' own interaction of axial force and bending.
fn hinge_load(l: f64, offset: impl Fn(f64) -> f64) -> f64 {
    let sec = Section::solid_round(0.03);
    let (n_p, p_e) = (STEEL_S235.yield_strength * sec.area, euler(l, &sec));
    let m_p = STEEL_S235.yield_strength * 0.03 * 0.03 * 0.03 / 6.0;
    let (mut lo, mut hi) = (0.0, n_p.min(p_e));
    for _ in 0..100 {
        let n = 0.5 * (lo + hi);
        if n * CROOKEDNESS * l * offset(n / p_e) < m_p * (1.0 - (n / n_p) * (n / n_p)) {
            lo = n;
        } else {
            hi = n;
        }
    }
    lo
}

/// A column of real S235 at relative slenderness λ̄ = 1 (0.7 m of 30 mm round bar, its
/// squash load and Euler's load about equal) gives way where its crookedness, amplified,
/// brings its middle to a plastic hinge: as the kink it carries says (within 1 %), and as
/// the half sine says that the kink stands for (the kink's higher modes cost it 1 %). That
/// lies above Eurocode 3's buckling curve a (whose imperfections include the residual
/// stresses of rolling, not modelled here) and below both Euler's and the squash load.
#[test]
fn a_column_of_middling_slenderness_fails_where_its_hinge_forms() {
    let (l, sec) = (0.7, Section::solid_round(0.03));
    let n_p = STEEL_S235.yield_strength * sec.area;
    let p_e = euler(l, &sec);
    // Ramped slowly (strain rates near the tensile test's) until it gives way.
    let steel = Material {
        damping: 0.05,
        ..STEEL_S235
    };
    let ramp = 30.0;
    let (_, samples) = column(l, sec, steel, |t| 1.2 * n_p.min(p_e) * t / ramp, ramp);
    let peak = samples
        .iter()
        .find(|(_, _, down)| *down > l / 20.0)
        .map(|(force, _, _)| *force)
        .expect("it buckles");
    let kink = hinge_load(l, kinked);
    let sine = hinge_load(l, |fraction| 1.0 / (1.0 - fraction));
    assert!(
        (peak / kink - 1.0).abs() < 0.01,
        "gave way at {peak:.0} N, kink {kink:.0} N"
    );
    assert!(
        (peak / sine - 1.0).abs() < 0.02,
        "gave way at {peak:.0} N, sine {sine:.0} N"
    );
    let lambda = (n_p / p_e).sqrt();
    let phi = 0.5 * (1.0 + 0.21 * (lambda - 0.2) + lambda * lambda);
    let chi = 1.0 / (phi + (phi * phi - lambda * lambda).sqrt());
    assert!(
        peak > chi * n_p && peak < n_p.min(p_e),
        "{peak} N, curve a {} N",
        chi * n_p
    );
}

/// One member, swinging and bent after a step, split once (as a half already crooked, or
/// not; its ends are held by nothing, so only the new joint settles). Returns the momentum, kinetic energy and strain energy before and after, and the
/// kinetic energy the re-lumped mass should lose, m/16·|v_a − v_b|².
fn split_once(crooked: bool) -> ([DVec3; 2], [f64; 2], [f64; 2], f64) {
    let mut b = StructureBuilder::new();
    let a = b.node(DVec3::ZERO);
    let c = b.node(DVec3::new(2.0, 0.3, 0.1));
    b.beam(a, c, Section::round_tube(0.1, 0.005), STEEL_S235);
    b.mass(a, 50.0, creeer_math::glam::DMat3::IDENTITY);
    b.mass(c, 80.0, creeer_math::glam::DMat3::IDENTITY);
    let mut s = b.build();
    s.gravity = DVec3::ZERO;
    s.velocity[a] = DVec3::new(3.0, -1.0, 0.5);
    s.velocity[c] = DVec3::new(-2.0, 0.5, 1.0);
    s.angular_velocity[a] = DVec3::new(0.0, 0.0, 0.4);
    s.angular_velocity[c] = DVec3::new(0.0, 0.2, -0.3);
    let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, DT, 1);
    solver.step(&mut s);
    s.elements[0].crooked = crooked;
    let momentum =
        |s: &Structure| -> DVec3 { (0..s.node_count()).map(|i| s.velocity[i] * s.mass[i]).sum() };
    let relumped = s.elements[0].mass() / 16.0 * (s.velocity[a] - s.velocity[c]).length_squared();
    let before = (momentum(&s), s.node_energy().0, s.elastic_energy());
    solver.wanted = vec![0];
    assert_eq!(solver.split_compressed(&mut s).len(), 1);
    let after = (momentum(&s), s.node_energy().0, s.elastic_energy());
    (
        [before.0, after.0],
        [before.1, after.1],
        [before.2, after.2],
        relumped,
    )
}

/// A split of a member swinging and vibrating keeps the momentum exactly. Re-lumped, the
/// member's mass carries less kinetic energy: m/16·|v_a − v_b|² less in its joints' motion,
/// a little less in their turning. The new joint settles where its halves carry its own
/// inertia, which changes their strain energy a little (0.06 % here); the energy as a
/// whole never rises.
#[test]
fn a_split_keeps_momentum_and_never_adds_energy() {
    for crooked in [true, false] {
        let (momentum, kinetic, strain, relumped) = split_once(crooked);
        let [before, after] = momentum;
        assert!(
            (after - before).length() <= 1e-12 * before.length(),
            "{before} -> {after}"
        );
        let lost = kinetic[0] - kinetic[1];
        assert!(
            lost >= relumped && lost < 1.2 * relumped,
            "kinetic {kinetic:?}, {relumped} J"
        );
        assert!(
            kinetic[1] + strain[1] < kinetic[0] + strain[0],
            "kinetic {kinetic:?}, strain {strain:?}"
        );
    }
}
