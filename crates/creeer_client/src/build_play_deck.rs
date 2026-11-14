//! The footbridge's deck built by hand at the bridge site (G2.11 B2, ADR-0039's site and kit):
//! `creeer_client --build-play deck <dir>`. Three stringers taken from their row, each turned
//! along the view and laid across the gully on edge; then deck boards taken from theirs, laid
//! across the stringers from the near bank and nailed where they cross each, two nails a
//! crossing; then the bridge looked at, and walked onto.

use crate::build_play::Player;
use creeer_math::glam::DVec3;
use creeer_net::entity::EntityId;
use creeer_server::world::site::{BridgeSite, OUT, WIDTH};
use std::f64::consts::FRAC_PI_2;
use winit::keyboard::KeyCode;

/// The stringers' places along the gully (m north of the start), in the order laid: the middle
/// first, so the next are carried in clear of it.
const STRINGERS: [f64; 3] = [0.0, -0.4, 0.4];
/// Where the player stands to lay and nail: this far short of the near rim (m).
const SHORT_OF_RIM: f64 = 0.6;
/// The deck boards laid from the near bank (within the placing reach), their first place
/// across (m east of the start) and their spacing (BTCV: four to the metre).
const BOARDS: usize = 6;
const FIRST_BOARD: f64 = OUT - 1.375;
const BOARD_EVERY: f64 = 0.25;
/// A stringer's height and a board's thickness (m: `data/kit.ron`).
const STRINGER: f64 = 0.15;
const BOARD: f64 = 0.05;
/// The second nail at a crossing this far along the stringer from the first (m).
const BESIDE: f64 = 0.05;

/// Where `id` is drawn now.
fn now(p: &Player, id: EntityId) -> DVec3 {
    let c = &p.view.session.connection;
    c.poses().get(&id).map_or(DVec3::ZERO, |pose| pose.position)
}

/// Looks at `at` for `seconds`.
fn look(p: &mut Player, at: DVec3, seconds: f64) {
    p.look = at;
    p.play(seconds, DVec3::ZERO);
}

/// Takes `id` where it lies now (G, looking at it), and waits for the hands.
fn take(p: &mut Player, id: EntityId, name: &str, frames: bool) -> Result<(), String> {
    let at = now(p, id);
    look(p, at, 0.4);
    if frames {
        p.shot(&format!("{name}-looked-at"))?;
    }
    p.key(KeyCode::KeyG);
    p.play(0.6, DVec3::ZERO);
    if frames {
        p.shot(&format!("{name}-taken"))?;
    }
    Ok(())
}

/// The top of stringer `id` as drawn, `x` m east of the start (on its axis's line there).
fn stringer_top(p: &Player, id: EntityId, x: f64) -> DVec3 {
    let up = p.up();
    let c = now(p, id);
    let off = (p.start + p.east * x - c).reject_from(up).dot(p.east);
    c + p.east * off + up * (0.5 * STRINGER)
}

/// Plays the deck.
///
/// # Errors
/// If a step cannot be played or a frame saved.
pub(crate) fn deck(p: &mut Player, site: &BridgeSite) -> Result<(), String> {
    let start = p.me();
    let at = |p: &Player, e: f64, n: f64| p.ground_at(start, e, n);
    let rim = OUT - 0.5 * WIDTH;
    look(p, at(p, OUT, 0.0), 0.5);
    p.shot("the-site")?;
    let stringers: Vec<EntityId> = (0..3).filter_map(|k| site.item("stringer", k)).collect();
    for (k, (id, y)) in stringers.iter().zip(STRINGERS).enumerate() {
        let name = format!("stringer{}", k + 1);
        p.walk_to(now(p, *id), 1.3, 1.0);
        take(p, *id, &name, true)?;
        // Turned along the view by the row, then carried to the bank (turned at the bank, its
        // 3 m swept the stringers laid there).
        p.turn_to(FRAC_PI_2);
        p.play(0.6, DVec3::ZERO);
        p.shot(&format!("{name}-turned-along"))?;
        p.walk_to(at(p, rim - 1.6, y), 0.3, 0.3);
        p.walk_to(at(p, rim - SHORT_OF_RIM, y), 0.15, 0.3);
        p.shot(&format!("{name}-at-the-bank"))?;
        look(p, at(p, OUT, y), 1.5);
        p.shot(&format!("{name}-aimed-across"))?;
        p.key(KeyCode::KeyG);
        p.play(0.5, DVec3::ZERO);
        p.shot(&format!("{name}-let-go"))?;
        p.play(2.0, DVec3::ZERO);
        p.shot(&format!("{name}-2s-on"))?;
    }
    look(p, at(p, OUT, 0.0), 0.5);
    p.shot("the-stringers")?;
    let boards: Vec<EntityId> = (0..BOARDS)
        .filter_map(|k| site.item("deck board", k))
        .collect();
    for (k, id) in boards.iter().enumerate() {
        let name = format!("board{}", k + 1);
        // Every frame of the first board's, the last frame of the others'.
        let frames = k == 0;
        p.walk_to(now(p, *id), 1.2, 1.0);
        take(p, *id, &name, frames)?;
        p.walk_to(at(p, rim - 1.6, 0.0), 0.3, 0.3);
        p.walk_to(at(p, rim - SHORT_OF_RIM, 0.0), 0.15, 0.3);
        #[allow(clippy::cast_precision_loss)] // six
        let x = FIRST_BOARD + BOARD_EVERY * k as f64;
        // Across the view (along the gully), aimed at the middle stringer where it goes.
        p.turn_to(0.0);
        look(p, stringer_top(p, stringers[0], x), 1.5);
        if frames {
            p.shot(&format!("{name}-laid-across"))?;
        }
        // Two nails at each crossing: the middle stringer's, then the others'.
        for (j, stringer) in stringers.iter().enumerate() {
            for (n, along) in [0.0, BESIDE].into_iter().enumerate() {
                let top = stringer_top(p, *stringer, x + along) + p.up() * BOARD;
                look(p, top, 0.4);
                if frames {
                    p.shot(&format!("{name}-aimed-crossing{}-nail{}", j + 1, n + 1))?;
                }
                p.key(KeyCode::KeyJ);
                p.play(0.3, DVec3::ZERO);
                if frames && j == 0 && n == 0 {
                    p.shot(&format!("{name}-j-held"))?;
                }
                p.play(2.5, DVec3::ZERO);
                if frames {
                    p.shot(&format!("{name}-after-crossing{}-nail{}", j + 1, n + 1))?;
                }
            }
        }
        if p.view.holding.holding() {
            p.key(KeyCode::KeyG);
        }
        p.play(1.0, DVec3::ZERO);
        p.shot(&format!("{name}-let-go"))?;
    }
    look(p, at(p, OUT, 0.0), 1.0);
    p.shot("the-deck")?;
    // From the side, a few steps south along the bank.
    p.walk_to(at(p, rim - 1.2, -2.5), 0.3, 0.3);
    look(p, at(p, OUT - 0.4, 0.0) + p.up() * 0.2, 1.0);
    p.shot("the-deck-from-the-side")
}
