//! The world does what players ask within the limits, keeps entities' ids and players'
//! avatars, and saves and loads as it was.

use super::*;
use creeer_sim::save::Save;
use creeer_terrain::edits::Brush;

/// The reference planet's terrain, loaded anew (each side of a game has its own, as on two
/// machines).
pub(crate) fn terrain() -> Arc<Terrain> {
    load_terrain(&creeer_data::Data::base(), "planets/home.ron").expect("the reference planet")
}

/// A new world on the reference planet: a test server, creative (G1.4d).
pub(crate) fn world() -> ServerWorld {
    let mut world = ServerWorld::new(creeer_data::Data::base(), terrain(), "planets/home.ron");
    world.creative = true;
    world
}

pub(crate) fn above(world: &ServerWorld, east: f64, height: f64) -> DVec3 {
    let up = world.spawn().normalize();
    let (e, _) = creeer_terrain::geology::deposits::tangent_frame(up);
    let at = world.ground_under(world.spawn() + e * east);
    at + at.normalize() * height
}

fn dig(center: DVec3, radius: f64) -> Command {
    let mut edits = Edits::default();
    edits.push(Edit::Dig(Brush::Sphere { center, radius }));
    Command::Edit(edits.to_bytes())
}

#[test]
fn avatars_appear_on_the_ground_and_are_their_players_own() {
    let mut w = world();
    let spawn = w.spawn();
    assert!(
        w.terrain.density(spawn).abs() < 1e-3,
        "the spawn point is not on the ground"
    );
    let ada = w.avatar_of("ada");
    let bob = w.avatar_of("bob");
    assert_ne!(ada, bob);
    assert_eq!(w.avatar_of("ada"), ada);
    // It stands on its legs after a second (steered as the server steers every character),
    // side by side with the other.
    for _ in 0..60 {
        for id in [ada, bob] {
            w.steer(id, &AvatarInput::default());
        }
        w.step();
    }
    let at = |w: &ServerWorld, id| w.world.bodies[w.index(id).unwrap()].structure.position[0];
    let height = w.terrain.density(at(&w, ada));
    let standing = creeer_sim::character::heights().0;
    assert!(
        (height - standing).abs() < 0.05,
        "its centre {height} m above the ground, standing {standing}"
    );
    assert!((at(&w, ada) - at(&w, bob)).length() > 1.0);
}

#[test]
fn commands_act_within_reach_and_the_limits_only() {
    let mut w = world();
    let ada = w.avatar_of("ada");
    w.apply(&Command::Crate(above(&w, 3.0, 0.6)), Some(ada))
        .unwrap();
    w.apply(&Command::Ball(above(&w, -3.0, 0.5)), Some(ada))
        .unwrap();
    w.apply(&Command::Frame(above(&w, 6.0, 0.05)), Some(ada))
        .unwrap();
    assert_eq!(w.world.bodies.len(), 4);
    let far = above(&w, REACH + 50.0, 1.0);
    assert!(
        w.apply(&Command::Crate(far), Some(ada))
            .unwrap_err()
            .contains("reach")
    );
    assert!(w.apply(&Command::Ball(DVec3::NAN), Some(ada)).is_err());
    assert!(
        w.apply(&dig(above(&w, 3.0, 0.0), MOST_BRUSH * 2.0), Some(ada))
            .unwrap_err()
            .contains("brush")
    );
    assert!(w.apply(&Command::Edit(vec![1, 2, 3]), Some(ada)).is_err());
    let mut two = Edits::default();
    two.push(Edit::Dig(Brush::Sphere {
        center: above(&w, 1.0, 0.0),
        radius: 1.0,
    }));
    two.push(Edit::Dig(Brush::Sphere {
        center: above(&w, 2.0, 0.0),
        radius: 1.0,
    }));
    assert!(w.apply(&Command::Edit(two.to_bytes()), Some(ada)).is_err());
    assert_eq!(w.terrain.edits().len(), 0);
    w.apply(&dig(above(&w, 10.0, 0.0), 2.0), Some(ada)).unwrap();
    assert_eq!(w.terrain.edits().len(), 1);
    assert_eq!(w.world.bodies.len(), 4);
}

#[test]
fn clearing_keeps_the_avatars_and_every_id() {
    let mut w = world();
    let ada = w.avatar_of("ada");
    for k in 0..4 {
        w.apply(
            &Command::Crate(above(&w, 2.0 + 2.0 * f64::from(k), 0.6)),
            None,
        )
        .unwrap();
    }
    let bob = w.avatar_of("bob");
    for _ in 0..20 {
        w.step();
    }
    w.apply(&Command::Clear, Some(ada)).unwrap();
    assert_eq!(w.ids(), &[ada, bob]);
    assert_eq!((w.player(0), w.player(1)), (Some("ada"), Some("bob")));
    // Ids are never handed out twice.
    w.apply(&Command::Crate(above(&w, 3.0, 0.6)), None).unwrap();
    assert!(w.ids()[2].0 > bob.0);
    for _ in 0..20 {
        w.step();
    }
}

#[test]
fn a_frame_stands_where_it_is_put_unstrained() {
    let mut w = world();
    w.apply(&Command::Frame(above(&w, 4.0, 0.05)), None)
        .unwrap();
    for _ in 0..120 {
        w.step();
    }
    let s = &w.world.bodies[0].structure;
    assert!(
        s.max_axial_strain() < 1e-4,
        "strained {}",
        s.max_axial_strain()
    );
    // Upright: its corner column (nodes 0, 3, 6) 2 m from its base to its top, standing within
    // 5° of the plumb. (The ground here slopes 2–3 %: the frame settles flat on it, its top
    // corners up to 6 cm above its base corner's plumb; measured along the plumb, as this did
    // before G2.3, it read the slope as height.)
    let (base, top) = (s.position[0], s.position[6]);
    let column = top - base;
    assert!(
        (column.length() - 2.0).abs() < 0.01,
        "{} m tall",
        column.length()
    );
    let lean = creeer_math::det::acos(column.normalize().dot(base.normalize())).to_degrees();
    assert!(lean < 5.0, "leaning {lean}°");
}

#[test]
fn a_world_saved_and_loaded_is_as_it_was() {
    let mut w = world();
    let ada = w.avatar_of("ada");
    w.apply(&Command::Crate(above(&w, 3.0, 0.6)), None).unwrap();
    w.apply(&dig(above(&w, 10.0, 0.0), 2.0), None).unwrap();
    for _ in 0..30 {
        w.step();
    }
    let bytes = w.to_save().to_bytes();
    let loaded = ServerWorld::from_save(
        creeer_data::Data::base(),
        Save::from_bytes(&bytes).unwrap(),
        terrain(),
    );
    assert_eq!(loaded.ids(), w.ids());
    assert_eq!(loaded.tick, 30);
    assert_eq!(loaded.terrain.edits().len(), 1);
    assert_eq!(loaded.world.to_bytes(), w.world.to_bytes());
    let mut loaded = loaded;
    assert_eq!(loaded.avatar_of("ada"), ada);
    loaded
        .apply(&Command::Crate(above(&loaded, 5.0, 0.6)), None)
        .unwrap();
    assert!(
        loaded
            .ids()
            .iter()
            .all(|id| loaded.ids().iter().filter(|o| *o == id).count() == 1)
    );
}
