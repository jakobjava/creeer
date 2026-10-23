//! *builders* (G2.8): eight players building by hand at once. Each has two green spruce poles
//! laid across each other before them; they nail them where they cross, take the joint apart
//! again, and nail it again, as long as the scene runs: the fasten and unfasten commands, their
//! time, the structures made and fallen apart again, eight at a time.

use super::Play;
use super::record::Act;
use super::scenes::world;
use crate::ServerWorld;
use creeer_math::det;
use creeer_math::glam::{DQuat, DVec3};
use creeer_net::message::{Command, Target};
use creeer_sim::items::{Form, Item, Piece};
use creeer_terrain::Terrain;
use creeer_terrain::geology::deposits::tangent_frame;
use std::sync::Arc;

/// How far out the poles lie from the spawn (m), and how near each player stands to them.
const OUT: f64 = 4.0;
const NEAR: f64 = 1.0;
/// A cycle of a player's work (ticks): nail at its start, take apart halfway.
const CYCLE: u32 = 900;

/// The world: two poles crossed before each of eight places round the spawn.
pub fn builders_world(terrain: Arc<Terrain>) -> ServerWorld {
    let mut w = world(terrain);
    let spawn = w.spawn();
    let (east, north) = tangent_frame(spawn.normalize());
    let pole = || {
        Item::Piece(
            Piece::solid(
                "spruce_green",
                800.0,
                Form::Rod {
                    radius: 0.025,
                    length: 1.6,
                },
            ),
            Vec::new(),
        )
    };
    for k in 0..8u32 {
        let (s, c) = det::sin_cos(f64::from(k) * std::f64::consts::FRAC_PI_4);
        let ground = w.ground_under(spawn + (east * c + north * s) * OUT);
        let up = ground.normalize();
        w.add_item(pole(), ground + up * 0.03, true);
        let above = w.add_item(pole(), ground + up * 0.09, true);
        if let Some(i) = w.index(above) {
            let s = &mut w.world.bodies[i].structure;
            s.orientation[0] =
                DQuat::from_axis_angle(up, std::f64::consts::FRAC_PI_2) * s.orientation[0];
        }
    }
    w
}

/// Every player's act: towards its poles, then nailing or taking apart where they cross.
pub fn builders(tick: u32, play: &Play) -> Vec<Act> {
    let w = &play.server.world;
    (0..play.players())
        .map(|k| {
            let mut act = Act::default();
            // Its upper pole: the second of its pair (the poles are made first, ids 1–16; the
            // upper keeps its id as it is nailed and taken apart).
            #[allow(clippy::cast_possible_truncation)] // eight
            let upper = creeer_net::entity::EntityId(2 * k as u32 + 2);
            let Some(at) = crossing(w, upper) else {
                return act;
            };
            // Up to it first: within reach of where they cross.
            act.walk = super::work::towards(play.avatar(k), at, NEAR);
            #[allow(clippy::cast_possible_truncation)] // eight
            let phase = (tick + 60 * k as u32) % CYCLE;
            if phase != 120 && phase != CYCLE / 2 + 120 {
                return act;
            }
            let target = Target {
                body: Some(upper),
                at,
                #[allow(clippy::cast_precision_loss)]
                seen: w.tick as f64,
            };
            act.commands.push(if phase == 120 {
                Command::Fasten {
                    target,
                    fastener: "nail_100".into(),
                }
            } else {
                Command::Unfasten(target)
            });
            act
        })
        .collect()
}

/// Where body `id` touches another body, if it does.
fn crossing(w: &ServerWorld, id: creeer_net::entity::EntityId) -> Option<DVec3> {
    let i = w.index(id)?;
    (w.world.touching(i, 0.005).into_iter())
        .find(|t| t.other.is_some_and(|(j, _)| j != i))
        .map(|t| t.point)
}
