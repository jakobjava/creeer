//! What the player has and sees, read from the game for the prompts (`prompts.rs`), and a name
//! for what the crosshair is on as a person would call it: a pole lying, a post standing and
//! its top, a log, the ground and what it is (G2.11 A3).

use crate::hud_feedback::fastener_words;
use crate::planet_aim::Target;
use crate::planet_view::PlanetView;
use crate::prompts::{Aimed, Seen, What};
use creeer_math::glam::DVec3;
use creeer_net::entity::{Kind, Shape};
use creeer_sim::carry::MOST_LIFTED;

/// A rod is at least this many times as long as it is thick.
const SLENDER: f64 = 4.0;
/// A part to build with is at least this long (m); shorter, it is a stick.
const PART: f64 = 0.8;
/// A post stands within this of upright (the cosine of 30°).
const UPRIGHT: f64 = 0.866;
/// Within this of a rod's end (m) the crosshair is on its end (a post's top).
const END: f64 = 0.15;
/// The largest a pocket takes (m³): the coverall's thigh pockets, 11.5 × 18 × 6 cm.
const POCKET: f64 = 0.115 * 0.18 * 0.06;

/// A thing's shape as the eye takes it: its length, thickness and long axis in its own frame.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Look {
    length: f64,
    thickness: f64,
    axis: DVec3,
    volume: f64,
}

/// `shape`'s look (a ball: its diameter both ways).
fn look(shape: &Shape) -> Look {
    let half = match shape {
        Shape::Box(half) => *half,
        Shape::Sphere(r) => DVec3::splat(*r),
        Shape::Hull(points) => {
            let (lo, hi) = (points.iter()).fold((DVec3::MAX, DVec3::MIN), |(lo, hi), p| {
                (lo.min(*p), hi.max(*p))
            });
            (hi - lo) * 0.5
        }
    };
    let k = if half.x >= half.y && half.x >= half.z {
        0
    } else if half.y >= half.z {
        1
    } else {
        2
    };
    let mut rest: Vec<f64> = (0..3).filter(|i| *i != k).map(|i| half[i]).collect();
    rest.sort_by(f64::total_cmp);
    let mut axis = DVec3::ZERO;
    axis[k] = 1.0;
    Look {
        length: 2.0 * half[k],
        thickness: 2.0 * rest[1],
        axis,
        volume: 8.0 * half.x * half.y * half.z,
    }
}

/// The edges of the box about `shape` (m), longest first.
fn extents(shape: &Shape) -> [f64; 3] {
    let l = look(shape);
    let mut e = [l.length, l.thickness, 0.0];
    e[2] = if l.length * l.thickness > 0.0 {
        l.volume / (l.length * l.thickness)
    } else {
        0.0
    };
    e.sort_by(|a, b| b.total_cmp(a));
    e
}

/// The name for a thing of `look`, by its word (`kit`: the kit's own name for it, kept as it
/// is), standing or lying, and where on it the crosshair is (`along`: from its middle along its
/// length, upwards for one standing).
fn named(word: &str, l: Look, (standing, kit): (bool, bool), along: f64) -> String {
    let rod = l.length >= SLENDER * l.thickness;
    if !(rod && l.length >= PART) {
        // The sandbox's metre-wide boxes are drawn as crates.
        return if word == "box" && l.thickness >= 0.6 {
            "crate".to_owned()
        } else {
            word.to_owned()
        };
    }
    let name = match (standing, word) {
        _ if kit => word,
        (true, _) => "post",
        (false, "block") => "beam",
        (false, "log") => "log",
        _ => "pole",
    };
    let near_end = along.abs() > 0.5 * l.length - END;
    match (standing, near_end) {
        (true, true) if along > 0.0 => format!("top of the {name}"),
        (true, true) => format!("foot of the {name}"),
        (false, true) => format!("end of the {name}"),
        _ => name.to_owned(),
    }
}

