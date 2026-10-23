//! Cutting into a stem (ADR-0031): what is left of a section against the circular segment's
//! closed forms, the notch's volume, the depth a stem chopped from one side goes over at and
//! which way, a steel axe's felling time as measured, and a stone axe's against it.

use super::chop::{self, Cut};
use super::section::{Notch, Section};
use super::tests::{data, forest};
use super::*;
use crate::items::{Edge, Form, Piece};
use crate::tools::Tool;
use crate::{making, strike};
use std::f64::consts::PI;

/// Circular segment of a disc of radius `r` past the chord at `r − d`: its area, centroid (x)
/// and ∫x² dA.
fn segment(r: f64, d: f64) -> (f64, f64, f64) {
    let a = r - d;
    let area = r * r * creeer_math::det::acos(a / r) - a * (r * r - a * a).sqrt();
    let half = creeer_math::det::acos(a / r);
    let centroid = 4.0
        * r
        * creeer_math::det::sin(half)
        * creeer_math::det::sin(half)
        * creeer_math::det::sin(half)
        / (3.0 * (2.0 * half - creeer_math::det::sin(2.0 * half)));
    // 2·∫ₐʳ x²·√(r² − x²) dx.
    let f = |x: f64| {
        x / 8.0 * (2.0 * x * x - r * r) * (r * r - x * x).max(0.0).sqrt()
            + (r * r * r * r) / 8.0 * creeer_math::det::asin((x / r).clamp(-1.0, 1.0))
    };
    (area, centroid, 2.0 * (f(r) - f(a)))
}

#[test]
fn what_a_notch_leaves_is_the_disc_less_its_segment() {
    let (r, d) = (0.15, 0.2);
    let s = Section::cut(
        r,
        &[Notch {
            azimuth: 0.0,
            depth: d,
        }],
    )
    .unwrap();
    let (seg, x, second) = segment(r, d);
    let area = PI * r * r - seg;
    let centre = -seg * x / area;
    assert!((s.area - area).abs() < 2e-3 * area, "{} {area}", s.area);
    assert!((s.centre.x - centre).abs() < 2e-3 * r && s.centre.y.abs() < 1e-12);
    let j = PI * (r * r * r * r) / 4.0 - second - area * centre * centre;
    assert!((s.second[0] - j).abs() < 4e-3 * j, "{} {j}", s.second[0]);
    // A weight N through the stem's axis: the closed form of EN 1995's rule.
    let (n, fm, fc) = (5000.0, 41e6, 18.8e6);
    let c = (r - d) - centre;
    let closed = (n / area / fc) * (n / area / fc) + n * (-centre) * c / j / fm;
    let used = s.utilisation(n, creeer_math::glam::DVec2::ZERO, fm, fc);
    assert!((used - closed).abs() < 5e-3 * closed, "{used} {closed}");
}

#[test]
fn a_notch_through_holds_pi_r_cubed_and_its_depth_is_found_again() {
    let r = 0.2;
    let through = chop::notch_volume(r, 2.0 * r);
    assert!(
        (through - PI * r * r * r).abs() < 1e-6 * through,
        "{through}"
    );
    for d in [0.01, 0.1, 0.2, 0.33] {
        let v = chop::notch_volume(r, d);
        assert!((chop::depth_of(r, v) - d).abs() < 1e-9);
    }
}

/// A steel axe's blow on a tree (J: the energy its striking point's effective mass carries into
/// what does not move) and its edge.
pub(super) fn steel_axe() -> (f64, Option<Edge>) {
    let axe = (Tool::all(&data()).unwrap().into_iter())
        .find(|t| t.name == "axe")
        .unwrap();
    let s = strike::swing(&axe);
    let (angle, length) = axe.edge.unwrap();
    let edge = Edge {
        angle: angle.to_radians(),
        length,
        radius: 1e-5,
    };
    (0.5 * s.mass * s.speed * s.speed, Some(edge))
}

/// A stone axe as a player makes one (G1.6d): a knapped chert core (its 70° edge) bound across a
/// 65 cm stick.
fn stone_axe() -> (f64, Option<Edge>) {
    let mut head = Piece::solid(
        "chert",
        2600.0,
        Form::Lump {
            size: creeer_math::glam::DVec3::new(0.14, 0.07, 0.045),
        },
    );
    head.edge = Some(Edge {
        angle: making::CORE_ANGLE,
        length: 0.06,
        radius: 5e-6,
    });
    let stick = |radius, length| Piece::solid("spruce_green", 800.0, Form::Rod { radius, length });
    let mut branch = stick(0.02, 0.9);
    let binding = making::strip(&mut branch, ("spruce_bast", 600.0)).unwrap();
    let made = making::bind(stick(0.016, 0.65), head, binding, 20e6).unwrap();
    let w = made.swinging();
    let s = strike::swing_of(w.mass, w.gyration, w.carry, w.strike);
    let edge = made.parts[w.head].piece.edge;
    (0.5 * s.mass * s.speed * s.speed, edge)
}

/// Blows that fell `tree` of `species` cut 0.5 m up, all from +x, with blows of `energy` and
/// `edge`; and the notch's depth then (m).
fn blows_to_fell(
    tree: &Tree,
    species: &Species,
    (energy, edge): (f64, Option<Edge>),
) -> (u32, f64) {
    let mut cut = Cut::at(0.5);
    let per_blow = chop::chopped(energy, edge, species.across);
    for blow in 1..100_000 {
        cut.chop(0.0, per_blow);
        match tree.at_cut(species, &cut, DVec3::ZERO) {
            Some((used, ..)) if used < 1.0 => {}
            _ => return (blow, cut.notches(tree.radius(0.5))[0].depth),
        }
    }
    unreachable!("never fell")
}

