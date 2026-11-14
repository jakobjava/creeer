//! The user's first ten minutes at the bridge site, played as a player plays them (G2.11 Q, the
//! play-through; `docs/playability-plan.md`): `creeer_client --build-play site <dir>`.
//!
//! The game started as `--bridge` starts it; the player looks about, opens the kit and the
//! inventory, takes the three stringers from their row and lays them across the gully, takes deck
//! boards and nails them on, walks onto the deck, carries a granite block onto it, turns the
//! stress overlay on (O), loads the deck from the kit until it breaks, reads what failed, and
//! opens the menu, the controls and the settings. A frame at every step shows the player's whole
//! screen (`build_play_shot.rs`), and `steps.txt` says what the screen said and what was heard.

use crate::bindings::Input;
use crate::build_play::Player;
use crate::menu::Page;
use creeer_math::glam::DVec3;
use creeer_net::entity::EntityId;
use std::f64::consts::FRAC_PI_2;
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

/// Where a stringer is laid across the gully (m north of the start), and where deck boards are
/// laid on them (m east of the start: the near rim is 2.85 m east, the far one 5.15).
const STRINGERS: [f64; 3] = [0.0, 0.4, 0.8];
const BOARDS: [f64; 3] = [3.0, 3.2, 3.4];
/// Where the player stands to lay things across (m east of the start): a stride short of the rim.
const BANK: f64 = 2.4;

/// Looks at `at` for `seconds`.
fn look(p: &mut Player, at: DVec3, seconds: f64) {
    p.look = at;
    p.play(seconds, DVec3::ZERO);
}

/// Where `id` is drawn now.
fn now(p: &Player, id: EntityId) -> DVec3 {
    let c = &p.view.session.connection;
    c.poses().get(&id).map_or(DVec3::ZERO, |pose| pose.position)
}

/// Takes `id` where it lies now (looking at it, G), a frame before and after.
fn take(p: &mut Player, id: EntityId, name: &str) -> Result<(), String> {
    let at = now(p, id);
    look(p, at, 0.5);
    p.shot(&format!("{name}-looked-at"))?;
    p.key(KeyCode::KeyG);
    p.play(0.8, DVec3::ZERO);
    p.shot(&format!("{name}-taken"))
}

/// The site's `name` item `k`.
fn item(p: &Player, name: &str, k: usize) -> Result<EntityId, String> {
    (p.site.as_ref())
        .and_then(|s| s.item(name, k))
        .ok_or(format!("no {name} {k} at the site"))
}

/// The kit's part `name` put down with the palette's button (C open), before the player.
fn put_down(p: &mut Player, name: &str) -> Result<(), String> {
    let k = (crate::kit_ui::parts().iter().position(|k| k.name == name))
        .ok_or(format!("no {name} in the kit"))?;
    let item = u8::try_from(k).map_err(|e| e.to_string())?;
    let camera = p.camera;
    crate::kit_ui::put_down(&mut p.view, &camera, item);
    Ok(())
}

/// The menu's page `page` opened, as its button does (the controls seen once: the first start's
/// hint goes, as `app/menu.rs` has it).
fn page(p: &mut Player, page: Page) {
    p.screen.menu = Some(page);
    if page == Page::Controls {
        p.game.controls_seen = true;
    }
}

/// The ten minutes.
///
/// # Errors
/// If a step cannot be played or a frame saved.
pub(crate) fn ten_minutes(p: &mut Player) -> Result<(), String> {
    let start = p.me();
    let up = p.up();
    let at = |p: &Player, e: f64, n: f64| p.ground_at(start, e, n);
    // As the game starts: looking across the gully at its far rim.
    look(p, at(p, 5.15, 0.0) + up * 0.3, 1.0);
    p.shot("start")?;
    look(p, at(p, 0.3, 2.4), 1.0);
    p.shot("look-at-the-timber")?;
    look(p, at(p, 0.0, -2.3), 1.0);
    p.shot("look-at-the-loads")?;
    look(p, at(p, 4.0, 0.0) - up * 1.5, 1.0);
    p.shot("look-into-the-gully")?;
    // What is carried (I), and the kit (C): a deck board put down from it.
    p.key(KeyCode::KeyI);
    p.play(0.3, DVec3::ZERO);
    p.shot("inventory")?;
    p.key(KeyCode::KeyI);
    look(p, at(p, -1.0, 0.0), 0.5);
    p.key(KeyCode::KeyC);
    p.play(0.3, DVec3::ZERO);
    p.shot("kit-open")?;
    put_down(p, "deck board")?;
    p.play(1.5, DVec3::ZERO);
    p.shot("kit-deck-board-put-down")?;
    p.key(KeyCode::Escape);
    p.play(0.3, DVec3::ZERO);
    p.shot("kit-closed")?;
    stringers(p, start)?;
    look(p, at(p, 4.0, 0.4), 1.0);
    p.shot("three-stringers")?;
    boards(p, start)?;
    onto_the_deck(p, start)?;
    load(p, start)?;
    menus(p)
}

