//! *toolmaking* (G1.10): four players making their first tool, as a survival player does. Each
//! has a station 3 m out from the spawn where a chert core, a basalt hammerstone and a fresh
//! spruce branch lie. Each takes up the hammerstone and knaps the core three times, puts the
//! hammerstone down, takes up the branch, strips its bark, binds the sharpest flake on as a
//! head, and then digs the ground before it with the axe it made, a blow every cycle.

use super::Play;
use super::record::Act;
use super::scenes::world;
use super::work::{apart, towards};
use crate::ServerWorld;
use creeer_math::det;
use creeer_math::glam::{DQuat, DVec3};
use creeer_net::message::Command;
use creeer_sim::items::{Form, Item, Piece};
use creeer_terrain::Terrain;
use creeer_terrain::geology::deposits::tangent_frame;
use std::sync::Arc;

/// How far out the stations are (m), and how far before them the players stand.
const OUT: f64 = 3.0;
const BEFORE: f64 = 0.7;
/// When each step is asked (ticks; each player a few ticks after the one before).
const TAKE_HAMMER: u32 = 300;
const KNAP: [u32; 3] = [420, 550, 680];
const PUT_DOWN: u32 = 820;
const TAKE_BRANCH: u32 = 880;
const STRIP: u32 = 940;
const BIND: u32 = 1000;
const DIG_FROM: u32 = 1080;
const EVERY: u32 = 125;

/// Station `k`'s centre on the ground, where its player stands, and the way out.
fn station(w: &ServerWorld, k: usize) -> (DVec3, DVec3, DVec3) {
    let spawn = w.spawn();
    let (east, north) = tangent_frame(spawn.normalize());
    #[allow(clippy::cast_precision_loss)] // four
    let (s, c) = det::sin_cos(k as f64 * std::f64::consts::FRAC_PI_2 + 0.4);
    let out = east * c + north * s;
    let centre = w.ground_under(spawn + out * OUT);
    let stand = w.ground_under(spawn + out * (OUT - BEFORE));
    (centre, stand, out)
}

fn lump(material: &str, density: f64, size: DVec3) -> Item {
    Item::Piece(
        Piece::solid(material, density, Form::Lump { size }),
        Vec::new(),
    )
}

/// The world: at each station a core, a hammerstone and a branch, lying.
pub fn toolmaking_world(terrain: Arc<Terrain>) -> ServerWorld {
    let mut w = world(terrain);
    for k in 0..4 {
        let (centre, _, out) = station(&w, k);
        let up = centre.normalize();
        let side = out.cross(up).normalize();
        let turn = DQuat::from_rotation_arc(DVec3::Z, up);
        let core = lump("chert", 2580.0, DVec3::new(0.14, 0.10, 0.06));
        let hammer = lump("basalt", 2900.0, DVec3::new(0.10, 0.08, 0.06));
        let branch = Item::Piece(
            Piece::solid(
                "spruce_green",
                800.0,
                Form::Rod {
                    radius: 0.015,
                    length: 0.6,
                },
            ),
            Vec::new(),
        );
        w.lay(core, centre, turn);
        w.lay(hammer, centre + side * 0.35, turn);
        w.lay(branch, centre - side * 0.4, turn);
    }
    w
}

/// The laid things near `at` that `pick` takes, by the client's view of them: the first.
fn near(
    play: &Play,
    k: usize,
    at: DVec3,
    pick: impl Fn(&creeer_net::message::Laid) -> bool,
) -> Option<u64> {
    let mut found: Vec<(u64, f64)> = (play.client(k).laid.values())
        .filter(|l| (l.centre - at).length() < 0.8 && pick(l))
        .map(|l| (l.id, (l.centre - at).length()))
        .collect();
    found.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    found.first().map(|f| f.0)
}

