use super::{DualQuat, linear_blend};
use creeer_math::det;
use creeer_math::glam::{Quat, Vec3};

fn close(a: Vec3, b: Vec3, tolerance: f32) -> bool {
    (a - b).length() <= tolerance
}

/// A few rigid motions: turns about awkward axes, by small and large angles, far off.
fn motions() -> Vec<(Quat, Vec3)> {
    vec![
        (Quat::IDENTITY, Vec3::ZERO),
        (
            Quat::from_axis_angle(Vec3::Y, 0.3),
            Vec3::new(1.0, 2.0, 3.0),
        ),
        (
            Quat::from_axis_angle(Vec3::new(1.0, -2.0, 0.5).normalize(), 2.9),
            Vec3::new(-40.0, 0.5, 12.0),
        ),
        (
            Quat::from_axis_angle(Vec3::X, std::f32::consts::PI),
            Vec3::new(0.0, -1.0, 0.0),
        ),
    ]
}

#[test]
fn one_bone_moves_its_vertices_rigidly() {
    let points = [
        Vec3::ZERO,
        Vec3::new(0.3, 1.7, -0.2),
        Vec3::new(-0.25, 0.9, 0.12),
    ];
    for (rotation, translation) in motions() {
        let dq = DualQuat::from_rigid(rotation, translation);
        assert!(close(dq.translation(), translation, 1e-5));
        // Alone, or blended with itself on the other side (−q is the same motion).
        let flipped = DualQuat {
            real: -dq.real,
            dual: -dq.dual,
        };
        for blended in [
            DualQuat::blend(&[(dq, 1.0)]),
            DualQuat::blend(&[(dq, 0.5), (flipped, 0.5)]),
        ] {
            for p in points {
                let rigid = rotation * p + translation;
                assert!(close(blended.point(p), rigid, 2e-5), "{p}");
                let n = Vec3::new(0.6, 0.0, 0.8);
                assert!(close(blended.direction(n), rotation * n, 1e-6));
            }
        }
    }
}

#[test]
fn the_rest_pose_is_the_mesh_itself() {
    let rest = DualQuat::from_rigid(Quat::IDENTITY, Vec3::ZERO);
    let p = Vec3::new(0.1, 1.2, -0.3);
    let blended = DualQuat::blend(&[(rest, 0.25), (rest, 0.5), (rest, 0.25)]);
    assert_eq!(blended.point(p), p);
}

/// A closed tube along +y, `length` long and `radius` round, in rings `step` apart, each
/// ring's points with the share of the second bone `upper(y)`.
fn tube(length: f32, radius: f32, step: f32, sides: u32) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let rings = (length / step).round() as u32 + 1;
    let mut points = Vec::new();
    for r in 0..rings {
        #[allow(clippy::cast_precision_loss)]
        let y = r as f32 * step;
        for s in 0..sides {
            let a = std::f64::consts::TAU * f64::from(s) / f64::from(sides);
            let (sin, cos) = det::sin_cos(a);
            #[allow(clippy::cast_possible_truncation)]
            points.push(Vec3::new(radius * cos as f32, y, radius * sin as f32));
        }
    }
    let mut triangles = Vec::new();
    for r in 0..rings - 1 {
        for s in 0..sides {
            let (a, b) = (r * sides + s, r * sides + (s + 1) % sides);
            let (c, d) = (a + sides, b + sides);
            triangles.push([a, c, b]);
            triangles.push([b, c, d]);
        }
    }
    // Caps, fanned from each end's middle.
    let bottom = u32::try_from(points.len()).expect("small");
    points.push(Vec3::ZERO);
    let top = bottom + 1;
    points.push(Vec3::new(0.0, length, 0.0));
    let last = (rings - 1) * sides;
    for s in 0..sides {
        triangles.push([bottom, s, (s + 1) % sides]);
        triangles.push([top, last + (s + 1) % sides, last + s]);
    }
    (points, triangles)
}

/// The volume a closed mesh encloses (divergence theorem).
fn volume(points: &[Vec3], triangles: &[[u32; 3]]) -> f32 {
    triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| points[i as usize]);
            a.dot(b.cross(c)) / 6.0
        })
        .sum()
}

/// The plan's measure (G1.3e): an elbow bent by 90° keeps its volume within 2 %. The tube's
/// lower half on one bone, its upper half on another turned 90° about the joint, the share
/// passing from one to the other over 10 cm each side of it (as the engineer's elbows are
/// weighted); linear blending measured beside.
#[test]
fn an_elbow_bent_ninety_degrees_keeps_its_volume() {
    let (rest, triangles) = tube(2.0, 0.1, 0.02, 48);
    let rest_volume = volume(&rest, &triangles);
    let joint = Vec3::new(0.0, 1.0, 0.0);
    let turn = Quat::from_axis_angle(Vec3::X, std::f32::consts::FRAC_PI_2);
    let lower = (Quat::IDENTITY, Vec3::ZERO);
    let upper = (turn, joint - turn * joint);
    let share = |y: f32| ((y - 0.9) / 0.2).clamp(0.0, 1.0);
    let bones = [
        DualQuat::from_rigid(lower.0, lower.1),
        DualQuat::from_rigid(upper.0, upper.1),
    ];
    let dq: Vec<Vec3> = rest
        .iter()
        .map(|&p| {
            let w = share(p.y);
            DualQuat::blend(&[(bones[0], 1.0 - w), (bones[1], w)]).point(p)
        })
        .collect();
    let lbs: Vec<Vec3> = rest
        .iter()
        .map(|&p| {
            let w = share(p.y);
            linear_blend(&[(lower, 1.0 - w), (upper, w)], p)
        })
        .collect();
    let (dq_volume, lbs_volume) = (volume(&dq, &triangles), volume(&lbs, &triangles));
    let change = |v: f32| (v - rest_volume) / rest_volume;
    println!(
        "bent 90°: dual quaternions {:+.2} %, linear blending {:+.2} % of the volume",
        100.0 * change(dq_volume),
        100.0 * change(lbs_volume)
    );
    assert!(change(dq_volume).abs() < 0.02);
    // Linear blending loses more: the joint's inside collapses.
    assert!(change(lbs_volume) < change(dq_volume));
}
