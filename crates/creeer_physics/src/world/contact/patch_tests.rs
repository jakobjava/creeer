//! Tests of the patch law: its integrals, the flat punch, and the series with the limit.

use super::cap::Cap;
use super::integrals::{pressed_mass, square_integral};
use super::patch::Patch;
use crate::world::shape::Domain;
use creeer_math::glam::DVec3;

/// ∫ max(f, 0)² over the unit right triangle by a fine midpoint rule.
fn numeric(f: [f64; 3]) -> f64 {
    let n = 400;
    let mut sum = 0.0;
    for i in 0..n {
        for j in 0..n - i {
            for (u, v, w) in [
                (
                    (f64::from(i) + 1.0 / 3.0) / f64::from(n),
                    (f64::from(j) + 1.0 / 3.0) / f64::from(n),
                    0.5,
                ),
                (
                    (f64::from(i) + 2.0 / 3.0) / f64::from(n),
                    (f64::from(j) + 2.0 / 3.0) / f64::from(n),
                    0.5,
                ),
            ] {
                if u + v > 1.0 {
                    continue;
                }
                let value = f[0] * (1.0 - u - v) + f[1] * u + f[2] * v;
                let positive = value.max(0.0);
                sum += positive * positive * w / f64::from(n * n);
            }
        }
    }
    sum
}

#[test]
fn triangle_integrals_match_quadrature_and_their_gradients() {
    let cases = [
        [1.0, 2.0, 3.0],
        [1.0, -2.0, -0.5],
        [2.0, 1.5, -1.0],
        [-1.0, -2.0, -3.0],
        [0.5, 0.0, -0.5],
        [-0.3, 0.7, 0.2],
    ];
    for f in cases {
        let (value, gradient) = square_integral(f, 0.5);
        let want = numeric(f);
        assert!(
            (value - want).abs() < 2e-3 * want.max(1e-3),
            "{f:?}: {value} vs {want}"
        );
        for k in 0..3 {
            let e = 1e-6;
            let (mut up, mut down) = (f, f);
            up[k] += e;
            down[k] -= e;
            let fd = (square_integral(up, 0.5).0 - square_integral(down, 0.5).0) / (2.0 * e);
            assert!(
                (fd - gradient[k]).abs() < 1e-6,
                "{f:?} ∂{k}: {fd} vs {}",
                gradient[k]
            );
        }
    }
}

#[test]
fn a_square_patch_is_a_flat_punch_and_tilts_smoothly() {
    let corners = [
        DVec3::new(1.0, 0.0, 1.0),
        DVec3::new(-1.0, 0.0, 1.0),
        DVec3::new(-1.0, 0.0, -1.0),
        DVec3::new(1.0, 0.0, -1.0),
    ];
    let mut patch = Patch::new(0, corners, &Domain::WHOLE, DVec3::Y, 1.0e9);
    patch.set_cap(Cap::NONE);
    // Pressed in evenly: the punch's stiffness 2·E*·√(A/π) on the whole face, shared
    // equally by the corners.
    let depth = 1e-6;
    let (energy, forces) = patch.evaluate([depth; 4]);
    let stiffness = 2.0e9 * (4.0 / std::f64::consts::PI).sqrt();
    assert!((energy / (0.5 * stiffness * depth * depth) - 1.0).abs() < 1e-12);
    for f in forces {
        assert!(
            (f / (0.25 * stiffness * depth) - 1.0).abs() < 1e-12,
            "{forces:?}"
        );
    }
    // Resting on one edge (two corners pressed in, two lifted as far): half the face
    // touches, and the load grows as the square of the depth, from nothing.
    let tilt = |d: f64| patch.evaluate([d, -d, -d, d]).1.iter().sum::<f64>();
    let (f1, f2) = (tilt(1e-6), tilt(2e-6));
    assert!((f2 / f1 - 2.0).abs() < 1e-9);
    assert!(f1 < 0.5 * stiffness * 1e-6);
    // Lumped stiffnesses add up to the punch's.
    let lumped: f64 = (0..4).map(|v| patch.vertex_stiffness(v)).sum();
    assert!((lumped / stiffness - 1.0).abs() < 1e-12);
    // Half the face clipped away: half the area presses, through the corners on its side.
    let half = Domain::part(&[[0.0, 0.0], [0.5, 0.0], [0.5, 1.0], [0.0, 1.0]]);
    let clipped = Patch::new(0, corners, &half, DVec3::Y, 1.0e9);
    let (_, forces) = clipped.evaluate([depth; 4]);
    let on_side = forces[0] + forces[3];
    assert!(on_side > 2.5 * (forces[1] + forces[2]), "{forces:?}");
    assert!((clipped.area - 2.0).abs() < 1e-12);
}

