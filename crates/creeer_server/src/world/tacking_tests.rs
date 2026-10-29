//! A scene for holding and fastening (G2.10, ADR-0036): two poles crossing before Ada's feet,
//! and her hold of one where it lies (`grip_told_tests.rs`; the build itself is played as the
//! player plays it in `server/build_flow_tests.rs`).

use super::building_tests::{centre, pole, wait};
use super::tests::world;
use super::*;
use creeer_math::glam::DQuat;

/// Ada, a pole lying across her way 0.6 m before her (the one she will hold), and on it, along
/// her way, another crossing it 10 cm from its near end: their ids and where they cross.
pub(super) fn across() -> (ServerWorld, EntityId, EntityId, EntityId, DVec3) {
    let mut w = world();
    w.creative = true;
    let ada = w.avatar_of("ada");
    wait(&mut w, ada, 60);
    let me = centre(&w, ada);
    let up = character::plumb(&w.world, me);
    let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
    // A rod lies along its own x.
    let lying = |axis: DVec3| {
        let side = up.reject_from(axis).normalize();
        DQuat::from_mat3(&creeer_math::glam::DMat3::from_cols(
            axis,
            side,
            axis.cross(side),
        ))
    };
    let lay = |w: &mut ServerWorld, id, at: DVec3, axis: DVec3| {
        let i = w.index(id).expect("there");
        let b = &mut w.world.bodies[i];
        (b.structure.position[0], b.structure.orientation[0]) = (at, lying(axis));
        b.solver.reset_history(&b.structure);
    };
    let at = w.ground_under(me + east * 0.6) + up * 0.03;
    let held = w.add_item(pole(), at, true);
    lay(&mut w, held, at, east.cross(up).normalize());
    // The other from on the held one to the ground beyond (the ground may rise or fall).
    let on = at + up * 0.051;
    let along = (w.ground_under(me + east * 2.1) + up * 0.03 - on).normalize();
    let other = w.add_item(pole(), on + along * 0.7, true);
    lay(&mut w, other, on + along * 0.7, along);
    wait(&mut w, ada, 90);
    let j = w.index(other).expect("there");
    let touch = (w
        .world
        .touching(w.index(held).expect("there"), 0.005)
        .into_iter())
    .find(|t| t.other.is_some_and(|(k, _)| k == j))
    .expect("they cross");
    (w, ada, held, other, touch.point)
}

/// Ada's hold of `part` where it lies now, held steady or not (G2.10).
pub(super) fn held_where_it_lies(
    w: &ServerWorld,
    ada: EntityId,
    part: EntityId,
    steady: bool,
) -> AvatarInput {
    let s = &w.world.bodies[w.index(part).expect("there")].structure;
    AvatarInput {
        hold: Some(creeer_net::message::Hold {
            offset: s.position[0] - centre(w, ada),
            turn: s.orientation[0],
            steady,
        }),
        ..AvatarInput::default()
    }
}