/// The three stringers, each taken from its row, carried to the near bank, turned along the
/// view (Q with the mouse) and laid across the gully.
fn stringers(p: &mut Player, start: DVec3) -> Result<(), String> {
    let at = |p: &Player, e: f64, n: f64| p.ground_at(start, e, n);
    for (k, n) in STRINGERS.into_iter().enumerate() {
        let name = format!("stringer{}", k + 1);
        let id = item(p, "stringer", k)?;
        p.walk_to(now(p, id), 1.3, 1.0);
        take(p, id, &name)?;
        p.walk_to(at(p, 1.0, n), 0.15, 0.0);
        p.shot(&format!("{name}-carried"))?;
        p.walk_to(at(p, BANK, n), 0.15, 0.0);
        look(p, at(p, 4.0, n) + p.up() * 0.5, 0.6);
        p.shot(&format!("{name}-at-the-bank"))?;
        p.turn_to(FRAC_PI_2);
        p.play(0.8, DVec3::ZERO);
        p.shot(&format!("{name}-turned-along"))?;
        for (where_, e) in [("far-rim", 5.15), ("gap", 4.0), ("near-rim", 2.9)] {
            look(p, at(p, e, n), 1.2);
            p.shot(&format!("{name}-aimed-at-the-{where_}"))?;
        }
        look(p, at(p, 4.0, n), 1.2);
        p.key(KeyCode::KeyG);
        p.play(0.5, DVec3::ZERO);
        p.shot(&format!("{name}-let-go"))?;
        p.play(2.0, DVec3::ZERO);
        p.shot(&format!("{name}-2s-on"))?;
    }
    Ok(())
}

/// Deck boards, each taken from its row, laid across the stringers and nailed (J where it rests
/// on the middle stringer, then J at the next), let go.
fn boards(p: &mut Player, start: DVec3) -> Result<(), String> {
    let at = |p: &Player, e: f64, n: f64| p.ground_at(start, e, n);
    let up = p.up();
    for (k, e) in BOARDS.into_iter().enumerate() {
        let name = format!("board{}", k + 1);
        let id = item(p, "deck board", k)?;
        p.walk_to(now(p, id), 1.3, 1.0);
        take(p, id, &name)?;
        p.walk_to(at(p, BANK, 0.4), 0.15, 0.0);
        p.turn_to(0.0);
        look(p, at(p, e, 0.4) + up * 0.15, 1.5);
        p.shot(&format!("{name}-laid-across"))?;
        p.key(KeyCode::KeyJ);
        p.play(0.4, DVec3::ZERO);
        p.shot(&format!("{name}-j-pressed"))?;
        p.play(2.0, DVec3::ZERO);
        p.shot(&format!("{name}-nailed"))?;
        look(p, at(p, e, 0.0) + up * 0.2, 0.6);
        p.shot(&format!("{name}-aimed-at-crossing2"))?;
        p.key(KeyCode::KeyJ);
        p.play(0.4, DVec3::ZERO);
        p.shot(&format!("{name}-j-at-crossing2"))?;
        p.play(2.0, DVec3::ZERO);
        p.shot(&format!("{name}-after-crossing2"))?;
        if p.view.holding.holding() {
            p.key(KeyCode::KeyG);
            p.play(0.5, DVec3::ZERO);
            p.shot(&format!("{name}-let-go"))?;
        }
    }
    look(p, at(p, 3.6, 0.4), 1.0);
    p.shot("the-deck")
}

/// Walks onto the deck, a frame every few steps.
fn onto_the_deck(p: &mut Player, start: DVec3) -> Result<(), String> {
    let at = |p: &Player, e: f64, n: f64| p.ground_at(start, e, n);
    look(p, at(p, 3.4, 0.4), 0.5);
    p.shot("before-stepping-on")?;
    for (k, e) in [3.0, 3.3].into_iter().enumerate() {
        p.walk_to(at(p, e, 0.4), 0.15, 0.5);
        p.shot(&format!("on-the-deck-{}", k + 1))?;
    }
    p.play(2.0, DVec3::ZERO);
    p.shot("on-the-deck-2s")?;
    p.walk_to(at(p, BANK - 0.4, 0.4), 0.15, 0.5);
    p.shot("back-on-the-bank")
}

/// A granite block carried onto the deck; the overlay (O); then the deck loaded from the kit
/// until it breaks, and what failed read.
fn load(p: &mut Player, start: DVec3) -> Result<(), String> {
    let at = |p: &Player, e: f64, n: f64| p.ground_at(start, e, n);
    let up = p.up();
    let block = item(p, "granite block", 3)?;
    p.walk_to(now(p, block), 1.2, 1.0);
    take(p, block, "block")?;
    p.walk_to(at(p, BANK, 0.4), 0.15, 0.0);
    look(p, at(p, 3.2, 0.4) + up * 0.2, 1.5);
    p.shot("block-over-the-deck")?;
    p.key(KeyCode::KeyG);
    p.play(1.0, DVec3::ZERO);
    p.shot("block-put-on-the-deck")?;
    p.key(KeyCode::KeyO);
    p.play(0.5, DVec3::ZERO);
    p.shot("overlay-on")?;
    p.press(Input::Mouse(MouseButton::Left));
    p.play(0.5, DVec3::ZERO);
    p.shot("clicked-at-the-deck")?;
    p.key(KeyCode::KeyC);
    for k in 1..=3 {
        put_down(p, "test weight")?;
        p.play(3.0, DVec3::ZERO);
        p.shot(&format!("test-weight-{k}"))?;
    }
    p.key(KeyCode::KeyC);
    look(p, at(p, 4.0, 0.4), 2.0);
    p.shot("what-failed")
}

/// The menu (Esc), its controls and settings, and back to the game.
fn menus(p: &mut Player) -> Result<(), String> {
    p.key(KeyCode::Escape);
    p.play(0.3, DVec3::ZERO);
    p.shot("menu")?;
    page(p, Page::Controls);
    p.shot("menu-controls")?;
    page(p, Page::Settings);
    p.shot("menu-settings")?;
    p.key(KeyCode::Escape);
    p.key(KeyCode::Escape);
    p.play(0.5, DVec3::ZERO);
    p.shot("back-in-the-game")
}