/// A 2 m square face on the ground plane, corners in order.
fn square() -> Patch {
    let corners = [
        DVec3::new(1.0, 0.0, 1.0),
        DVec3::new(-1.0, 0.0, 1.0),
        DVec3::new(-1.0, 0.0, -1.0),
        DVec3::new(1.0, 0.0, -1.0),
    ];
    Patch::new(0, corners, &Domain::WHOLE, DVec3::Y, 1.0e11)
}

#[test]
fn the_pressed_mass_is_half_the_integral_s_hessian() {
    for f in [
        [1.0, 2.0, 3.0],
        [1.0, -2.0, -0.5],
        [2.0, 1.5, -1.0],
        [-0.3, 0.7, 0.2],
    ] {
        let mass = pressed_mass(f, 0.5);
        for b in 0..3 {
            let e = 1e-6;
            let (mut up, mut down) = (f, f);
            up[b] += e;
            down[b] -= e;
            let (gu, gd) = (square_integral(up, 0.5).1, square_integral(down, 0.5).1);
            for a in 0..3 {
                let fd = (gu[a] - gd[a]) / (2.0 * e);
                assert!((fd - 2.0 * mass[a][b]).abs() < 1e-6, "{f:?} [{a}][{b}]");
            }
        }
    }
}

/// Pressed in all over, the face is the material in series with the limit: pushed in it
/// has their series stiffness, and its corners' stiffnesses for the matrix add up to it.
#[test]
fn a_face_pressed_all_over_is_the_material_in_series_with_the_limit() {
    let mut patch = square();
    let punch = 2.0e11 * (4.0 / std::f64::consts::PI).sqrt();
    let limit = 1.0e8;
    patch.set_cap(Cap {
        limit,
        reach: 1.0,
        carried: 0.0,
    });
    let series = 1.0 / (1.0 / punch + 1.0 / limit);
    let depth = 1e-4;
    let (energy, forces) = patch.evaluate([depth; 4]);
    assert!(
        (energy / (0.5 * series * depth * depth) - 1.0).abs() < 1e-9,
        "{energy}"
    );
    let total: f64 = forces.iter().sum();
    assert!((total / (series * depth) - 1.0).abs() < 1e-9, "{forces:?}");
    let matrix: f64 = (0..4)
        .flat_map(|k| (0..4).map(move |l| (k, l)))
        .map(|(k, l)| patch.coupling(k, l))
        .sum();
    assert!((matrix / series - 1.0).abs() < 1e-9, "{matrix}");
    // Tilted (one edge in, the other out by as much, still pressed all over): the limit
    // times J/A about its axis, J/A = 1/12 of the side² in the face's coordinates.
    let tilt = 1e-5;
    let (energy, _) = patch.evaluate([depth + tilt, depth - tilt, depth - tilt, depth + tilt]);
    let pushed = 0.5 * series * depth * depth;
    let tilting = 0.5 / (1.0 / (punch / 12.0) + 1.0 / (limit / 12.0)) * (2.0 * tilt) * (2.0 * tilt);
    assert!(((energy - pushed) / tilting - 1.0).abs() < 1e-6, "{energy}");
}

/// Barely pressed in near one corner, the face is far softer than the limit there (the
/// pressed area grows as the depth squared) and follows the material; its forces are the
/// slope of its energy throughout.
#[test]
fn a_face_pressed_near_a_corner_follows_the_material() {
    let free = square();
    let mut patch = square();
    patch.set_cap(Cap {
        limit: 1.0e8,
        reach: 1.0,
        carried: 0.0,
    });
    let depths = [1e-6, -1e-3, -2e-3, -1e-3];
    let (material, _) = free.evaluate(depths);
    let (energy, forces) = patch.evaluate(depths);
    assert!(
        energy <= material && energy > 0.99 * material,
        "{energy} vs {material}"
    );
    for depths in [depths, [3e-4, 1e-4, -2e-4, 5e-5], [1e-4, 1e-4, 1e-4, 1e-4]] {
        let (_, forces) = patch.evaluate(depths);
        // A step small against the shallowest depth (the region pressed at a corner is tiny).
        let e = 1e-4 * depths.iter().map(|d| d.abs()).fold(f64::INFINITY, f64::min);
        for k in 0..4 {
            let (mut up, mut down) = (depths, depths);
            up[k] += e;
            down[k] -= e;
            let fd = (patch.evaluate(up).0 - patch.evaluate(down).0) / (2.0 * e);
            assert!(
                (fd - forces[k]).abs() < 1e-5 * forces.iter().map(|f| f.abs()).sum::<f64>(),
                "{depths:?} corner {k}: {fd} vs {}",
                forces[k]
            );
        }
    }
    assert!(forces[0] > 0.0);
}

