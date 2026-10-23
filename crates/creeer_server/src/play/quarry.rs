//! *quarry* (G1.10): four players with picks along the near rim of a pit 5 m across dug into the
//! ground east of the spawn, four boulders lying in it. Each digs the wall before it, a blow every cycle, its
//! heap growing beside it; each side has been dug a while (its ledger owes nearly a pit), so
//! the terrain is cut during the scene as it is in play, every so often.

use super::Play;
use super::record::Act;
use super::scenes::world;
use super::work::{apart, towards};
use crate::ServerWorld;
use creeer_math::det;
use creeer_math::glam::DVec3;
use creeer_net::message::Command;
use creeer_terrain::Terrain;
use creeer_terrain::edits::{Brush, Edit, Edits};
use creeer_terrain::geology::deposits::tangent_frame;
use std::sync::Arc;

/// The pit: how far east of the spawn its centre is, its radius and its depth (m).
const AT: f64 = 8.0;
const RADIUS: f64 = 2.5;
const DEPTH: f64 = 1.2;
/// What each side's ledger owes when the scene begins (m³): about a pit, so its first blows cut.
const OWED: f64 = 0.5;
/// The ticks between a player's blows.
const EVERY: u32 = 125;

/// The pit's centre on the ground, and the local up.
fn pit(w: &ServerWorld) -> (DVec3, DVec3) {
    let (east, _) = tangent_frame(w.spawn().normalize());
    let ground = w.ground_under(w.spawn() + east * AT);
    (ground, ground.normalize())
}

/// Where player `k` stands on the rim (the spawn's side: none walks through the pit), and the
/// ground it digs before it (the pit's lip).
fn side(w: &ServerWorld, k: usize) -> (DVec3, DVec3) {
    let (centre, up) = pit(w);
    let (east, north) = tangent_frame(up);
    #[allow(clippy::cast_precision_loss)] // four
    let (s, c) = det::sin_cos(std::f64::consts::PI + (k as f64 - 1.5) * 0.7);
    let out = east * c + north * s;
    let stand = w.ground_under(centre + out * (RADIUS + 0.9) + up * 2.0);
    let dig = w.ground_under(centre + out * RADIUS + up * 2.0);
    (stand, dig)
}

/// The world: the pit dug, four boulders dropped into it, and each side's ledger.
pub fn quarry_world(terrain: Arc<Terrain>) -> ServerWorld {
    let mut w = world(terrain);
    let (centre, up) = pit(&w);
    let mut edits = Edits::default();
    edits.push(Edit::Dig(Brush::Sphere {
        center: centre + up * (RADIUS - DEPTH),
        radius: RADIUS,
    }));
    w.apply(&Command::Edit(edits.to_bytes()), None)
        .expect("the pit");
    let (east, north) = tangent_frame(up);
    for k in 0..4u32 {
        let (s, c) = det::sin_cos(f64::from(k) * std::f64::consts::FRAC_PI_2 + 0.6);
        let floor = w.ground_under(centre + (east * c + north * s) * 1.0);
        w.apply(&Command::Ball(floor + up * 0.6), None)
            .expect("a boulder");
    }
    for k in 0..4 {
        let (_, dig) = side(&w, k);
        w.owe_dug(dig, OWED);
    }
    w
}

/// *quarry*'s acts this tick.
pub fn quarry(tick: u32, play: &Play) -> Vec<Act> {
    (0..play.players())
        .map(|k| {
            let w = &play.server.world;
            let (stand, dig) = side(w, k);
            let me = play.avatar(k);
            let mut act = Act::walk(towards(me, stand, 0.15));
            let index = u32::try_from(k).unwrap_or(0);
            if tick == 1 {
                act.commands.push(Command::Wield { tool: Some(0) });
            } else if tick % EVERY == (29 * index) % EVERY && apart(me, stand) < 0.5 {
                let client = play.client(k);
                let up = dig.normalize();
                if let Some(a) = client
                    .aim(dig + up * 1.0, -up, 2.0)
                    .filter(|a| a.body.is_none())
                {
                    act.commands.push(client.strike_command(a));
                }
            }
            act
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play::scenes::{scene, terrain};

    /// The whole scene: blows dig, heaps grow, and the terrain is cut.
    #[test]
    fn the_quarry_digs_and_cuts_its_pit() {
        let s = scene("quarry").expect("the scene");
        let mut play = Play::new((s.build)(terrain()), s.name, s.players, &terrain());
        let (mut asked, mut fell) = (0, 0);
        let edits = play.server.world.terrain.edits().len();
        for t in 0..s.ticks.0 {
            let acts = (s.script)(t, &play);
            asked += acts.iter().map(|a| a.commands.len()).sum::<usize>();
            play.tick(&acts);
            fell += play.server.world.take_blows().len();
        }
        let w = &play.server.world;
        let cuts = w.terrain.edits().len() - edits;
        println!(
            "{asked} asked, {fell} blows; {cuts} cuts, {:.2} m³",
            w.cut_dug()
        );
        assert!(fell >= 40, "blows fell: {fell} of {asked}");
        assert!(cuts >= 4 && w.cut_dug() > 1.0, "the pit was cut");
    }
}
