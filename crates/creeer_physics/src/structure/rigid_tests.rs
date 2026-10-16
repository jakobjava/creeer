//! Rigid bodies (a node with mass and inertia, no members): free flight and rotation.

use super::*;
use creeer_math::det;

const DT: f64 = 1.0 / 60.0;

/// Inertia of a solid box with edge lengths `size` (kg·m²).
fn box_inertia(mass: f64, size: DVec3) -> DMat3 {
    let s = size * size;
    DMat3::from_diagonal(DVec3::new(s.y + s.z, s.x + s.z, s.x + s.y) * (mass / 12.0))
}

/// A 10 kg box of 0.1 × 0.3 × 0.6 m: three clearly different principal moments, the
/// intermediate one about y.
fn tumbling_box(omega: DVec3) -> Structure {
    let inertia = box_inertia(10.0, DVec3::new(0.1, 0.3, 0.6));
    let mut s = StructureBuilder::rigid_body(DVec3::ZERO, 10.0, inertia);
    s.gravity = DVec3::ZERO;
    s.angular_velocity[0] = omega;
    s
}

fn momentum(s: &Structure) -> DVec3 {
    let r = DMat3::from_quat(s.orientation[0]);
    r * s.inertia[0] * r.transpose() * s.angular_velocity[0]
}

/// Constant acceleration is integrated exactly by Newmark: x = x₀ + v₀·t + g·t²/2.
#[test]
fn free_fall_is_exact() {
    let mut s = StructureBuilder::rigid_body(DVec3::ZERO, 2.0, DMat3::IDENTITY);
    s.velocity[0] = DVec3::new(3.0, 5.0, 0.0);
    let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, DT, 1);
    for _ in 0..120 {
        solver.step(&mut s);
    }
    let t = 2.0;
    let want = DVec3::new(3.0, 5.0, 0.0) * t + STANDARD_GRAVITY * (0.5 * t * t);
    assert!(
        (s.position[0] - want).length() < 1e-9,
        "{} vs {want}",
        s.position[0]
    );
}

/// Torque-free tumbling about a general axis: angular momentum (not angular velocity) is
/// conserved, and so is the energy (measured over 10 s: 0.012 % and 0.03 %).
#[test]
fn tumbling_conserves_angular_momentum_and_energy() {
    let mut s = tumbling_box(DVec3::new(1.0, 2.0, 3.0));
    let (l0, e0) = (momentum(&s), s.energy());
    let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, DT, 1);
    for tick in 0..600 {
        solver.step(&mut s);
        let drift = (momentum(&s) - l0).length() / l0.length();
        assert!(drift < 1e-3, "tick {tick}: momentum drift {drift}");
    }
    let energy_drift = (s.energy() / e0 - 1.0).abs();
    assert!(energy_drift < 1e-3, "energy drift {energy_drift}");
}

/// The tennis-racket (Dzhanibekov) effect: spin about the intermediate axis is unstable and
/// flips over within seconds; spin about the major axis is stable.
#[test]
fn intermediate_axis_flips_and_major_axis_is_stable() {
    // Angle between the body's y (or z) axis and its initial direction, over 10 s.
    let largest_tilt = |omega: DVec3, axis: DVec3| {
        let mut s = tumbling_box(omega);
        let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, DT, 1);
        (0..600).fold(0.0, |tilt: f64, _| {
            solver.step(&mut s);
            tilt.max(det::acos(
                (s.orientation[0] * axis).dot(axis).clamp(-1.0, 1.0),
            ))
        })
    };
    let intermediate = largest_tilt(DVec3::new(0.01, 5.0, 0.01), DVec3::Y);
    let major = largest_tilt(DVec3::new(5.0, 0.01, 0.01), DVec3::X);
    assert!(
        intermediate > 2.5,
        "intermediate axis should flip: {intermediate} rad"
    );
    assert!(major < 0.1, "major axis should stay put: {major} rad");
}

/// Quadratic drag (½·ρ·C_d·A = 0.2 kg/m on a 1 kg body): falling from rest, the body
/// approaches the terminal velocity √(m·g/drag) = 7.00 m/s, and the speed at any time
/// follows the exact solution v(t) = v_t·tanh(g·t/v_t).
#[test]
fn falling_with_drag_reaches_terminal_velocity() {
    let mut s = StructureBuilder::rigid_body(DVec3::ZERO, 1.0, DMat3::IDENTITY);
    s.drag[0] = 0.2;
    let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, DT, 1);
    let g = STANDARD_GRAVITY.length();
    let terminal = (g / 0.2_f64).sqrt();
    for tick in 1..=300 {
        solver.step(&mut s);
        let t = f64::from(tick) * DT;
        let want = terminal * det::tanh(g * t / terminal);
        let speed = -s.velocity[0].y;
        assert!(
            (speed - want).abs() < 0.005 * terminal,
            "t {t}: {speed} vs {want}"
        );
    }
}

/// A projectile with drag (fired at 30 m/s, 45°) against a reference integration with
/// 1000× smaller steps (classical Runge-Kutta): range and flight time within 0.5 %.
#[test]
fn projectile_with_drag_matches_a_fine_reference() {
    let drag = 0.01;
    let v0 = DVec3::new(1.0, 1.0, 0.0).normalize() * 30.0;
    let g = STANDARD_GRAVITY;
    // Reference: RK4 at 1/60 000 s until the projectile is back at y = 0.
    let accel = |v: DVec3| g - v * (drag * v.length());
    let (mut x, mut v, mut t) = (DVec3::ZERO, v0, 0.0);
    let h = DT / 1000.0;
    while !(t > 0.1 && x.y <= 0.0) {
        let k1 = (v, accel(v));
        let k2 = (v + k1.1 * (h / 2.0), accel(v + k1.1 * (h / 2.0)));
        let k3 = (v + k2.1 * (h / 2.0), accel(v + k2.1 * (h / 2.0)));
        let k4 = (v + k3.1 * h, accel(v + k3.1 * h));
        x += (k1.0 + k2.0 * 2.0 + k3.0 * 2.0 + k4.0) * (h / 6.0);
        v += (k1.1 + k2.1 * 2.0 + k3.1 * 2.0 + k4.1) * (h / 6.0);
        t += h;
    }
    let (range, flight) = (x.x, t);
    let mut s = StructureBuilder::rigid_body(DVec3::ZERO, 1.0, DMat3::IDENTITY);
    s.drag[0] = drag;
    s.velocity[0] = v0;
    let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, DT, 1);
    let mut ticks = 0;
    let mut previous;
    loop {
        previous = s.position[0];
        solver.step(&mut s);
        ticks += 1;
        if ticks > 6 && s.position[0].y <= 0.0 {
            break;
        }
    }
    // Interpolate the landing between the last two ticks.
    let p = s.position[0];
    let f = previous.y / (previous.y - p.y);
    let landed_x = previous.x + (p.x - previous.x) * f;
    let landed_t = (f64::from(ticks) - 1.0 + f) * DT;
    assert!(
        (landed_x / range - 1.0).abs() < 0.005,
        "range {landed_x} vs {range}"
    );
    assert!(
        (landed_t / flight - 1.0).abs() < 0.005,
        "flight {landed_t} vs {flight}"
    );
}