/// What the player has and sees, and the crosshair's name for what it is on.
pub(crate) fn seen(view: &PlanetView, target: Option<&Target>) -> (Seen, Option<String>) {
    let c = &view.session.connection;
    let planet = view.terrain.planet();
    let g = planet.mu / (planet.radius * planet.radius);
    let lifts = MOST_LIFTED * 9.806_65 / g;
    let held = c.own_load();
    let kind_look = |id| match c.kinds.get(&id) {
        Some(Kind::Body(shape, _)) => Some(look(shape)),
        _ => None,
    };
    let hands_part = held
        .and_then(kind_look)
        .is_some_and(|l| l.length >= PART && l.length >= SLENDER * l.thickness);
    let mut seen = Seen {
        walking: view.walking,
        hands: c.inventory.hands.as_ref().map(|h| h.word.clone()),
        holding: view.holding.holding(),
        pinned: view.holding.pinned(),
        hands_part,
        hands_pocketable: held.and_then(kind_look).is_none_or(|l| l.volume <= POCKET),
        tool: view.wielding.is_some(),
        // In words ("a 75 mm nail", not "nail 75"); empty, the hand's own choice.
        fastener: view.fastening.name().map(fastener_words),
        fly_tool: (!view.walking).then(|| tool_name(view)),
        lifts: (lifts, 2.0 * lifts),
        aimed: None,
    };
    let Some(t) = target else {
        return (seen, None);
    };
    let what = if t.tree.is_some() {
        What::Tree
    } else if t.loose.is_some() || t.aimed.body.is_some() {
        What::Thing
    } else {
        What::Ground
    };
    let mut aimed = Aimed {
        what,
        in_reach: t.in_reach,
        mass: t.mass,
        in_hands: t.aimed.body.is_some() && t.aimed.body == held,
        stone: ["stone", "flake", "worked stone", "rock"]
            .iter()
            .any(|w| t.what.starts_with(w)),
        ..Aimed::default()
    };
    let mut name = t.what.clone();
    if what == What::Ground {
        name = ground_name(view, t.aimed.point);
    }
    if let Some(id) = t.aimed.body {
        let kind = c.kinds.get(&id);
        if let Some(Kind::Body(shape, matter)) = kind {
            let l = look(shape);
            aimed.mass = aimed.mass.or(Some(matter.mass));
            aimed.pocketable = l.volume <= POCKET;
            // A part of the creative kit is known by its name (and a container by the kit's
            // word for it); another squat block may be one (the client is not told).
            let kit = crate::kit_ui::by_shape(extents(shape), matter.mass);
            aimed.container = kit
                .map_or(t.what == "block" && l.length < SLENDER * l.thickness, |k| {
                    k.container
                });
            if let Some(pose) = c.poses().get(&id) {
                let axis = pose.orientation * l.axis;
                let up = pose.position.normalize();
                let standing = axis.dot(up).abs() > UPRIGHT;
                // Upwards along it, for one standing.
                let axis = if axis.dot(up) < 0.0 { -axis } else { axis };
                let along = (t.aimed.point - pose.position).dot(axis);
                let word = kit.map_or(t.what.as_str(), |k| k.name.as_str());
                name = named(word, l, (standing, kit.is_some()), along);
                aimed.part = l.length >= PART && l.length >= SLENDER * l.thickness;
                aimed.standing = standing;
            }
        }
        if let Some(Kind::Tree(_)) = kind {
            aimed.part = true;
        }
        // Whether it is fastened to anything the client is not told: U is offered on any part.
        aimed.built = aimed.part;
    }
    if aimed.in_hands {
        name = format!("{name} in your hands");
    }
    seen.aimed = Some(aimed);
    (seen, Some(name))
}

/// What the hands hold, named as the crosshair names it (a pole, not a "stick" 1.6 m long).
pub(crate) fn held_name(view: &PlanetView) -> Option<String> {
    let c = &view.session.connection;
    let word = &c.inventory.hands.as_ref()?.word;
    match c.own_load().and_then(|id| c.kinds.get(&id)) {
        Some(Kind::Body(shape, matter)) => {
            let kit = crate::kit_ui::by_shape(extents(shape), matter.mass);
            let word = kit.map_or(word.as_str(), |k| k.name.as_str());
            Some(named(word, look(shape), (false, kit.is_some()), 0.0))
        }
        _ => Some(word.clone()),
    }
}

