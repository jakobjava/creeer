//! Aerodynamics against its references: the sphere's standard drag curve, lifting-line
//! theory's slopes, a flat plate broadside, Ackeret, the pitot shock, flat-plate friction,
//! the recovery temperature; the dashpot's slope.

use super::coefficients::{
    Section, lift_drag, skin_friction, sphere_drag, stagnation_cp, wing_slope,
};
use super::eigen::positive_part;
use super::{Shape, Surface};
use crate::atmosphere::Air;
use creeer_math::glam::{DMat3, DVec3};
use std::f64::consts::PI;

/// Sea-level standard air.
const AIR: Air = Air {
    t: 288.15,
    p: 101_325.0,
    density: 1.225,
    viscosity: 1.789e-5,
    sound: 340.294,
};

/// The standard drag curve of a sphere (Schlichting, Boundary-Layer Theory, fig. 1.21; White,
/// Fluid Mechanics, fig. 7.16): within 10 % from Re 0.1 to 10⁵, Stokes's 24/Re at the
/// smallest, and past the crisis (Re 10⁶) below 0.25.
#[test]
fn spheres_follow_the_standard_drag_curve() {
    for (re, measured) in [
        (0.1, 244.0),
        (1.0, 27.0),
        (10.0, 4.1),
        (100.0, 1.07),
        (1000.0, 0.46),
        (1e4, 0.40),
        (1e5, 0.47),
    ] {
        let cd = sphere_drag(re, 0.0);
        assert!(
            (cd / measured - 1.0).abs() < 0.1,
            "Re {re}: {cd}, measured {measured}"
        );
    }
    assert!(
        (sphere_drag(1e-4, 0.0) * 1e-4 / 24.0 - 1.0).abs() < 1e-3,
        "Stokes"
    );
    assert!(sphere_drag(1e6, 0.0) < 0.25, "past the crisis");
    // Through the sound barrier the drag rises to about one.
    assert!((sphere_drag(1e6, 1.2) - 0.98).abs() < 1e-9);
}

/// Lifting-line slopes: a₀ for an endless wing, πA/2 (slender-wing theory) for a short one,
/// and for A = 6 within 4 % of Prandtl's a₀/(1 + a₀/πA) (which overstates short wings).
#[test]
fn wing_slopes_follow_lifting_line_theory() {
    let a0 = 2.0 * PI;
    assert!((wing_slope(a0, 1e6) / a0 - 1.0).abs() < 1e-5);
    assert!((wing_slope(a0, 0.01) / (PI * 0.01 / 2.0) - 1.0).abs() < 1e-3);
    let prandtl = a0 / (1.0 + a0 / (PI * 6.0));
    assert!((wing_slope(a0, 6.0) / prandtl - 1.0).abs() < 0.04);
}

/// A flat plate at small angles lifts as the theory says, C_L = a·α, its drag only the
/// induced C_L²/πeA; broadside a square plate's drag is Hoerner's 1.17 (within 5 %); past
/// the stall the lift falls; backwards a plate lifts as forwards.
#[test]
fn a_flat_plate_lifts_and_stalls() {
    let plate = Section::PLATE;
    let a = wing_slope(2.0 * PI, 6.0);
    let alpha = 3.0 * PI / 180.0;
    let (cl, cd) = lift_drag(&plate, 6.0, 0.9, alpha, 0.0);
    assert!((cl / (a * alpha) - 1.0).abs() < 1e-12, "{cl}");
    assert!((cd - cl * cl / (PI * 0.9 * 6.0)).abs() < 1e-15);
    let (_, broadside) = lift_drag(&plate, 1.0, 0.9, 0.5 * PI, 0.0);
    assert!((broadside / 1.17 - 1.0).abs() < 0.05, "{broadside}");
    let peak = lift_drag(&plate, 6.0, 0.9, 8.0 * PI / 180.0, 0.0).0;
    let stalled = lift_drag(&plate, 6.0, 0.9, 14.0 * PI / 180.0, 0.0).0;
    assert!(stalled < peak, "{stalled} past the stall, {peak} at it");
    let back = lift_drag(&plate, 6.0, 0.9, PI - alpha, 0.0);
    assert!(
        (back.0 + cl).abs() < 1e-12 && (back.1 - cd).abs() < 1e-12,
        "{back:?}"
    );
}

/// Supersonic: Ackeret's C_L = 4α/√(M² − 1) at small angles; behind a normal shock the
/// pitot pressure coefficient is 1.657 at Mach 2 and 1.839 hypersonic; Prandtl–Glauert's
/// 1/√(1 − M²) on an endless wing's slope.
#[test]
fn compressibility_follows_its_theories() {
    let alpha = 2.0 * PI / 180.0;
    let (cl, _) = lift_drag(&Section::PLATE, 1e6, 1.0, alpha, 2.0);
    let ackeret = 4.0 * alpha / 3f64.sqrt();
    assert!((cl / ackeret - 1.0).abs() < 1e-3, "{cl} against {ackeret}");
    assert!(
        (stagnation_cp(2.0) - 1.657).abs() < 2e-3,
        "{}",
        stagnation_cp(2.0)
    );
    assert!((stagnation_cp(1e4) - 1.839).abs() < 1e-3);
    let (low, _) = lift_drag(&Section::PLATE, 1e9, 1.0, alpha, 0.0);
    let (fast, _) = lift_drag(&Section::PLATE, 1e9, 1.0, alpha, 0.6);
    assert!((fast / low - 1.25).abs() < 1e-6, "{}", fast / low);
}

