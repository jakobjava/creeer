//! Correctness of the beam element's mathematics.

use super::element::{Element, MEASURES};
use super::section::{STEEL_S235, Section};
use crate::so3;
use creeer_math::glam::{DMat3, DQuat, DVec3};

fn element() -> Element {
    Element::new(
        (0, DVec3::new(0.1, 0.2, -0.3)),
        (1, DVec3::new(2.0, 0.5, -0.7)),
        DVec3::Y,
        Section::rectangle(0.08, 0.12),
        STEEL_S235,
    )
}

/// A noticeably deformed and rotated state.
fn deformed() -> (DVec3, DMat3, DVec3, DMat3) {
    (
        DVec3::new(0.15, 0.1, -0.25),
        DMat3::from_quat(so3::exp(DVec3::new(0.2, -0.35, 0.1))),
        DVec3::new(1.9, 0.75, -0.6),
        DMat3::from_quat(so3::exp(DVec3::new(-0.1, 0.3, 0.45))),
    )
}

/// Moves the four dof groups by `step` (rotations applied on the left, in the world frame).
fn perturb(state: (DVec3, DMat3, DVec3, DMat3), step: [DVec3; 4]) -> (DVec3, DMat3, DVec3, DMat3) {
    let (xa, ra, xb, rb) = state;
    let turn = |v: DVec3| DMat3::from_quat(so3::exp(v));
    (
        xa + step[0],
        turn(step[1]) * ra,
        xb + step[2],
        turn(step[3]) * rb,
    )
}

/// The same ends as a fastened joint (`fastened.rs`): its stretch is measured along a's axis.
fn joint() -> Element {
    let mut el = element();
    el.fastened = Some(super::fastened::Fastened {
        withdrawal: 1.0,
        lateral: 1.0,
        interaction: 1.0,
    });
    el
}

#[test]
fn gradients_match_finite_differences() {
    for el in [element(), joint()] {
        gradients_match_for(&el);
    }
}

fn gradients_match_for(el: &Element) {
    let state = deformed();
    let (xa, ra, xb, rb) = state;
    let (s, grads) = el.kinematics(xa, ra, xb, rb);
    assert_eq!(
        s,
        el.measures(xa, ra, xb, rb),
        "both paths compute the same measures"
    );
    let direction = [
        DVec3::new(0.3, -0.7, 0.2),
        DVec3::new(-0.4, 0.1, 0.9),
        DVec3::new(0.5, 0.2, -0.6),
        DVec3::new(0.8, -0.3, -0.1),
    ];
    let h = 1e-6;
    let plus = perturb(state, direction.map(|d| d * h));
    let minus = perturb(state, direction.map(|d| d * -h));
    let sp = el.measures(plus.0, plus.1, plus.2, plus.3);
    let sm = el.measures(minus.0, minus.1, minus.2, minus.3);
    for m in 0..MEASURES {
        let numeric = (sp[m] - sm[m]) / (2.0 * h);
        let analytic: f64 = grads[m]
            .iter()
            .zip(&direction)
            .map(|(g, d)| g.dot(*d))
            .sum();
        assert!(
            (numeric - analytic).abs() < 1e-8 * (1.0 + analytic.abs()),
            "measure {m}: numeric {numeric}, analytic {analytic}"
        );
    }
}

#[test]
fn rigid_motion_measures_no_deformation() {
    for el in [element(), joint()] {
        rigid_motion_for(&el);
    }
}