/// The ground at `point`, with what it is made of where it would be dug.
fn ground_name(view: &PlanetView, point: DVec3) -> String {
    let below = point - point.normalize() * 0.5;
    let rocks = &view.terrain.planet().geology.rocks.rocks;
    match view.terrain.rock_at(below) {
        Some(r) => format!("ground ({})", rocks[usize::from(r)].name),
        None => "ground".to_owned(),
    }
}

/// The sandbox tool a click uses while flying.
fn tool_name(view: &PlanetView) -> String {
    (crate::planet_view::Tool::ALL.iter())
        .find(|(t, _)| *t == view.tool)
        .map_or("dig", |(_, n)| n)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_rod_is_a_pole_lying_and_a_post_standing_with_its_top_and_ends() {
        let rod = look(&Shape::Box(DVec3::new(0.05, 0.8, 0.05)));
        assert!((rod.length - 1.6).abs() < 1e-12 && (rod.thickness - 0.1).abs() < 1e-12);
        assert_eq!(rod.axis, DVec3::Y);
        let (lying, standing) = ((false, false), (true, false));
        assert_eq!(named("stick", rod, lying, 0.0), "pole");
        assert_eq!(named("stick", rod, lying, -0.75), "end of the pole");
        assert_eq!(named("stick", rod, standing, 0.2), "post");
        assert_eq!(named("stick", rod, standing, 0.75), "top of the post");
        assert_eq!(named("stick", rod, standing, -0.75), "foot of the post");
        assert_eq!(named("block", rod, lying, 0.0), "beam");
    }

    /// The bridge site's parts are named as the kit names them (G2.11 Q: a deck board, a
    /// stringer and a sleeper were all "beam"), a granite block is not a container, a crate is.
    #[test]
    fn the_kits_parts_are_named_as_the_kit_names_them() {
        let named_as = |half: DVec3, mass: f64, along: f64| {
            let shape = Shape::Box(half);
            let kit = crate::kit_ui::by_shape(extents(&shape), mass).expect("a kit part");
            let name = named(&kit.name, look(&shape), (false, true), along);
            (name, kit.container)
        };
        let board = |kg| named_as(DVec3::new(0.5625, 0.025, 0.075), kg, 0.0);
        let stringer = named_as(DVec3::new(1.5, 0.075, 0.0375), 14.2, 1.45);
        assert_eq!(board(3.5), ("deck board".to_owned(), false));
        assert_eq!(stringer, ("end of the stringer".to_owned(), false));
        let block = named_as(DVec3::new(0.16, 0.075, 0.1), 25.4, 0.0);
        assert_eq!(block, ("granite block".to_owned(), false));
        let load_crate = named_as(DVec3::new(0.4, 0.25, 0.25), 18.0, 0.0);
        assert_eq!(load_crate, ("load crate".to_owned(), true));
        assert!(
            crate::kit_ui::by_shape([1.125, 0.15, 0.05], 9.0).is_none(),
            "not its weight"
        );
    }

    #[test]
    fn short_or_squat_things_keep_their_word() {
        let lying = (false, false);
        let stick = look(&Shape::Box(DVec3::new(0.012, 0.012, 0.225)));
        assert_eq!(stick.axis, DVec3::Z);
        assert_eq!(named("stick", stick, lying, 0.0), "stick");
        let crate_ = look(&Shape::Box(DVec3::new(0.25, 0.2, 0.2)));
        assert_eq!(named("block", crate_, lying, 0.24), "block");
        let sandbox_crate = look(&Shape::Box(DVec3::splat(0.5)));
        assert_eq!(named("box", sandbox_crate, lying, 0.0), "crate");
        let ball = look(&Shape::Sphere(0.04));
        assert!((ball.volume - 0.08 * 0.08 * 0.08).abs() < 1e-12);
    }
}