#[test]
fn chopped_from_one_side_it_goes_over_towards_the_notch() {
    let (forest, _) = forest();
    for species in &forest.species {
        let tree = Tree::grow(species, 3, 0.3);
        let (blows, depth) = blows_to_fell(&tree, species, steel_axe());
        let diameter = 2.0 * tree.radius(0.5);
        // Past the middle the weight bears beside what is left (axe-felling's front notch goes
        // a third to a half in, then the back notch: Weisgerber 1999).
        assert!(
            (0.45..0.95).contains(&(depth / diameter)),
            "{} {depth}",
            species.name
        );
        let mut cut = Cut::at(0.5);
        cut.chop(0.0, chop::notch_volume(tree.radius(0.5), depth));
        let (_, pivot, way) = tree.at_cut(species, &cut, DVec3::ZERO).unwrap();
        assert!(way.x > 0.99, "it goes over towards the notch: {way}");
        let left = Section::cut(tree.radius(0.5), &cut.notches(tree.radius(0.5))).unwrap();
        // It pivots on what is left's edge towards the notch, the weight beyond it.
        assert!(
            pivot.x > left.centre.x && tree.above(0.5).1.x > pivot.x,
            "{pivot}"
        );
        println!(
            "{} 30 cm: {blows} blows, the notch {:.0} % through",
            species.name,
            100.0 * depth / diameter
        );
    }
}

#[test]
fn a_steel_axe_fells_a_30_cm_spruce_in_about_five_minutes() {
    // The calibration (`chop::CHIP`): stone axes fell 30 cm in about 20 min, 4–4.5 times the
    // steel axe's time on large trees (New Guinea, Moore's synthesis); felling axemen swing
    // 25–35 strokes a minute (Glaser, Unasylva 1954): about 150 blows.
    let (forest, _) = forest();
    let spruce = &forest.species[0];
    let tree = Tree::grow(spruce, 3, 0.3);
    let axe = steel_axe();
    let (blows, _) = blows_to_fell(&tree, spruce, axe);
    #[allow(clippy::cast_precision_loss)]
    let minutes = f64::from(blows) * strike::CYCLE / 60.0;
    println!(
        "steel: {:.0} J a blow, {blows} blows, {minutes:.1} min",
        axe.0
    );
    assert!((4.0..6.0).contains(&minutes), "{minutes}");
}

#[test]
fn a_stone_axe_fells_as_much_slower_as_measured() {
    // Steel against stone: 2.5–4.4 times as fast in New Guinea (Townsend 1969, Salisbury 1962,
    // Godelier & Garanger 1973), over 6 in Saraydar & Shimada 1971.
    let (forest, _) = forest();
    let spruce = &forest.species[0];
    for d in [0.15, 0.3] {
        let tree = Tree::grow(spruce, 3, d);
        let (steel, _) = blows_to_fell(&tree, spruce, steel_axe());
        let stone = stone_axe();
        let (slow, _) = blows_to_fell(&tree, spruce, stone);
        let ratio = f64::from(slow) / f64::from(steel);
        println!(
            "{d} m: stone {slow} blows ({:.0} J), steel {steel}: {ratio:.1}×",
            stone.0
        );
        assert!((2.5..6.5).contains(&ratio), "{ratio}");
    }
}

#[test]
fn the_wind_decides_which_way_a_balanced_cut_goes() {
    let (forest, _) = forest();
    let spruce = &forest.species[0];
    let tree = Tree::grow(spruce, 3, 0.2);
    let r = tree.radius(0.5);
    // Notched from both sides alike: what is left is a band across the middle.
    let mut cut = Cut::at(0.5);
    for side in [0.0, PI] {
        cut.chop(side, chop::notch_volume(r, 0.8 * r));
    }
    let still = tree.at_cut(spruce, &cut, DVec3::ZERO).unwrap().0;
    let (windy, _, way) = tree
        .at_cut(spruce, &cut, DVec3::new(0.0, 15.0, 0.0))
        .unwrap();
    assert!(windy > still, "{windy} {still}");
    assert!(way.y > 0.9, "it goes with the wind: {way}");
}

#[test]
fn a_felled_part_is_its_stem_and_the_branches_still_on_it() {
    let (forest, _) = forest();
    let tree = Tree::grow(&forest.species[0], 9, 0.25);
    let whole = Part {
        species: 0,
        seed: 9,
        dbh: 0.25,
        from: 0.6,
        to: tree.height,
        cut: Vec::new(),
    };
    let (mass, centre, inertia) = tree.part_mass(&whole);
    let (above, at) = tree.above(0.6);
    assert!((mass - above).abs() < 1e-9 * mass && (centre - at).length() < 1e-9);
    // A long thin body: least inertia about its stem, the other two alike.
    let (ixx, iyy, izz) = (inertia.x_axis.x, inertia.y_axis.y, inertia.z_axis.z);
    assert!(
        izz < 0.05 * ixx && (ixx - iyy).abs() < 0.05 * ixx,
        "{inertia}"
    );
    // Cut off a node's branches: lighter by them, thinnest first.
    let mut limbed = whole.clone();
    limbed.cut = vec![0; tree.nodes.len()];
    limbed.cut[2] = 2;
    let order = Tree::cut_order(&tree.nodes[2]);
    let gone: f64 = order[..2]
        .iter()
        .map(|&b| tree.nodes[2].branches[b].mass())
        .sum();
    assert!((tree.part_mass(&limbed).0 - (mass - gone)).abs() < 1e-9 * mass);
    assert_eq!(tree.kept(&limbed).len(), tree.kept(&whole).len() - 2);
    // The hull holds the stem: rings at least every 3 m.
    let hull = tree.part_hull(&whole);
    assert!(hull.len() >= 12 * 8, "{}", hull.len());
}