/// *toolmaking*'s acts this tick.
pub fn toolmaking(tick: u32, play: &Play) -> Vec<Act> {
    (0..play.players())
        .map(|k| {
            let w = &play.server.world;
            let (centre, stand, out) = station(w, k);
            let me = play.avatar(k);
            let mut act = Act::walk(towards(me, stand, 0.1));
            if apart(me, stand) > 0.5 {
                return act;
            }
            let index = u32::try_from(k).unwrap_or(0);
            let t = tick.saturating_sub(7 * index);
            let stone = |l: &creeer_net::message::Laid| l.word == "stone";
            let command = match t {
                TAKE_HAMMER => {
                    // The lighter of the two stones.
                    let mut stones: Vec<(u64, f64)> = (play.client(k).laid.values())
                        .filter(|l| (l.centre - centre).length() < 0.8 && stone(l))
                        .map(|l| (l.id, l.mass))
                        .collect();
                    stones.sort_by(|a, b| a.1.total_cmp(&b.1));
                    stones.first().map(|s| Command::Pick {
                        loose: s.0,
                        pocket: false,
                    })
                }
                _ if KNAP.contains(&t) => {
                    near(play, k, centre, stone).map(|id| play.client(k).strike_loose_command(id))
                }
                PUT_DOWN => Some(Command::Drop),
                TAKE_BRANCH => {
                    near(play, k, centre, |l| l.word == "stick").map(|id| Command::Pick {
                        loose: id,
                        pocket: false,
                    })
                }
                STRIP => Some(Command::Strip),
                BIND => near(play, k, centre, |l| l.word == "flake").map(Command::BindLying),
                _ if t >= DIG_FROM && (t - DIG_FROM).is_multiple_of(EVERY) => {
                    let client = play.client(k);
                    let up = me.normalize();
                    let ground = w.ground_under(me + out * 1.0);
                    client
                        .aim(ground + up, -up, 2.0)
                        .filter(|a| a.body.is_none())
                        .map(|a| client.strike_command(a))
                }
                _ => None,
            };
            act.commands.extend(command);
            act
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play::scenes::{scene, terrain};

    /// The whole scene: every player makes an axe and digs with it.
    #[test]
    fn every_player_makes_a_tool_and_digs_with_it() {
        let s = scene("toolmaking").expect("the scene");
        let mut play = Play::new((s.build)(terrain()), s.name, s.players, &terrain());
        let mut blows = 0;
        let mut made = 0;
        for t in 0..s.ticks.0 {
            let acts = (s.script)(t, &play);
            play.tick(&acts);
            blows += play.server.world.take_blows().len();
            if t == DIG_FROM + 30 {
                made = (0..4)
                    .filter(|&k| {
                        (play.client(k).inventory.hands.as_ref()).is_some_and(|h| h.word == "tool")
                    })
                    .count();
            }
        }
        let refused: Vec<_> = (0..4).map(|k| play.client(k).refused.clone()).collect();
        println!("{made} tools made, {blows} blows; refused {refused:?}");
        assert_eq!(made, 4, "every player made a tool");
        assert!(blows >= 4 * 3 + 4 * 3, "knapped and dug: {blows} blows");
    }
}

#[cfg(test)]
mod probe {
    #[test]
    #[ignore = "prints the slow ticks and their phases"]
    fn slow_ticks() {
        let s = crate::play::scenes::scene("toolmaking").unwrap();
        crate::play::scenes::run_with(s, s.ticks.0, |t, c| {
            if c.ms > 3.0 {
                let phases: Vec<String> = (creeer_physics::profile::NAMES.iter())
                    .zip(c.phases)
                    .filter(|(_, ns)| *ns > 300_000)
                    .map(|((_, n), ns)| {
                        format!(
                            "{n} {:.1}",
                            1e-6 * f64::from(u32::try_from(ns).unwrap_or(u32::MAX))
                        )
                    })
                    .collect();
                println!("tick {t}: {:.1} ms, work {}; {phases:?}", c.ms, c.work);
            }
        });
    }
}
