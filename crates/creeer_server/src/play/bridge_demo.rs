//! *bridge demo* (G2.11, ADR-0037): the bridge site's ten minutes (`world/site.rs`) played end
//! to end, faster. Two players at the gully with the site's timber and loads:
//!
//! 1. **Built** by the scene's world, as the tower is (play scenes cannot steer what the hands
//!    hold yet, and a player's J cannot yet nail a board to a second stringer of the same deck):
//!    the three stringers across the gully on edge, the twelve deck boards across them four to
//!    the metre, each nailed to each stringer (36 nails of 100 mm, the joints a player's J makes).
//! 2. **Walked over:** Ada and Ben to the far bank and back.
//! 3. **Loaded:** each carries two granite blocks from the pile and lets them go on the deck;
//!    then Ada sets twelve more from the kit along its middle, one every 1.3 s (400 kg in all).
//! 4. **Broken:** 1 t test weights set on its middle from the kit every 4 s, as bridges are
//!    proof-loaded, until it breaks.
//!
//! CI plays its first 150 ticks: the site and the bridge settling, before anyone walks onto it.
//! With people on the deck a tick takes seconds today (ADR-0037, Cost: streams A4 and A6), so the
//! whole scene is run by hand (`BRIDGE_SAVES=<dir>` saves its moments for `creeer_client --planet
//! bridge --save <file>`), and the frames of the bridge loaded and broken come from the client's
//! own game (`creeer_client --planet bridge --built --blocks <n> --weights <n>`).

use super::Play;
use super::record::Act;
use super::scenes::world;
use super::work::{apart, towards};
use crate::ServerWorld;
use crate::world::site::{BridgeSite, OUT, WIDTH};
use creeer_math::glam::{DMat3, DQuat, DVec3};
use creeer_net::entity::EntityId;
use creeer_net::message::{Command, Target};
use creeer_terrain::Terrain;
use std::sync::Arc;

/// The stringers' places (m along the gully from its middle), the deck boards' first place and
/// spacing (m across from the start), and the stringers' and boards' height and thickness (m).
const STRINGERS: [f64; 3] = [-0.4, 0.0, 0.4];
const FIRST_BOARD: f64 = OUT - 1.375;
const BOARD_EVERY: f64 = 0.25;
const BOARDS: usize = 12;
const STRINGER_DEPTH: f64 = 0.15;
const BOARD: f64 = 0.05;
/// When things happen (ticks): walking over, carrying, the kit's blocks, the test weights.
// (After the bridge has settled on its banks: CI plays the first 150 ticks, the settling.)
pub const OVER: u32 = 180;
pub const CARRY: u32 = OVER + 480;
const PER_CARRY: u32 = 600;
pub const BLOCKS: u32 = CARRY + 2 * PER_CARRY;
const PER_BLOCK: u32 = 80;
pub const WEIGHTS: u32 = BLOCKS + 12 * PER_BLOCK;
const PER_WEIGHT: u32 = 240;

/// The world: the bridge site, its stringers and deck boards nailed across the gully.
pub fn bridge_demo_world(terrain: Arc<Terrain>) -> ServerWorld {
    let mut w = world(terrain);
    let site = w.prepare_bridge_site();
    let up = site.middle.normalize();
    let stringers: Vec<EntityId> = (0..3).filter_map(|k| site.item("stringer", k)).collect();
    for (id, y) in stringers.iter().zip(STRINGERS) {
        // From bank to bank, on the ground 0.35 m back from each rim.
        let (near, far) = (bank(&w, &site, y, -1.0), bank(&w, &site, y, 1.0));
        let centre = (near + far) * 0.5 + up * (STRINGER_DEPTH * 0.5 + 0.002);
        place(&mut w, *id, centre, far - near, up);
    }
    let boards: Vec<EntityId> = (0..BOARDS)
        .filter_map(|k| site.item("deck board", k))
        .collect();
    for (k, id) in boards.iter().enumerate() {
        let on = deck(&w, &site, board_x(k), 0.0) - up * (BOARD * 0.5 - 0.002);
        place(&mut w, *id, on, site.along, up);
    }
    // Nailed where each board crosses each stringer, down through the board into the stringer
    // (the part named first is the one the nail is driven through: from the stringer's side a
    // 100 mm nail reaches 25 mm into the board, under the 8 diameters it needs to hold against
    // pulling, and every such joint let go as the bridge settled). The first board to all
    // three stringers (they become one), each other board to the first, then the loops closed.
    let mut built = boards[0];
    let crossing = |w: &ServerWorld, k: usize, y: f64| deck(w, &site, board_x(k), y) - up * BOARD;
    for (k, board) in boards.iter().enumerate() {
        if k > 0 {
            let at = crossing(&w, k, STRINGERS[0]);
            w.fasten_now((*board, built), at, "nail_100")
                .expect("nailed");
            built = *board;
        }
        for (j, y) in STRINGERS.iter().enumerate() {
            let at = crossing(&w, k, *y);
            if k == 0 {
                w.fasten_now((built, stringers[j]), at, "nail_100")
                    .expect("nailed");
            } else if j > 0 {
                let ways = (site.along, site.across);
                w.nail_within(built, at, ways, "nail_100").expect("nailed");
            }
        }
    }
    w
}

/// The bridge's entity once built: the structure takes the last board's id.
pub fn bridge(site: &BridgeSite) -> Option<EntityId> {
    site.item("deck board", BOARDS - 1)
}

/// Deck board `k`'s place across (m from the start).
fn board_x(k: usize) -> f64 {
    #[allow(clippy::cast_precision_loss)] // twelve
    let k = k as f64;
    FIRST_BOARD + BOARD_EVERY * k
}

