//! A player standing on a loose pole (G2.10, ADR-0023: standing on a light body): *builders*'
//! eight players by a green spruce pole (1.6 m, 50 mm, 2.5 kg) laid where the third of them
//! spawns, 4 m east of the spawn, the players standing by.

use super::Play;
use super::record::Act;
use super::scenes::{scene, terrain, world};
use creeer_math::glam::DVec3;
use creeer_net::entity::EntityId;
use creeer_sim::items::{Form, Item, Piece};
use creeer_terrain::geology::deposits::tangent_frame;

/// Its work in a tick at most, and the pole's spin about its own axis at most, the person's
/// landing on it in the first tick (it rolls a little under the soles, carrying the person):
/// each a tenth above what was measured (2026-10-28: 110 Newton iterations at tick 0; 6.5 rad/s
/// at tick 0, under 0.001 rad/s after the first second). Before G2.10 the pole spun at up to
/// 125 rad/s within each tick's first step, the step was redone in 16 substeps every tick, and
/// a tick took 7–18 ms and up to 405 iterations.
const MOST_WORK: u64 = 121;
const MOST_SPIN: f64 = 7.1;
/// The tick it sleeps by, the same (measured: 106; 105 before, after rattling under the
/// person's feet for 77 ticks).
const ASLEEP_BY: u32 = 117;

#[test]
fn a_pole_stood_on_carries_the_person_and_comes_to_rest() {
    let s = scene("builders").expect("the scene");
    let terrain = terrain();
    let mut w = world(terrain.clone());
    let spawn = w.spawn();
    let (east, _) = tangent_frame(spawn.normalize());
    let ground = w.ground_under(spawn + east * 4.0);
    let rod = Form::Rod {
        radius: 0.025,
        length: 1.6,
    };
    let pole = Item::Piece(Piece::solid("spruce_green", 800.0, rod), Vec::new());
    let id = w.add_item(pole, ground + ground.normalize() * 0.03, true);
    assert_eq!(id, EntityId(1));
    let mut play = Play::new(w, s.name, s.players, &terrain);
    let idle = vec![Act::default(); s.players.len()];
    let (mut work, mut spin, mut asleep) = ((0, 0), (0.0f64, 0), None);
    for tick in 0..240u32 {
        let t = play.tick(&idle);
        if t.work > work.0 {
            work = (t.work, tick);
        }
        let w = &play.server.world;
        let b = &w.world.bodies[w.index(id).expect("the pole")];
        let axis = b.structure.orientation[0] * DVec3::X;
        let about = b.structure.angular_velocity[0].dot(axis).abs();
        if about > spin.0 {
            spin = (about, tick);
        }
        if b.sleep.asleep {
            asleep.get_or_insert(tick);
        } else {
            asleep = None;
        }
    }
    println!("most work {work:?}, most spin {spin:?}, asleep from {asleep:?}");
    assert!(
        work.0 <= MOST_WORK,
        "{} iterations at tick {}",
        work.0,
        work.1
    );
    assert!(spin.0 <= MOST_SPIN, "{} rad/s at tick {}", spin.0, spin.1);
    assert!(
        asleep.is_some_and(|t| t <= ASLEEP_BY),
        "asleep from tick {asleep:?}"
    );
}
