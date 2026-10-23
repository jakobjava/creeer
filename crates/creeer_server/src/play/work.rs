//! *work* (G1.4g): players at work with their hands. Four strike a crate each with a tool (an
//! axe or a pick, a blow every two seconds); four carry a 15 kg box each 6 m out from where
//! they stand, throw it, and fetch it again. Every act is asked as the game asks it: aimed from
//! the player's own client, at what it draws, a little in the past.

use super::Play;
use super::record::Act;
use super::scenes::world;
use crate::ServerWorld;
use crate::client::Aimed;
use creeer_math::det;
use creeer_math::glam::DVec3;
use creeer_net::entity::EntityId;
use creeer_net::message::{Command, Target};
use creeer_physics::world::Body;
use creeer_physics::world::shape::Shape;
use creeer_terrain::Terrain;
use creeer_terrain::geology::deposits::tangent_frame;
use std::sync::Arc;

/// The players who strike (the first four) and who carry (the rest).
const STRIKERS: usize = 4;
/// The boxes the carriers carry (kg, m on a side).
const BOX: (f64, f64) = (15.0, 0.4);
/// How far out they carry them (m), and how near they walk to what they work on (m).
const OUT: f64 = 6.0;
const NEAR: f64 = 1.3;
/// The ticks between asking (a player asks once in this many, at its own tick).
const EVERY: u32 = 30;

/// The world: a crate for each striker and a box for each carrier in a ring about the spawn
/// (the crates first: entities 1–4, then the boxes: 5–8).
pub fn work_world(terrain: Arc<Terrain>) -> ServerWorld {
    let mut w = world(terrain);
    let spawn = w.spawn();
    let (east, north) = tangent_frame(spawn.normalize());
    for k in 0..8u32 {
        let (s, c) = det::sin_cos(f64::from(k) * std::f64::consts::FRAC_PI_4);
        let ground = w.ground_under(spawn + (east * c + north * s) * 5.0);
        let up = ground.normalize();
        if k < 4 {
            w.apply(&Command::Crate(ground + up * 0.55), None)
                .expect("a crate");
        } else {
            let half = 0.5 * BOX.1;
            let mut body = Body::rigid(
                Shape::Box {
                    half: DVec3::splat(half),
                },
                BOX.0,
                ground + up * (half + 0.01),
                crate::world::DT,
            );
            body.structure.orientation[0] = crate::things::upright(ground);
            body.friction = 0.6;
            w.add_body(body, None);
        }
    }
    w
}

/// Where entity `id` is now on the server.
pub(super) fn at(play: &Play, id: EntityId) -> Option<DVec3> {
    let w = &play.server.world;
    w.index(id).map(|i| w.world.bodies[i].structure.position[0])
}

/// A walk towards `to` from `from` along the ground, stopping `near` m from it.
pub(super) fn towards(from: DVec3, to: DVec3, near: f64) -> DVec3 {
    let up = from.normalize();
    let d = (to - from).reject_from(up);
    if d.length() > near {
        d.normalize() * (d.length() - near).clamp(0.3, 1.2)
    } else {
        DVec3::ZERO
    }
}

/// What player `k` aims at on `id`: its top's middle, from above, as it draws it.
fn aim(play: &Play, k: usize, id: EntityId) -> Option<Aimed> {
    let client = play.client(k);
    let drawn = client.poses().get(&id)?.position;
    let up = drawn.normalize();
    let a = client.aim(drawn + up * 3.0, -up, 5.0)?;
    (a.body == Some(id)).then_some(a)
}

/// *work*'s acts this tick.
pub fn work(tick: u32, play: &Play) -> Vec<Act> {
    (0..play.players())
        .map(|k| {
            let index = u32::try_from(k).unwrap_or(0);
            let thing = EntityId(index + 1);
            let me = play.avatar(k);
            let mine = tick % EVERY == (7 * index) % EVERY;
            let Some(there) = at(play, thing) else {
                return Act::default();
            };
            if k < STRIKERS {
                striking(play, k, tick, (me, there, thing), mine)
            } else {
                carrying(play, k, (me, there, thing), mine)
            }
        })
        .collect()
}

/// How far apart two points are along the ground (m).
pub(super) fn apart(a: DVec3, b: DVec3) -> f64 {
    (b - a).reject_from(a.normalize()).length()
}

/// Whether player `k` stands still (people stop to pick things up).
fn still(play: &Play, k: usize) -> bool {
    let w = &play.server.world;
    let avatar = play.client(k).welcome.as_ref().map(|w| w.avatar);
    avatar
        .and_then(|a| w.index(a))
        .is_some_and(|i| w.world.bodies[i].structure.velocity[0].length() < 0.1)
}

