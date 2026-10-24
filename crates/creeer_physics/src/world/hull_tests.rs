//! Convex hulls (`hull.rs`): their construction and mass properties.

use super::hull::{Hull, HullError};
use super::shape::Shape;
use creeer_math::det;
use creeer_math::glam::{DMat3, DVec3};

/// Points spread over a ball of radius `radius` (deterministic).
pub(super) fn cloud(n: usize, radius: f64, seed: u64) -> Vec<DVec3> {
    let mut state = seed;
    let mut next = move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        #[allow(clippy::cast_precision_loss)] // 53 bits of it
        let unit = (state >> 11) as f64 / (1u64 << 53) as f64;
        2.0 * unit - 1.0
    };
    let mut points = Vec::with_capacity(n);
    while points.len() < n {
        let p = DVec3::new(next(), next(), next());
        if p.length_squared() <= 1.0 && p.length_squared() > 1e-6 {
            points.push(p.normalize() * radius);
        }
    }
    points
}

/// Every point lies inside or on every face; every face is convex, its corners in its
/// plane and counter-clockwise seen from outside; V − E + F = 2; every edge has two faces.
fn check(hull: &Hull, points: &[DVec3], centroid: DVec3) {
    let scale = hull.bounding_radius();
    for face in hull.faces() {
        for &p in points {
            assert!(face.normal.dot(p - centroid) - face.offset <= 1e-9 * scale);
        }
        let ring: Vec<DVec3> = face.ring.iter().map(|&i| hull.vertices()[i]).collect();
        let len = ring.len();
        assert!(len >= 3);
        for k in 0..len {
            let (a, b, c) = (ring[k], ring[(k + 1) % len], ring[(k + 2) % len]);
            assert!((face.normal.dot(a) - face.offset).abs() <= 1e-9 * scale);
            assert!(
                (b - a).cross(c - b).dot(face.normal) > 0.0,
                "convex, counter-clockwise"
            );
        }
    }
    let (v, e, f) = (
        hull.vertices().len(),
        hull.edges().len(),
        hull.faces().len(),
    );
    assert_eq!(v + f, e + 2, "Euler: {v} corners, {e} edges, {f} faces");
    for edge in hull.edges() {
        assert_ne!(edge.faces[0], edge.faces[1]);
    }
}

#[test]
fn a_box_of_points_makes_a_box() {
    let half = DVec3::new(0.5, 0.3, 0.2);
    let mut points: Vec<DVec3> = (0..8)
        .map(|c| {
            let s = |bit: usize| if c & bit == 0 { -1.0 } else { 1.0 };
            half * DVec3::new(s(1), s(2), s(4)) + DVec3::new(3.0, -1.0, 2.0)
        })
        .collect();
    // Points inside, on the faces and on the edges change nothing.
    points.push(DVec3::new(3.0, -1.0, 2.0));
    points.push(DVec3::new(3.0, -1.0, 2.2));
    points.push(DVec3::new(3.5, -1.3, 2.0));
    let (hull, centroid) = Hull::new(&points).expect("a box");
    assert!((centroid - DVec3::new(3.0, -1.0, 2.0)).length() < 1e-12);
    assert_eq!(hull.vertices().len(), 8);
    assert_eq!(hull.faces().len(), 6);
    assert!(hull.faces().iter().all(|f| f.ring.len() == 4));
    check(&hull, &points, centroid);
    assert!((hull.volume() / (8.0 * half.x * half.y * half.z) - 1.0).abs() < 1e-12);
    let box_inertia = Shape::Box { half }.inertia(30.0);
    let difference = hull.inertia(30.0) - box_inertia;
    for col in [difference.x_axis, difference.y_axis, difference.z_axis] {
        assert!(col.length() < 1e-12 * box_inertia.x_axis.x);
    }
}

#[test]
fn a_cloud_of_points_makes_a_hull_around_it() {
    let points = cloud(400, 1.0, 7);
    let (hull, centroid) = Hull::new(&points).expect("a ball");
    check(&hull, &points, centroid);
    // Inscribed in the unit ball, and nearly filling it.
    let ball = 4.0 / 3.0 * std::f64::consts::PI;
    assert!(
        hull.volume() < ball && hull.volume() > 0.95 * ball,
        "{}",
        hull.volume()
    );
    assert!(centroid.length() < 0.02);
}

/// A regular tetrahedron of edge a: volume a³/(6√2), inertia m·a²/20 about any axis.
#[test]
fn a_tetrahedron_has_its_mass_properties() {
    let a = 0.8;
    let s = a / 8f64.sqrt();
    let points = [
        DVec3::new(1.0, 1.0, 1.0) * s,
        DVec3::new(1.0, -1.0, -1.0) * s,
        DVec3::new(-1.0, 1.0, -1.0) * s,
        DVec3::new(-1.0, -1.0, 1.0) * s,
    ]
    .map(|p| p + DVec3::new(-2.0, 5.0, 1.0));
    let (hull, centroid) = Hull::new(&points).expect("a tetrahedron");
    check(&hull, &points, centroid);
    assert!((centroid - DVec3::new(-2.0, 5.0, 1.0)).length() < 1e-12);
    let volume = a * a * a / (6.0 * 2f64.sqrt());
    assert!((hull.volume() / volume - 1.0).abs() < 1e-12);
    let want = DMat3::IDENTITY * (10.0 * a * a / 20.0);
    let difference = hull.inertia(10.0) - want;
    for col in [difference.x_axis, difference.y_axis, difference.z_axis] {
        assert!(col.length() < 1e-12 * want.x_axis.x);
    }
}