fn rigid_motion_for(el: &Element) {
    let (xa, ra, xb, rb) = deformed();
    let before = el.measures(xa, ra, xb, rb);
    let q = DMat3::from_quat(DQuat::from_axis_angle(
        DVec3::new(1.0, 2.0, 3.0).normalize(),
        2.4,
    ));
    let shift = DVec3::new(100.0, -5.0, 30.0);
    let after = el.measures(q * xa + shift, q * ra, q * xb + shift, q * rb);
    for m in 0..MEASURES {
        assert!(
            (before[m] - after[m]).abs() < 1e-12,
            "measure {m}: {before:?} vs {after:?}"
        );
    }
    let rest = el.measures(
        DVec3::new(0.1, 0.2, -0.3),
        DMat3::IDENTITY,
        DVec3::new(2.0, 0.5, -0.7),
        DMat3::IDENTITY,
    );
    assert!(rest.iter().all(|s| s.abs() < 1e-15), "{rest:?}");
}

/// Stiffness matrix of the element at rest, as a function of (group, axis) pairs.
fn rest_stiffness(el: &Element, p: (usize, usize), q: (usize, usize)) -> f64 {
    let (start, end) = (DVec3::ZERO, el.axis * el.rest_length);
    let (_, g) = el.kinematics(start, DMat3::IDENTITY, end, DMat3::IDENTITY);
    let mut sum = 0.0;
    for m in 0..MEASURES {
        for n in 0..MEASURES {
            sum += el.stiffness.entry(m, n) * g[m][p.0][p.1] * g[n][q.0][q.1];
        }
    }
    sum
}

/// For small deformations the element must be exactly the textbook 3D Timoshenko frame
/// element (e.g. Przemieniecki, "Theory of Matrix Structural Analysis", §5).
#[test]
fn small_deformation_stiffness_is_the_textbook_frame_element() {
    let section = Section::rectangle(0.08, 0.12);
    let length = 1.5;
    let el = Element::new(
        (0, DVec3::ZERO),
        (1, DVec3::X * length),
        DVec3::Y,
        section,
        STEEL_S235,
    );
    let (e, g) = (STEEL_S235.youngs_modulus, STEEL_S235.shear_modulus);
    let (a, l) = (section.area, length);
    // Bending in the x-y plane (displacement along y, rotation about z) uses I_z and the
    // shear area along y; in the x-z plane I_y and the shear area along z.
    let phi_z = 12.0 * e * section.i_z / (section.shear_factor[0] * g * a * l * l);
    let phi_y = 12.0 * e * section.i_y / (section.shear_factor[1] * g * a * l * l);
    let (xa, ra, xb, rb) = (0, 1, 2, 3);
    let expected = [
        ((xa, 0), (xa, 0), e * a / l),
        ((xa, 0), (xb, 0), -e * a / l),
        ((ra, 0), (ra, 0), g * section.torsion / l),
        (
            (xa, 1),
            (xa, 1),
            12.0 * e * section.i_z / (l * l * l * (1.0 + phi_z)),
        ),
        (
            (xa, 1),
            (ra, 2),
            6.0 * e * section.i_z / (l * l * (1.0 + phi_z)),
        ),
        (
            (ra, 2),
            (ra, 2),
            (4.0 + phi_z) * e * section.i_z / (l * (1.0 + phi_z)),
        ),
        (
            (ra, 2),
            (rb, 2),
            (2.0 - phi_z) * e * section.i_z / (l * (1.0 + phi_z)),
        ),
        (
            (xa, 2),
            (xa, 2),
            12.0 * e * section.i_y / (l * l * l * (1.0 + phi_y)),
        ),
        (
            (xa, 2),
            (ra, 1),
            -6.0 * e * section.i_y / (l * l * (1.0 + phi_y)),
        ),
        (
            (ra, 1),
            (rb, 1),
            (2.0 - phi_y) * e * section.i_y / (l * (1.0 + phi_y)),
        ),
        ((xa, 1), (xa, 2), 0.0),
        ((xa, 0), (ra, 1), 0.0),
    ];
    for (p, q, want) in expected {
        let got = rest_stiffness(&el, p, q);
        let scale = e * a / l;
        assert!(
            (got - want).abs() < 1e-12 * scale,
            "K[{p:?}][{q:?}] = {got}, expected {want}"
        );
    }
}
