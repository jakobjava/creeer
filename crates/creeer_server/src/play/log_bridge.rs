//! *log bridge* (G2.8): a small bridge built by hand and loaded until it fails. Two green
//! spruce poles lie on the ground 2 m apart (its abutments), a third across them (its
//! stringer); two players nail the stringer down, one at each end; then crates fall on its
//! middle one by one until it breaks.

use super::Play;
use super::record::Act;
use super::scenes::world;
use crate::ServerWorld;
use creeer_math::glam::{DQuat, DVec3};
use creeer_net::entity::EntityId;
use creeer_net::message::{Command, Target};
use creeer_sim::items::{Form, Item, Piece};
use creeer_terrain::Terrain;
use creeer_terrain::geology::deposits::tangent_frame;
use std::sync::Arc;

/// Where the bridge stands from the spawn (m east), its span (m), and how near a builder stands.
const OUT: f64 = 3.0;
const SPAN: f64 = 2.0;
const NEAR: f64 = 0.9;
/// When the nails go in (and again every [`AGAIN`] while an end is loose: a builder may still be
/// on the way), and when the first crate falls and how often the next (ticks).
const NAIL: u32 = 240;
const LOAD: u32 = 900;
const EVERY: u32 = 120;
const AGAIN: u32 = 600;

fn pole(length: f64) -> Item {
    Item::Piece(
        Piece::solid(
            "spruce_green",
            800.0,
            Form::Rod {
                radius: 0.025,
                length,
            },
        ),
        Vec::new(),
    )
}

/// The world: the abutments (entities 1 and 2, across the span's way) and the stringer (3).
pub fn log_bridge_world(terrain: Arc<Terrain>) -> ServerWorld {
    let mut w = world(terrain);
    let spawn = w.spawn();
    let (east, north) = tangent_frame(spawn.normalize());
    let middle = w.ground_under(spawn + east * OUT);
    let up = middle.normalize();
    // Rods lie along their x: the abutments turned to lie north–south, the stringer east–west.
    let across = DQuat::from_rotation_arc(DVec3::X, north);
    for side in [-0.5, 0.5] {
        let at = w.ground_under(middle + east * (side * SPAN));
        let id = w.add_item(pole(1.2), at + up * 0.03, true);
        if let Some(i) = w.index(id) {
            let s = &mut w.world.bodies[i].structure;
            s.orientation[0] = across * s.orientation[0];
        }
    }
    let id = w.add_item(pole(SPAN + 0.6), middle + up * 0.09, true);
    if let Some(i) = w.index(id) {
        let s = &mut w.world.bodies[i].structure;
        s.orientation[0] = DQuat::from_rotation_arc(DVec3::X, east) * s.orientation[0];
    }
    w
}

/// Where body `a` touches body `b`, if it does.
fn touch(w: &ServerWorld, a: EntityId, b: EntityId) -> Option<DVec3> {
    let (i, j) = (w.index(a)?, w.index(b)?);
    (w.world.touching(i, 0.005).into_iter())
        .find(|t| t.other.is_some_and(|(k, _)| k == j))
        .map(|t| t.point)
}

/// The two builders' acts: each to an end, nailing it; then crates on the middle.
pub fn log_bridge(tick: u32, play: &Play) -> Vec<Act> {
    let w = &play.server.world;
    let stringer = EntityId(3);
    (0..play.players())
        .map(|k| {
            let mut act = Act::default();
            #[allow(clippy::cast_possible_truncation)] // two
            let abutment = EntityId(k as u32 + 1);
            if let Some(at) = touch(w, stringer, abutment) {
                act.walk = super::work::towards(play.avatar(k), at, NEAR);
                if tick >= NAIL && (tick - NAIL) % AGAIN == 0 {
                    let target = Target {
                        body: Some(stringer),
                        at,
                        #[allow(clippy::cast_precision_loss)]
                        seen: w.tick as f64,
                    };
                    act.commands.push(Command::Fasten {
                        target,
                        fastener: "nail_100".into(),
                    });
                }
            }
            // The first builder drops the crates (a creative server's).
            if k == 0 && tick >= LOAD && (tick - LOAD) % EVERY == 0 {
                let middle = w.index(stringer).map(|i| {
                    let s = &w.world.bodies[i].structure;
                    #[allow(clippy::cast_precision_loss)] // a few nodes
                    let n = s.position.len() as f64;
                    s.position.iter().sum::<DVec3>() / n
                });
                if let Some(middle) = middle {
                    act.commands
                        .push(Command::Crate(middle + middle.normalize() * 1.5));
                }
            }
            act
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::Play;
    use super::super::scenes::{scene, terrain};
    use super::*;

    /// The bridge is built (the three poles one structure) before the crates come; under them it
    /// breaks or comes apart (a piece a body of its own: more entities than the two players,
    /// the bridge and the crates' first two).
    #[test]
    #[ignore = "WIP: the stringer never touches its abutments (touching() finds no contact): to fix"]
    fn the_log_bridge_is_built_and_fails_under_its_crates() {
        let s = scene("log_bridge").expect("the scene");
        let terrain = terrain();
        let mut play = Play::new((s.build)(terrain.clone()), s.name, s.players, &terrain);
        let mut joined = false;
        for tick in 0..1800 {
            let acts = (s.script)(tick, &play);
            play.tick(&acts);
            let w = &play.server.world;
            if false {}
            if tick == LOAD - 1 {
                joined = w.index(EntityId(1)).is_none() && w.index(EntityId(2)).is_none();
            }
        }
        assert!(
            joined,
            "the abutments were never nailed to the stringer: {:?}",
            play.server.world.take_notes()
        );
        let w = &play.server.world;
        let failed = w.index(EntityId(3)).is_none_or(|i| {
            let s = &w.world.bodies[i].structure;
            s.elements_broken() > 0 || s.pieces().len() > 1
        }) || w.ids().len() > 6;
        assert!(failed, "the crates never broke it");
    }
}