/// A prism with octagonal ends: each end one face of eight corners (its fan of triangles
/// merged), its sides quadrilaterals.
#[test]
fn faces_in_one_plane_are_one_face() {
    let mut points = Vec::new();
    for k in 0..8 {
        let angle = std::f64::consts::TAU * f64::from(k) / 8.0;
        for z in [-0.4, 0.4] {
            points.push(DVec3::new(det::cos(angle) * 0.3, det::sin(angle) * 0.3, z));
        }
    }
    let (hull, centroid) = Hull::new(&points).expect("a prism");
    check(&hull, &points, centroid);
    let mut sizes: Vec<usize> = hull.faces().iter().map(|f| f.ring.len()).collect();
    sizes.sort_unstable();
    assert_eq!(sizes, [4, 4, 4, 4, 4, 4, 4, 4, 8, 8]);
    let area = 2.0 * 0.3 * 0.3 * 2f64.sqrt();
    assert!((hull.volume() / (area * 0.8) - 1.0).abs() < 1e-12);
}

#[test]
fn flat_or_broken_points_make_no_hull() {
    let flat: Vec<DVec3> = cloud(50, 1.0, 3)
        .into_iter()
        .map(|p| DVec3::new(p.x, 0.0, p.z))
        .collect();
    assert_eq!(Hull::new(&flat).err(), Some(HullError::Flat));
    let line = [DVec3::ZERO, DVec3::X, DVec3::X * 2.0, DVec3::X * 3.0];
    assert_eq!(Hull::new(&line).err(), Some(HullError::Flat));
    let mut broken = cloud(10, 1.0, 4);
    broken.push(DVec3::new(f64::NAN, 0.0, 0.0));
    assert_eq!(Hull::new(&broken).err(), Some(HullError::NotFinite));
}

/// Two twelve-sided rings of a pole member, as G2.8's builders assembled them: its faces met
/// along every edge (a corner nearly on a straight edge, or a face grouped through it, left an
/// edge with one face and the build panicked).
#[test]
fn a_member_s_prism_from_the_builders_scene_is_a_closed_hull() {
    let points = [
        DVec3::new(
            0.012883017770946026,
            0.014560032985173166,
            -0.015717292786575854,
        ),
        DVec3::new(
            0.0143536560353823,
            0.020055276807397604,
            -0.004093706229468808,
        ),
        DVec3::new(
            0.011978243826888502,
            0.020176725345663726,
            0.008626785565866157,
        ),
        DVec3::new(
            0.0063932707998901606,
            0.014891836617607623,
            0.019035737175727263,
        ),
        DVec3::new(
            -0.0009047739440575242,
            0.0056166923604905605,
            0.02434407835244201,
        ),
        DVec3::new(
            -0.00796038523549214,
            -0.00516344013158232,
            0.02312944340519607,
        ),
        DVec3::new(
            -0.012883017770946026,
            -0.014560032985173166,
            0.015717292786575854,
        ),
        DVec3::new(
            -0.0143536560353823,
            -0.020055276807397604,
            0.004093706229468808,
        ),
        DVec3::new(
            -0.011978243826888502,
            -0.020176725345663726,
            -0.008626785565866157,
        ),
        DVec3::new(
            -0.0063932707998901606,
            -0.014891836617607623,
            -0.019035737175727263,
        ),
        DVec3::new(
            0.0009047739440575242,
            -0.0056166923604905605,
            -0.02434407835244201,
        ),
        DVec3::new(
            0.00796038523549214,
            0.00516344013158232,
            -0.02312944340519607,
        ),
        DVec3::new(
            0.3400640214094892,
            -0.20670246245572343,
            0.047492666577454656,
        ),
        DVec3::new(0.3415346596739255, -0.201207218633499, 0.0591162531345617),
        DVec3::new(
            0.3391592474654317,
            -0.20108577009523287,
            0.07183674492989667,
        ),
        DVec3::new(
            0.33357427443843335,
            -0.20637065882328898,
            0.08224569653975777,
        ),
        DVec3::new(
            0.32627622969448566,
            -0.21564580308040604,
            0.08755403771647252,
        ),
        DVec3::new(
            0.31922061840305105,
            -0.22642593557247892,
            0.08633940276922658,
        ),
        DVec3::new(
            0.31429798586759716,
            -0.23582252842606977,
            0.07892725215060636,
        ),
        DVec3::new(0.3128273476031609, -0.2413177722482942, 0.06730366559349932),
        DVec3::new(
            0.3152027598116547,
            -0.24143922078656033,
            0.05458317379816435,
        ),
        DVec3::new(
            0.32078773283865303,
            -0.23615433205850422,
            0.04417422218830325,
        ),
        DVec3::new(0.3280857775826007, -0.22687918780138716, 0.0388658810115885),
        DVec3::new(
            0.33514138887403533,
            -0.21609905530931428,
            0.04008051595883444,
        ),
    ];
    let (hull, _) = Hull::new(&points).expect("a hull");
    assert!(hull.volume() > 0.0);
    // Two end rings and twelve sides.
    assert_eq!(hull.faces().len(), 14);
    assert_eq!(hull.vertices().len(), 24);
    for e in hull.edges() {
        assert_ne!(e.faces[0], e.faces[1]);
    }
}
