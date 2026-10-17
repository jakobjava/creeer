//! What a command puts in the world: a steel space frame standing upright where it is put.

use crate::world::DT;
use creeer_math::glam::{DQuat, DVec3};
use creeer_physics::structure::Section;
use creeer_physics::structure::section::STEEL_S235;
use creeer_physics::structure::shapes::{Base, space_frame};
use creeer_physics::world::Body;

/// An orientation standing upright on the local horizon at `at`.
pub(crate) fn upright(at: DVec3) -> DQuat {
    DQuat::from_rotation_arc(DVec3::Y, at.normalize())
}

/// A steel space frame (2 × 2 × 2 cells of 1 m, welded tubes of 50 mm, 3 mm wall: 160 kg)
/// standing upright with its base centred at `at`.
pub(crate) fn frame(at: DVec3) -> Body {
    let structure = space_frame(
        [2, 2, 2],
        1.0,
        Section::round_tube(0.05, 0.003),
        STEEL_S235,
        Base::Free,
    );
    let mut body = Body::structure(structure, DT);
    let turn = upright(at);
    let s = &mut body.structure;
    for (p, q) in s.position.iter_mut().zip(&mut s.orientation) {
        // Turned as a whole (every node with it, so no member is strained).
        *p = at + turn * (*p - DVec3::new(1.0, 0.0, 1.0));
        *q = turn;
    }
    body.friction = 0.6;
    body.solver.refactor(&body.structure);
    body.solver.reset_history(&body.structure);
    body
}