/// The ground under a stringer's end on the near (`side` −1) or far (1) bank, `y` m along.
/// It rests on the highest ground under its bearing (from the rim to its end, across its width),
/// so it is put down on the bank, neither in it nor dropped onto it.
fn bank(w: &ServerWorld, site: &BridgeSite, y: f64, side: f64) -> DVec3 {
    let up = site.middle.normalize();
    let x = OUT + side * (WIDTH * 0.5 + 0.2);
    let ground = w.ground_under(site.at(x, 0.0) + site.along * y);
    let highest = (0..=6)
        .flat_map(|i| [-0.04, 0.0, 0.04].map(|d| (f64::from(i), d)))
        .map(|(i, d)| {
            let at = site.at(OUT + side * (WIDTH * 0.5 + 0.06 * i), 0.0);
            (w.ground_under(at + site.along * (y + d)) - ground).dot(up)
        })
        .fold(0.0, f64::max);
    ground + up * highest
}

/// Body `id` put with its centre at `centre`, its length along `axis`, its height up.
fn place(w: &mut ServerWorld, id: EntityId, centre: DVec3, axis: DVec3, up: DVec3) {
    let Some(i) = w.index(id) else { return };
    let x = axis.normalize();
    let y = up.reject_from(x).normalize();
    let body = &mut w.world.bodies[i];
    body.structure.position[0] = centre;
    body.structure.orientation[0] = DQuat::from_mat3(&DMat3::from_cols(x, y, x.cross(y)));
    body.solver.reset_history(&body.structure);
}

/// The deck's top `x` m across from the start and `y` m along, as laid: the stringers' tops (a
/// straight line from bank to bank) and a board on them.
fn deck(w: &ServerWorld, site: &BridgeSite, x: f64, y: f64) -> DVec3 {
    let up = site.middle.normalize();
    let (near, far) = (bank(w, site, y, -1.0), bank(w, site, y, 1.0));
    let t = (x - (OUT - WIDTH * 0.5 - 0.2)) / (WIDTH + 0.4);
    near.lerp(far, t) + up * (STRINGER_DEPTH + BOARD)
}

/// The two players' acts this tick.
pub fn bridge_demo(tick: u32, play: &Play) -> Vec<Act> {
    let site = BridgeSite::of(&play.server.world, 1);
    (0..play.players())
        .map(|k| {
            let me = play.avatar(k);
            let side = if k == 0 { 0.25 } else { -0.25 };
            if tick < OVER {
                Act::default()
            } else if tick < CARRY {
                // Over to the far bank and back.
                let x = if tick < OVER + 240 { OUT + 2.0 } else { 1.5 };
                Act::walk(towards(me, site.at(x, side), 0.2))
            } else if tick < BLOCKS {
                carrying(play, &site, k, tick - CARRY)
            } else if k == 0 {
                setting(play, &site, tick)
            } else {
                Act::default()
            }
        })
        .collect()
}

/// Player `k` carrying their two granite blocks from the pile onto the deck, one each
/// [`PER_CARRY`]: to the block, taken; to the deck's middle, let go.
fn carrying(play: &Play, site: &BridgeSite, k: usize, t: u32) -> Act {
    let me = play.avatar(k);
    let j = (t / PER_CARRY) as usize;
    let Some(block) = site.item("granite block", 4 * k + j) else {
        return Act::default();
    };
    let w = &play.server.world;
    let into = t % PER_CARRY;
    if into < 200 {
        let Some(at) = w
            .index(block)
            .map(|i| w.world.bodies[i].structure.position[0])
        else {
            return Act::default();
        };
        let mut act = Act::walk(towards(me, at, 0.55));
        let up = at.normalize();
        let aimed = (play.client(k).aim(at + up * 3.0, -up, 5.0)).filter(|a| a.body == Some(block));
        if into == 180
            && apart(me, at) < 1.0
            && let Some(a) = aimed
        {
            act.commands.push(Command::Grab(Target {
                body: a.body,
                at: a.point,
                seen: a.seen,
            }));
        }
        act
    } else {
        let side = if k == 0 { 0.25 } else { -0.25 };
        #[allow(clippy::cast_precision_loss)]
        let x = OUT - 0.9 + 0.3 * j as f64;
        let mut act = Act::walk(towards(me, deck(w, site, x, side), 0.1));
        if into == PER_CARRY - 20 {
            act.commands.push(Command::Drop);
        }
        act
    }
}

/// Ada setting loads from the kit: twelve granite blocks along the deck's middle, then the test
/// weights on its middle.
fn setting(play: &Play, site: &BridgeSite, tick: u32) -> Act {
    let w = &play.server.world;
    let mut act = Act::default();
    let kit = |name: &str| BridgeSite::kit_index(&w.data, name);
    if tick < WEIGHTS {
        if (tick - BLOCKS).is_multiple_of(PER_BLOCK) {
            let n = (tick - BLOCKS) / PER_BLOCK;
            let x = OUT - 0.75 + 0.5 * f64::from(n % 4);
            let y = 0.3 * (f64::from(n / 4) - 1.0);
            if let Some(item) = kit("granite block") {
                let at = deck(w, site, x, y);
                act.commands.push(Command::Kit {
                    item,
                    at: at + at.normalize() * 0.09,
                });
            }
        }
    } else if (tick - WEIGHTS).is_multiple_of(PER_WEIGHT)
        && let Some(item) = kit("test weight")
    {
        let at = deck(w, site, OUT, 0.0);
        act.commands.push(Command::Kit {
            item,
            at: at + at.normalize() * 0.31,
        });
    }
    act
}

#[cfg(test)]
#[path = "bridge_demo_tests.rs"]
mod tests;