/// Flat-plate friction: Blasius's 1.328/√Re laminar (0.0042 at Re 10⁵), Prandtl–Schlichting
/// turbulent (0.00284 at Re 10⁷, Schlichting fig. 21.2), continuous at the transition.
#[test]
fn skin_friction_follows_the_flat_plate() {
    assert!((skin_friction(1e5, 0.0) - 1.328 / 1e5f64.sqrt()).abs() < 1e-15);
    assert!(
        (skin_friction(1e7, 0.0) - 0.002_84).abs() < 5e-5,
        "{}",
        skin_friction(1e7, 0.0)
    );
    let (below, above) = (
        skin_friction(4.999_999e5, 0.0),
        skin_friction(5.000_001e5, 0.0),
    );
    assert!((above / below - 1.0).abs() < 0.01, "{below} {above}");
}

/// A 1 m² plate at 3° in 50 m/s air, from its force: lift and drag as the coefficients say,
/// friction on both faces along the flow.
#[test]
fn a_panel_force_resolves_to_lift_and_drag() {
    let panel = Surface {
        shape: Shape::Panel {
            area: 1.0,
            aspect: 4.0,
            efficiency: 0.9,
            section: Section::PLATE,
            normal: DVec3::Y,
            chord: DVec3::X,
        },
        offset: DVec3::ZERO,
    };
    let alpha = 3.0 * PI / 180.0;
    // Flying along +x, descending at α: the air meets it from below.
    let (sin, cos) = creeer_math::det::sin_cos(alpha);
    let v = DVec3::new(cos, -sin, 0.0) * 50.0;
    let f = panel.force(DMat3::IDENTITY, v, &AIR);
    let q = 0.5 * AIR.density * 2500.0;
    let (cl, cd) = lift_drag(&Section::PLATE, 4.0, 0.9, alpha, 50.0 / AIR.sound);
    let along = -v.normalize();
    let lift = along.cross(DVec3::Z).normalize();
    assert!(lift.y > 0.0);
    let cf = skin_friction(AIR.density * 50.0 * 0.5 / AIR.viscosity, 50.0 / AIR.sound);
    let friction = 2.0 * q * cf * cos;
    assert!(
        (f.dot(lift) / q - (cl - 2.0 * cf * cos * sin)).abs() < 1e-9,
        "lift {}",
        f.dot(lift) / q
    );
    assert!(
        (f.dot(along) - q * cd - friction * cos).abs() < 1e-6 * q,
        "drag {}",
        f.dot(along)
    );
}

/// The dashpot: a sphere's quadratic drag F = −k|v|v has slope −k(|v|·I + v·vᵀ/|v|), all of
/// it damping; the positive part of an indefinite matrix keeps only its positive half.
#[test]
fn the_slope_is_the_damping() {
    let sphere = Surface {
        shape: Shape::Sphere { diameter: 0.02 },
        offset: DVec3::ZERO,
    };
    // Re ≈ 4·10⁴, where the drag curve is flat (nearer the crisis its fall bends the slope).
    let v = DVec3::new(30.0, -10.0, 5.0);
    let (f, d) = sphere.linearised(DMat3::IDENTITY, v, &AIR);
    let k = -f.dot(v) / (v.length() * v.length() * v.length());
    let speed = v.length();
    let exact = DMat3::IDENTITY * speed + DMat3::from_cols(v * v.x, v * v.y, v * v.z) / speed;
    // (Its drag coefficient held where it is: the slope is the formula's, to rounding.)
    let off = (d - exact * k)
        .to_cols_array()
        .iter()
        .fold(0.0f64, |m, x| m.max(x.abs()));
    assert!(off < 1e-12 * k * speed, "{d:?} against {:?}", exact * k);
    let m = DMat3::from_cols(
        DVec3::new(2.0, 1.0, 0.0),
        DVec3::new(1.0, -3.0, 0.5),
        DVec3::new(0.0, 0.5, 1.0),
    );
    let p = positive_part(m);
    for e in [DVec3::X, DVec3::Y, DVec3::Z, DVec3::ONE.normalize()] {
        assert!(e.dot(p * e) >= -1e-12);
    }
    // Its trace is the positive eigenvalues' sum; the full matrix's, all three.
    let all = m.x_axis.x + m.y_axis.y + m.z_axis.z;
    let kept = p.x_axis.x + p.y_axis.y + p.z_axis.z;
    assert!(
        kept > all
            && (p - p.transpose())
                .to_cols_array()
                .iter()
                .all(|x| x.abs() < 1e-12)
    );
    assert!(
        ((positive_part(DMat3::IDENTITY * 2.0) - DMat3::IDENTITY * 2.0)
            .to_cols_array()
            .iter()
            .all(|x| x.abs() < 1e-14))
    );
}

