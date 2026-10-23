//! What the crosshair is on, and whether the engineer can reach it (G1.4d, ADR-0027 decision
//! 4). The camera's ray says what is meant; a second ray, from the engineer's eye to it, finds
//! anything in the way, which is then what is reached for (the camera behind him sees over what
//! he cannot: third-person games trace from the camera and then from the character). In reach
//! within 2.5 m of his hands (`creeer_sim::reach`), as the server will judge it.

use crate::planet_view::PlanetView;
use creeer_net::entity::{Kind, Shape};
use creeer_render::Camera;
use creeer_server::client::Aimed;
use creeer_sim::reach;

/// How much farther than reach the crosshair names what it is on (m): a little, to show what
/// is just out of reach.
const NAMED: f64 = 2.0;
/// How near the eye's ray must come to what the camera's met to be the same (m).
const SAME: f64 = 0.05;

/// What the crosshair is on.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub aimed: Aimed,
    /// What it is, in a word or a name.
    pub what: String,
    /// How far from the hands (m), and whether within reach.
    pub from_hands: f64,
    pub in_reach: bool,
    /// The loose thing it is on, if any (G1.6c: a stone or a branch, by its id).
    pub loose: Option<u64>,
    /// The standing tree it is on, if any (G1.7: its id, and the height up its stem).
    pub tree: Option<(u64, f64)>,
    /// Its mass if it is a thing one may lift (kg, G1.9).
    pub mass: Option<f64>,
}

/// What the crosshair of `camera` is on, while the engineer walks: a thing within reach, or just
/// out of it.
pub fn target(view: &PlanetView, camera: &Camera) -> Option<Target> {
    let connection = &view.session.connection;
    let avatar = connection.avatar().filter(|_| view.walking)?;
    let centre = avatar.position;
    let up = centre.normalize();
    let hands = reach::hands(centre, up);
    let length = (camera.position - hands).length() + reach::REACH + NAMED;
    let meant = connection.aim(camera.position, camera.forward(), length)?;
    // A stone or branch on the ground, if the ray meets one first (G1.6c).
    let rocks = &view.terrain.planet().geology.rocks;
    let things = (view.loose.as_ref())
        .map(|l| l.things(&connection.taken, &connection.laid, rocks))
        .unwrap_or_default();
    let loose = crate::loose_view::aim(&things, camera.position, camera.forward(), length)
        .filter(|(_, t)| *t < meant.distance);
    if let Some((l, t)) = loose {
        let point = camera.position + camera.forward() * t;
        let from_hands = (point - hands).length();
        return (from_hands <= reach::REACH + NAMED).then(|| Target {
            aimed: Aimed {
                body: None,
                point,
                distance: t,
                seen: meant.seen,
            },
            what: l.name(),
            from_hands,
            in_reach: from_hands <= reach::REACH,
            loose: Some(l.id),
            tree: None,
            mass: Some(l.mass),
        });
    }
    // A standing tree's stem (G1.7), if the ray meets one first.
    let tree = (view.trees.as_ref())
        .and_then(|v| {
            v.aim(connection, camera.position, camera.forward(), length)
                .map(|a| (v, a))
        })
        .filter(|(_, (_, _, t))| *t < meant.distance);
    if let Some((v, (t, z, s))) = tree {
        let point = camera.position + camera.forward() * s;
        let from_hands = (point - hands).length();
        return (from_hands <= reach::REACH + NAMED).then(|| Target {
            aimed: Aimed {
                body: None,
                point,
                distance: s,
                seen: meant.seen,
            },
            what: v.name(connection, &t),
            from_hands,
            in_reach: from_hands <= reach::REACH,
            loose: None,
            tree: Some((t.id, z)),
            mass: None,
        });
    }
    // From his eye: what is in the way, if anything.
    let eye = reach::eye(centre, up);
    let to = meant.point - eye;
    let aimed = match connection.aim(eye, to.normalize(), to.length() + SAME) {
        Some(first) if first.distance < to.length() - SAME => first,
        _ => meant,
    };
    let from_hands = (aimed.point - hands).length();
    (from_hands <= reach::REACH + NAMED).then(|| Target {
        aimed,
        what: name(aimed, connection),
        from_hands,
        in_reach: from_hands <= reach::REACH,
        loose: None,
        tree: None,
        mass: (aimed.body)
            .and_then(|id| connection.items.get(&id))
            .map(|i| i.2),
    })
}

/// A word for what was aimed at: an item by what the server says it is (G1.6), else by its kind.
fn name(aimed: Aimed, connection: &creeer_server::Connection) -> String {
    let Some(id) = aimed.body else {
        return "ground".into();
    };
    if let Some((word, ..)) = connection.items.get(&id) {
        return word.clone();
    }
    match connection.kinds.get(&id) {
        Some(Kind::Character { player, .. }) => player.clone(),
        Some(Kind::Segment { .. }) => "someone fallen".into(),
        Some(Kind::Body(Shape::Box(_), _)) => "box".into(),
        Some(Kind::Body(Shape::Sphere(_), _)) => "ball".into(),
        Some(Kind::Body(Shape::Hull(_), _)) => "rock".into(),
        Some(Kind::Structure(_)) => "frame".into(),
        Some(Kind::Tree(t)) => format!("log, {:.0} m", t.to - t.from),
        None => "something".into(),
    }
}

/// The crosshair's label: what it is on, and how far from the hands, or that it is out of reach.
pub fn label(target: &Target) -> String {
    if target.in_reach {
        format!("{} · {:.1} m", target.what, target.from_hands)
    } else {
        format!("{} · out of reach", target.what)
    }
}

impl PlanetView {
    /// Whether the player's character carries something in both hands (G1.4f), as the server
    /// last told.
    pub fn carrying(&self) -> bool {
        let c = &self.session.connection;
        let avatar = c.welcome.as_ref().map(|w| w.avatar);
        matches!(
            avatar.and_then(|a| c.kinds.get(&a)),
            Some(Kind::Character {
                carries: Some(_),
                ..
            })
        )
    }

    /// Takes what the crosshair is on in both hands, if within reach (the tool in hand put
    /// away), or puts down what they hold.
    pub fn grab_or_drop(&mut self, camera: &Camera) {
        use creeer_net::message::{Command, Target};
        if self.carrying() {
            self.send(Command::Drop);
            return;
        }
        let Some(target) = target(self, camera).filter(|t| t.in_reach) else {
            return;
        };
        if let Some(loose) = target.loose {
            self.wielding = None;
            self.send(Command::Pick {
                loose,
                pocket: false,
            });
            return;
        }
        if target.aimed.body.is_none() {
            return;
        }
        self.wielding = None;
        let a = target.aimed;
        self.send(Command::Grab(Target {
            body: a.body,
            at: a.point,
            seen: a.seen,
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use creeer_math::glam::DVec3;

    #[test]
    fn the_label_names_it_and_its_distance_or_that_it_is_out_of_reach() {
        let aimed = Aimed {
            body: None,
            point: DVec3::ZERO,
            distance: 3.0,
            seen: 10.0,
        };
        let mut t = Target {
            aimed,
            what: "box".into(),
            from_hands: 1.94,
            in_reach: true,
            loose: None,
            tree: None,
            mass: None,
        };
        assert_eq!(label(&t), "box · 1.9 m");
        t.in_reach = false;
        assert_eq!(label(&t), "box · out of reach");
    }
}
