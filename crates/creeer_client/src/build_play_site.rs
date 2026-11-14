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
/// Where the player stands to lay a stringer across (m east of the start): short of the rim and
/// near enough to slide its middle out over the gully's within reach; where it is rested on the
/// bank first, and how many notches of the wheel slide it out (5 cm each: its middle 4.0 m east);
/// and where the player stands to lay the deck, short of the stringers' ends.
const BANK: f64 = 2.2;
const REST: f64 = 2.7;
const SLID: u32 = 26;
const DECKING: f64 = 2.0;

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

/// Stringer `k`'s line as it lies (a point on it and its way), before anything is nailed to it.
fn line(p: &Player, k: usize) -> Result<(DVec3, DVec3), String> {
    let id = item(p, "stringer", k)?;
    let c = &p.view.session.connection;
    let pose = (c.poses().get(&id).copied()).ok_or(format!("no stringer {k}"))?;
    Ok((pose.position, pose.orientation * DVec3::X))
}

/// The top of what lies on line `(at, along)` nearest `near` (the stringer, or a board on it), as
/// the eye finds it: a ray down onto it there.
fn top_of(p: &Player, (at, along): (DVec3, DVec3), near: DVec3) -> DVec3 {
    let up = p.up();
    let on = at + along * (near - at).dot(along);
    let c = &p.view.session.connection;
    let held = c.own_load();
    (c.aim_skipping(on + up * 1.0, -up, 2.5, held)).map_or(on, |a| a.point)
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

/// A frame with what the crosshair is on told at the bottom (the default), then the same told
/// beside the crosshair (the settings' other choice), for the user to choose between.
fn both_places(p: &mut Player, step: &str) -> Result<(), String> {
    p.shot(step)?;
    p.game.prompts_beside = true;
    let shot = p.shot(&format!("{step}-beside"));
    p.game.prompts_beside = false;
    shot
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
    both_places(p, "look-at-the-timber")?;
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
    if !in_the_gully(p, start) {
        look(p, at(p, 4.0, 0.4), 1.0);
        p.shot("three-stringers")?;
        boards(p, start)?;
    }
    if in_the_gully(p, start) {
        // (Down there she cannot climb out: the rest is played on the built bridge,
        // `site-walk` and `site-load`.)
        p.note("fell into the gully: the rest of the ten minutes on the built bridge");
    } else {
        onto_the_deck(p, start, LAID)?;
        load(p, start, LAID)?;
    }
    menus(p)
}

/// Whether the player is down in the gully (half a metre below where she started).
fn in_the_gully(p: &Player, start: DVec3) -> bool {
    (p.me() - start).dot(p.up()) < -0.5
}

/// The three stringers, each taken from its row, carried to the near bank, turned along the
/// view (Q with the mouse), aimed at the far rim and into the gap (both beyond reach: it stays
/// before the chest), then rested on the bank before the feet and slid out along itself over the
/// gully with the wheel until it lies on both banks (ADR-0036: the wheel lays a beam from one
/// support to the next), let go.
fn stringers(p: &mut Player, start: DVec3) -> Result<(), String> {
    for (k, n) in STRINGERS.into_iter().enumerate() {
        let id = item(p, "stringer", k)?;
        // Tried again, as a player would, if it did not come to lie across.
        for attempt in 1..=2 {
            let name = match attempt {
                1 => format!("stringer{}", k + 1),
                _ => format!("stringer{}-again", k + 1),
            };
            let lies = stringer(p, start, (id, n), &name)?;
            p.note(&format!(
                "{name}: its middle {lies:.2} m east (the gully's 4.0)"
            ));
            if (lies - 4.0).abs() < 0.4 || in_the_gully(p, start) {
                break;
            }
        }
        if in_the_gully(p, start) {
            break;
        }
    }
    Ok(())
}

/// Stringer `id` laid across at `n` m north (`stringers`); where its middle lies after (m east).
fn stringer(
    p: &mut Player,
    start: DVec3,
    (id, n): (EntityId, f64),
    name: &str,
) -> Result<f64, String> {
    let at = |p: &Player, e: f64, n: f64| p.ground_at(start, e, n);
    p.walk_to(now(p, id), 1.3, 1.0);
    take(p, id, name)?;
    p.walk_to(at(p, 1.0, n), 0.15, 0.0);
    p.shot(&format!("{name}-carried"))?;
    p.walk_to(at(p, BANK, n), 0.15, 0.0);
    look(p, at(p, 4.0, n) + p.up() * 0.5, 0.6);
    p.shot(&format!("{name}-at-the-bank"))?;
    p.turn_to(FRAC_PI_2);
    p.play(0.8, DVec3::ZERO);
    p.shot(&format!("{name}-turned-along"))?;
    for (where_, e) in [("far-rim", 5.15), ("gap", 4.0)] {
        look(p, at(p, e, n), 1.2);
        p.shot(&format!("{name}-aimed-at-the-{where_}"))?;
    }
    look(p, at(p, REST, n), 1.2);
    p.shot(&format!("{name}-rested-on-the-bank"))?;
    for _ in 0..SLID {
        p.view.holding.wheel(1.0);
        p.play(0.03, DVec3::ZERO);
    }
    p.play(1.0, DVec3::ZERO);
    p.shot(&format!("{name}-slid-out-across"))?;
    p.key(KeyCode::KeyG);
    p.play(0.5, DVec3::ZERO);
    p.shot(&format!("{name}-let-go"))?;
    p.play(2.0, DVec3::ZERO);
    p.shot(&format!("{name}-2s-on"))?;
    Ok((now(p, id) - start).dot(p.east))
}

/// Deck boards, each taken from its row, laid across the stringers and nailed (J where it rests
/// on the middle stringer, then J at the next), let go.
fn boards(p: &mut Player, start: DVec3) -> Result<(), String> {
    let at = |p: &Player, e: f64, n: f64| p.ground_at(start, e, n);
    let up = p.up();
    let lines = [line(p, 0)?, line(p, 1)?];
    for (k, e) in BOARDS.into_iter().enumerate() {
        if in_the_gully(p, start) {
            break;
        }
        let name = format!("board{}", k + 1);
        let id = item(p, "deck board", k)?;
        // To the boards' row's end, between the handrail posts and the sleepers (walked up to
        // straight, she stood on a sleeper, and the board taken from under her stood up on end).
        p.walk_to(at(p, -1.0, 2.0), 0.15, 1.0);
        take(p, id, &name)?;
        // Back to the bank, short of the stringers' ends lying on it (walked into, they trip
        // her up), coming at them square.
        p.walk_to(at(p, 1.0, 0.4), 0.3, 0.0);
        p.walk_to(at(p, DECKING, 0.4), 0.15, 0.0);
        p.turn_to(0.0);
        // On the middle stringer's top, where the eye finds it (as laid, not where planned).
        let middle = top_of(p, lines[1], at(p, e, 0.4));
        look(p, middle, 1.5);
        both_places(p, &format!("{name}-laid-across"))?;
        p.key(KeyCode::KeyJ);
        p.play(0.4, DVec3::ZERO);
        p.shot(&format!("{name}-j-pressed"))?;
        p.play(2.0, DVec3::ZERO);
        p.shot(&format!("{name}-nailed"))?;
        let first = top_of(p, lines[0], at(p, e, 0.0));
        look(p, first + up * 0.05, 0.6);
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

/// A deck to walk onto and load: its line (m north of the start), where to step onto it and
/// where its loads go (m east), and where the player stands to load it.
#[derive(Clone, Copy)]
struct Deck {
    n: f64,
    steps: [f64; 3],
    loads: [f64; 2],
    stand: f64,
}

/// The deck laid by hand (`boards`), and *bridge_demo*'s built footbridge (ADR-0039: three
/// stringers 0.4 m apart, twelve boards from 2.6 m to 5.4 m east).
const LAID: Deck = Deck {
    n: 0.4,
    steps: [3.0, 3.2, 3.4],
    loads: [3.2, 3.2],
    stand: DECKING,
};
const BUILT: Deck = Deck {
    n: 0.0,
    steps: [3.0, 3.6, 4.2],
    loads: [3.4, 4.0],
    stand: 2.2,
};

/// Walks onto the deck, a frame at each step and on after, then back.
fn onto_the_deck(p: &mut Player, start: DVec3, deck: Deck) -> Result<(), String> {
    let at = |p: &Player, e: f64, n: f64| p.ground_at(start, e, n);
    p.walk_to(at(p, deck.stand, deck.n), 0.15, 0.5);
    look(p, at(p, deck.steps[2], deck.n), 0.5);
    p.shot("before-stepping-on")?;
    for (k, e) in deck.steps.into_iter().enumerate() {
        p.walk_to(at(p, e, deck.n), 0.15, 0.5);
        p.shot(&format!("on-the-deck-{}", k + 1))?;
    }
    for s in [2, 5] {
        p.play(f64::from(s), DVec3::ZERO);
        p.shot(&format!("on-the-deck-{s}s-on"))?;
    }
    p.walk_to(at(p, deck.stand - 0.4, deck.n), 0.15, 0.5);
    p.shot("back-on-the-bank")
}

/// A granite block carried onto the deck; the overlay (O); then the deck loaded from the kit
/// until it breaks, and what failed read.
fn load(p: &mut Player, start: DVec3, deck: Deck) -> Result<(), String> {
    let at = |p: &Player, e: f64, n: f64| p.ground_at(start, e, n);
    let up = p.up();
    let block = item(p, "granite block", 3)?;
    p.walk_to(now(p, block), 1.2, 1.0);
    take(p, block, "block")?;
    p.walk_to(at(p, 1.0, deck.n), 0.3, 0.0);
    p.walk_to(at(p, deck.stand, deck.n), 0.15, 0.0);
    look(p, at(p, deck.loads[0], deck.n) + up * 0.2, 1.5);
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
    look(p, at(p, deck.loads[1], deck.n) + up * 0.2, 0.5);
    p.key(KeyCode::KeyC);
    p.play(0.3, DVec3::ZERO);
    p.shot("kit-at-the-deck")?;
    for k in 1..=3 {
        put_down(p, "test weight")?;
        p.play(3.0, DVec3::ZERO);
        p.shot(&format!("test-weight-{k}"))?;
    }
    p.key(KeyCode::KeyC);
    look(p, at(p, 4.0, deck.n), 2.0);
    p.shot("what-failed")?;
    p.play(5.0, DVec3::ZERO);
    p.shot("what-failed-5s-on")
}

/// On *bridge_demo*'s footbridge, built across the gully (`--build-play site-walk|site-load`):
/// walked onto (`walk`), or loaded until it breaks; then the menus.
///
/// # Errors
/// If a step cannot be played or a frame saved.
pub(crate) fn on_the_built_bridge(p: &mut Player, walk: bool) -> Result<(), String> {
    let start = p.me();
    let up = p.up();
    let at = |p: &Player, e: f64, n: f64| p.ground_at(start, e, n);
    look(p, at(p, 5.15, 0.0) + up * 0.3, 1.0);
    p.shot("start")?;
    look(p, at(p, 4.0, 0.0), 1.0);
    p.shot("the-bridge")?;
    if walk {
        onto_the_deck(p, start, BUILT)?;
    } else {
        load(p, start, BUILT)?;
    }
    menus(p)
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