/// A striker: its tool taken in hand, to its crate, a blow every two seconds.
fn striking(
    play: &Play,
    k: usize,
    tick: u32,
    (me, there, id): (DVec3, DVec3, EntityId),
    mine: bool,
) -> Act {
    let mut act = Act::walk(towards(me, there, NEAR));
    if tick == 1 {
        // The axe and the pick by turns.
        let tool = if k.is_multiple_of(2) { 2 } else { 0 };
        act.commands.push(Command::Wield { tool: Some(tool) });
    } else if mine
        && apart(me, there) < NEAR + 0.3
        && (tick / EVERY).is_multiple_of(4)
        && let Some(a) = aim(play, k, id)
    {
        act.commands.push(play.client(k).strike_command(a));
    }
    act
}

/// A carrier: to its box, stopped, taken up, lifted, carried out its own way, thrown, and
/// fetched again.
fn carrying(play: &Play, k: usize, (me, there, id): (DVec3, DVec3, EntityId), mine: bool) -> Act {
    let w = &play.server.world;
    let avatar = play.client(k).welcome.as_ref().map(|w| w.avatar);
    let held = avatar.and_then(|a| w.carried(a)) == Some(id);
    if !held {
        let mut act = Act::walk(towards(me, there, 0.9));
        // (People wait for a thrown box to come to rest before they take it up.)
        let resting = w.index(id).is_some_and(|i| {
            let s = &w.world.bodies[i].structure;
            s.velocity[0].length() < 0.05 && s.angular_velocity[0].length() < 0.1
        });
        if mine
            && resting
            && apart(me, there) < 1.2
            && still(play, k)
            && let Some(a) = aim(play, k, id)
        {
            let target = Target {
                body: a.body,
                at: a.point,
                seen: a.seen,
            };
            act.commands.push(Command::Grab(target));
        }
        return act;
    }
    if !avatar.is_some_and(|a| w.lifted(a)) {
        return Act::default();
    }
    // Out from the spawn, each its own way.
    let spawn = w.spawn();
    let (east, north) = tangent_frame(spawn.normalize());
    #[allow(clippy::cast_precision_loss)]
    let (s, c) = det::sin_cos((k as f64 + 0.5) * std::f64::consts::FRAC_PI_4);
    let away = east * c + north * s;
    let goal = spawn + away * (5.0 + OUT);
    let mut act = Act::walk(towards(me, goal, 0.3));
    if mine && apart(me, goal) < 0.8 {
        act.commands
            .push(Command::Throw((away + me.normalize()).normalize()));
    }
    act
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play::scenes::{scene, terrain};

    /// The whole scene: every strike asked falls, and every load taken up is carried until it
    /// is thrown (the arms give, so the body's bob and jerks shake no load free).
    #[test]
    fn at_work_every_blow_falls_and_no_load_is_dropped() {
        let s = scene("work").expect("the scene");
        let mut play = Play::new((s.build)(terrain()), s.name, s.players, &terrain());
        let (mut strikes, mut blows, mut grabs, mut throws, mut drops) = (0, 0, 0, 0, 0);
        let mut held_before = [false; 8];
        let mut threw = [0u32; 8];
        for t in 0..s.ticks.0 {
            let acts = (s.script)(t, &play);
            for (k, a) in acts.iter().enumerate() {
                for c in &a.commands {
                    match c {
                        Command::Throw(_) => threw[k] = t,
                        Command::Strike { .. } => strikes += 1,
                        _ => {}
                    }
                }
            }
            play.tick(&acts);
            blows += play.server.world.take_blows().len();
            for (k, before) in held_before.iter_mut().enumerate().skip(STRIKERS) {
                let a = play.client(k).welcome.as_ref().map(|w| w.avatar);
                let held = a.and_then(|a| play.server.world.carried(a)).is_some();
                if held && !*before {
                    grabs += 1;
                }
                if *before && !held {
                    if t - threw[k] < 20 {
                        throws += 1;
                    } else {
                        drops += 1;
                    }
                }
                *before = held;
            }
        }
        println!(
            "{strikes} strikes, {blows} blows; {grabs} taken up, {throws} thrown, {drops} dropped"
        );
        assert!(
            strikes > 30 && blows == strikes,
            "{blows} of {strikes} blows fell"
        );
        assert!(
            throws >= 8 && drops == 0,
            "{drops} dropped, {throws} thrown"
        );
    }
}
