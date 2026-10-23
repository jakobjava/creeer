//! The steel play scenes of G2.8 (`docs/g2-plan.md`): structures at the reference load.
//!
//! - *tower*: a 20-storey steel frame of about 20,000 beams (17 × 20 × 17 bays of 3 m, round
//!   tubes, welded), standing on its columns, at rest.
//! - *collapse*: the same tower, one ground-floor column gone (severed as the scene starts): its
//!   load finds other paths, or the frame comes down as far as it must.
//! - *drop*: a 10 t block falls 3 m onto the middle of a 40 m steel bridge.
//!
//! The structures are assembled by script (as play scenes are, plan.md G2), settled under
//! their weight first, so the scene starts from rest.

use super::scenes::world;
use crate::ServerWorld;
use creeer_math::glam::{DQuat, DVec3};
use creeer_physics::structure::section::STEEL_S235;
use creeer_physics::structure::shapes::{Base, space_frame};
use creeer_physics::structure::{Section, Structure, statics};
use creeer_physics::world::Body;
use creeer_physics::world::shape::Shape;
use creeer_terrain::Terrain;
use creeer_terrain::geology::deposits::tangent_frame;
use std::sync::Arc;

/// The tower's bays and their size (m).
const TOWER: [usize; 3] = [17, 20, 17];
const BAY: f64 = 3.0;
/// Where it stands from the spawn (m east: clear of the players).
const OUT: f64 = 40.0;

/// `s` (made about the origin, y up) stood on the ground `out` m east of the spawn, settled under
/// its weight first.
fn stood(w: &mut ServerWorld, mut s: Structure, out: f64, lift: f64) {
    s.gravity = DVec3::NEG_Y * 9.81;
    statics::nonlinear(&mut s, 200);
    let spawn = w.spawn();
    let (east, _) = tangent_frame(spawn.normalize());
    let ground = w.ground_under(spawn + east * out);
    let turn = DQuat::from_rotation_arc(DVec3::Y, ground.normalize());
    for p in &mut s.position {
        *p = ground + turn * (*p + DVec3::Y * lift);
    }
    for q in &mut s.orientation {
        *q = turn * *q;
    }
    let mut body = Body::structure(s, crate::world::DT);
    body.solver.reset_history(&body.structure);
    w.add_body(body, None);
}

fn tower() -> Structure {
    space_frame(
        TOWER,
        BAY,
        Section::round_tube(0.2, 0.01),
        STEEL_S235,
        Base::Every(1),
    )
}

/// *tower*.
pub fn tower_world(terrain: Arc<Terrain>) -> ServerWorld {
    let mut w = world(terrain);
    stood(&mut w, tower(), OUT, 0.0);
    w
}

/// *collapse*: a corner column of the ground floor gone.
pub fn collapse_world(terrain: Arc<Terrain>) -> ServerWorld {
    let mut w = world(terrain);
    let mut s = tower();
    s.gravity = DVec3::NEG_Y * 9.81;
    statics::nonlinear(&mut s, 200);
    // The member from the corner node at the ground (0, 0, 0) up to (0, BAY, 0).
    let column = (0..s.element_count()).find(|&e| {
        s.member(e).is_some_and(|(a, b, _)| {
            let (pa, pb) = (s.position[a], s.position[b]);
            pa.length() < 0.5 && (pb - DVec3::Y * BAY).length() < 0.5
        })
    });
    if let Some(e) = column {
        s.sever(e);
    }
    stood(&mut w, s, OUT, 0.0);
    w
}

/// *drop*: a 40 m bridge (20 × 2 × 4 bays of 2 m, on piers every 10 bays) and a 10 t block 3 m
/// above its middle.
pub fn drop_world(terrain: Arc<Terrain>) -> ServerWorld {
    let mut w = world(terrain);
    let bridge = space_frame(
        [20, 2, 4],
        2.0,
        Section::round_tube(0.2, 0.01),
        STEEL_S235,
        Base::Every(10),
    );
    stood(&mut w, bridge, 25.0, 0.0);
    let spawn = w.spawn();
    let (east, _) = tangent_frame(spawn.normalize());
    let ground = w.ground_under(spawn + east * 25.0);
    let up = ground.normalize();
    // Over the middle of the deck (x 20 m, z 4 m in the bridge's frame), 3 m above its top
    // (4 m) and its half height.
    let turn = DQuat::from_rotation_arc(DVec3::Y, up);
    let middle = ground + turn * DVec3::new(20.0, 7.5, 4.0);
    let mut block = Body::rigid(
        Shape::Box {
            half: DVec3::new(1.0, 0.5, 1.0),
        },
        10_000.0,
        middle,
        crate::world::DT,
    );
    block.structure.orientation[0] = turn;
    w.add_body(block, None);
    w
}
