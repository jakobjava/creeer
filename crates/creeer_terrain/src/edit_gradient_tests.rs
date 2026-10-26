//! The edited density's gradient (`edit_gradient.rs`): exact around a narrow dug hole, its
//! rounded rim included, and of unit length up to the hole's axis, where the projections of
//! physics' ground contacts start.

use crate::edits::{Brush, Edit};
use crate::elevation::fibonacci_sphere;
use crate::{Planet, Terrain};
use creeer_math::glam::DVec3;

/// The home planet with a hole dug as the player's blows dig one (15 cm across in radius,
/// 0.7 m deep, G2.9) on gentle land: the terrain, the hole's mouth and the way down.
fn holed() -> (Terrain, DVec3, DVec3) {
    let t = Terrain::new(
        Planet::load(&creeer_data::Data::base(), "planets/home.ron").expect("home.ron"),
    );
    let up = fibonacci_sphere(20_000)
        .find(|&d| (50.0..300.0).contains(&t.surface_height(d)))
        .expect("gentle land");
    let mouth = up * (t.planet().datum.radius(up) + t.surface_height(up));
    let down = -mouth.normalize();
    t.edit(Edit::Dig(Brush::Capsule {
        a: mouth,
        b: mouth + down * 0.7,
        radius: 0.15,
    }));
    (t, mouth, down)
}

/// The gradient agrees with fine central differences of the same field over the hole, its
/// walls, its rounded rim and the ground around, and its value is the density's, bit for bit.
#[test]
fn the_gradient_around_a_dug_hole_is_exact() {
    let (t, mouth, down) = holed();
    let (east, north) = down.any_orthonormal_pair();
    let fine = |q: DVec3| {
        let h = 1e-5;
        DVec3::new(
            t.density(q + DVec3::X * h) - t.density(q - DVec3::X * h),
            t.density(q + DVec3::Y * h) - t.density(q - DVec3::Y * h),
            t.density(q + DVec3::Z * h) - t.density(q - DVec3::Z * h),
        ) / (2.0 * h)
    };
    let mut checked = 0;
    for i in 0..=24 {
        for k in 0..=12 {
            // From the axis out to 0.6 m (past the round) and from 0.3 m above the mouth to
            // 0.6 m down the hole; a little off the axis, where the distance has its ridge, and
            // off the level of the brush's end, where its curvature jumps from 0 to 1/r (the
            // differences across it are off by h/4r: 1.2·10⁻³ 2 mm from the axis).
            let across = 0.002 + 0.6 * f64::from(i) / 24.0;
            let depth = -0.29 + 0.9 * f64::from(k) / 12.0;
            let p = mouth + down * depth + (east * 0.8 + north * 0.6) * across;
            let (d, g) = t.density_gradient(p);
            assert_eq!(
                d.to_bits(),
                t.density(p).to_bits(),
                "value at {across}, {depth}"
            );
            let f = fine(p);
            assert!(
                (g - f).length() < 1e-4 * f.length().max(1.0),
                "{across:.3} m out, {depth:.2} m down: {g} against {f}"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 25 * 13);
}

/// Next to the hole's axis the gradient keeps its unit length, pointing to the axis from the
/// nearer wall, and a projection as physics' (`creeer_physics::world::ground::project`, two
/// Newton steps d/|∇d|) lands on that wall. With central differences over 1 cm the gradient
/// shrank to r/1 cm within 1 cm of the axis (0.03 at 0.3 mm), and the step threw the foot of
/// a point 15 cm from the wall 5 m away (G2.9).
#[test]
fn next_to_a_holes_axis_the_gradient_points_to_the_nearer_wall() {
    let (t, mouth, down) = holed();
    let side = down.any_orthonormal_vector();
    let axis = mouth + down * 0.3;
    for off in [3e-4, 3e-3, 3e-2] {
        let p = axis + side * off;
        let (d, g) = t.density_gradient(p);
        assert!(
            (g.length() - 1.0).abs() < 1e-9,
            "{off} m off: |∇d| = {}",
            g.length()
        );
        assert!(g.dot(side) < -0.999, "{off} m off: towards the axis, {g}");
        assert!((d - (0.15 - off)).abs() < 1e-9, "{off} m off: d = {d}");
        let mut q = p;
        for _ in 0..2 {
            let (d, g) = t.density_gradient(q);
            q -= g * (d / g.length_squared());
        }
        let wall = axis + side * 0.15;
        assert!(
            q.distance(wall) < 1e-6,
            "{off} m off: foot {:.4} m from the wall",
            q.distance(wall)
        );
    }
    // On the axis itself (the brush's end, at the mouth) the brush gives no slope, not the
    // world's y (`Brush::gradient`): what is left is the ground's share of the round there,
    // about none.
    let (_, g) = t.density_gradient(mouth);
    assert!(g.is_finite() && g.length() < 0.01, "on the axis: {g}");
}