/// A face pressed in all over and carrying a hundred times the weight of the bodies it
/// joins sinks a reach per e-fold of that, d·(1 + ln 100), plus the material's own
/// compression: under load its push stiffens with its pressure.
#[test]
fn a_loaded_face_sinks_a_reach_per_e_fold_of_its_load() {
    let mut patch = square();
    let (limit, reach) = (1.0e8, 1.0e-5);
    patch.set_cap(Cap {
        limit,
        reach,
        carried: 1.0e7,
    });
    let load = 100.0 * limit * reach;
    let total = |depth: f64| patch.evaluate([depth; 4]).1.iter().sum::<f64>();
    let (mut lo, mut hi) = (0.0, 1e-2);
    for _ in 0..100 {
        let mid = 0.5 * (lo + hi);
        if total(mid) < load {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let punch = 2.0e11 * (4.0 / std::f64::consts::PI).sqrt();
    let want = reach * (1.0 + creeer_math::det::ln(100.0)) + load / punch;
    assert!(
        (lo / want - 1.0).abs() < 1e-3,
        "sank {lo:e} m, expected {want:e}"
    );
}

/// A face pressed in anyhow (evenly, tilted, near a corner, along a thin strip at an edge),
/// in series with a limit or not: each corner's force is the energy's slope there, to 10⁻⁶.
#[test]
fn forces_are_the_energy_s_slope_however_the_face_presses() {
    let corners = [
        DVec3::new(0.15, 0.0, 0.4),
        DVec3::new(-0.15, 0.0, 0.4),
        DVec3::new(-0.15, 0.0, -0.4),
        DVec3::new(0.15, 0.0, -0.4),
    ];
    for capped in [false, true] {
        let mut patch = Patch::new(0, corners, &Domain::WHOLE, DVec3::Y, 4.0e10);
        patch.set_cap(if capped {
            super::cap::cap(50.0, 1.0 / 60.0, 490.0)
        } else {
            Cap::NONE
        });
        for depths in [
            [2e-5, 2e-5, 2e-5, 2e-5],
            [3e-5, -1e-5, -1e-5, 3e-5],
            [2e-5, -3e-5, -4e-5, -3e-5],
            [1e-6, -0.3, -0.3, 1e-6],
            [1e-6, -0.3, -0.3, 2e-6],
        ] {
            patch.prepare(depths);
            let (_, forces) = patch.evaluate(depths);
            let scale = forces.iter().map(|f| f.abs()).fold(0.0, f64::max);
            for k in 0..4 {
                let h = 1e-6 * depths.iter().map(|d| d.abs()).fold(0.0, f64::max).min(1e-6) + 1e-12;
                let (mut up, mut down) = (depths, depths);
                up[k] += h;
                down[k] -= h;
                let slope = (patch.evaluate(up).0 - patch.evaluate(down).0) / (2.0 * h);
                assert!(
                    (slope - forces[k]).abs() <= 1e-6 * scale + 1e-9,
                    "capped {capped} {depths:?} corner {k}: {} vs slope {slope}",
                    forces[k]
                );
            }
        }
    }
}

/// The material's energy, a punch of the pressed area's own stiffness, is convex in the
/// face's rigid motions (the step's minimisation relies on it): at the midpoint of any two
/// positions of the face it is at most the mean of their energies (5,000 random pairs, from
/// lifted to pressed all over, pressed at a corner, along an edge). A rigid face's corner
/// depths lie in a plane, d₀ + d₂ = d₁ + d₃.
#[test]
fn the_material_s_energy_is_convex() {
    let corners = [
        DVec3::new(0.3, 0.0, 0.2),
        DVec3::new(-0.3, 0.0, 0.2),
        DVec3::new(-0.3, 0.0, -0.2),
        DVec3::new(0.3, 0.0, -0.2),
    ];
    let triangle = Domain::part(&[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
    let mut state = 7u64;
    let mut next = move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        #[allow(clippy::cast_precision_loss)] // 53 bits of it
        let unit = (state >> 11) as f64 / (1u64 << 53) as f64;
        2.0 * unit - 1.0
    };
    for domain in [Domain::WHOLE, triangle] {
        let patch = Patch::new(0, corners, &domain, DVec3::Y, 1.0e10);
        for _ in 0..2500 {
            let mut plane = || {
                let (d, s, t) = (next() * 1e-4, next() * 2e-4, next() * 2e-4);
                [d, d + s, d + s + t, d + t]
            };
            let (a, b) = (plane(), plane());
            let mid: [f64; 4] = std::array::from_fn(|k| 0.5 * (a[k] + b[k]));
            let (ea, eb, em) = (
                patch.evaluate(a).0,
                patch.evaluate(b).0,
                patch.evaluate(mid).0,
            );
            assert!(
                em <= 0.5 * (ea + eb) * (1.0 + 1e-12) + 1e-18,
                "{a:?} {b:?}: {em} > {}",
                0.5 * (ea + eb)
            );
        }
    }
}