/// Heating: a panel at its recovery temperature, T(1 + r·(γ − 1)/2·M²) with r = Pr^⅓, takes
/// no heat; colder it takes heat, warmer it gives. At Mach 2 that is 1.71 T.
#[test]
fn a_wall_at_recovery_takes_no_heat() {
    let panel = Surface {
        shape: Shape::Panel {
            area: 1.0,
            aspect: 1.0,
            efficiency: 0.9,
            section: Section::PLATE,
            normal: DVec3::Y,
            chord: DVec3::X,
        },
        offset: DVec3::ZERO,
    };
    let v = DVec3::new(2.0 * AIR.sound, 0.0, 0.0);
    let recovery = AIR.t * (1.0 + creeer_math::det::cbrt(0.71) * 0.2 * 4.0);
    assert!((recovery / AIR.t - 1.713).abs() < 1e-3);
    assert!(panel.heating(v, &AIR, recovery).abs() < 1e-6);
    assert!(panel.heating(v, &AIR, 300.0) > 0.0 && panel.heating(v, &AIR, 600.0) < 0.0);
}

/// The positive part against its definition, on matrices with one, two and three negative
/// eigenvalues and repeated ones: V·max(Λ, 0)·Vᵀ, built from known eigenvectors.
#[test]
fn the_positive_part_is_exact() {
    let r = DMat3::from_quat(creeer_math::glam::DQuat::from_axis_angle(
        DVec3::new(1.0, 2.0, -0.5).normalize(),
        0.7,
    ));
    for lambda in [
        [3.0, 1.0, -2.0],
        [2.0, -1.0, -4.0],
        [-1.0, -2.0, -3.0],
        [5.0, 5.0, -1.0],
        [2.0, -3.0, -3.0],
        [4.0, 0.0, -1.0],
    ] {
        let m = r * DMat3::from_diagonal(DVec3::from_array(lambda)) * r.transpose();
        let kept = r
            * DMat3::from_diagonal(DVec3::from_array(lambda.map(|l: f64| l.max(0.0))))
            * r.transpose();
        let off = (positive_part(m) - kept)
            .to_cols_array()
            .iter()
            .fold(0.0f64, |a, x| a.max(x.abs()));
        assert!(off < 1e-12, "{lambda:?}: off by {off}");
    }
}

/// The panel's analytic slope against finite differences of its force (coefficients held as
/// the slope holds them), across attached, stalled, reversed and supersonic flow, sideslip
/// included.
#[test]
fn the_panel_slope_is_its_forces() {
    let shape = Shape::Panel {
        area: 0.7,
        aspect: 5.0,
        efficiency: 0.9,
        section: Section::NACA_2412,
        normal: DVec3::Y,
        chord: DVec3::X,
    };
    let panel = Surface {
        shape,
        offset: DVec3::ZERO,
    };
    let turn = DMat3::from_quat(creeer_math::glam::DQuat::from_axis_angle(
        DVec3::new(0.3, 1.0, 0.2).normalize(),
        0.4,
    ));
    for v in [
        DVec3::new(40.0, -3.0, 1.0),
        DVec3::new(30.0, -12.0, -4.0),
        DVec3::new(-25.0, 2.0, 6.0),
        DVec3::new(900.0, -60.0, 20.0),
    ] {
        let v = turn * v;
        let (f, frozen) = panel.evaluate(turn, v, &AIR, None);
        let Shape::Panel {
            area,
            aspect,
            efficiency,
            section,
            normal,
            chord,
        } = shape
        else {
            unreachable!()
        };
        let p = super::slope::Panel {
            area,
            aspect,
            efficiency,
            section: &section,
            n: turn * normal,
            c: turn * chord,
        };
        let (force, j) = super::slope::slope(&p, -v, AIR.density, frozen.0, frozen.1);
        assert!(
            (force - f).length() < 1e-12 * f.length(),
            "{force} against {f}"
        );
        let h = 1e-6 * v.length();
        for k in 0..3 {
            let mut u = v;
            u[k] -= h;
            let mut w = v;
            w[k] += h;
            // dF/du along k is −dF/dv.
            let fd = (panel.evaluate(turn, u, &AIR, Some(frozen)).0
                - panel.evaluate(turn, w, &AIR, Some(frozen)).0)
                / (2.0 * h);
            let off = (j.col(k) - fd).length();
            assert!(
                off < 1e-5 * f.length() / v.length() * 10.0 + 1e-9,
                "v {v}, column {k}: {} against {fd}",
                j.col(k)
            );
        }
    }
}
