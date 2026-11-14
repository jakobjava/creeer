//! The crosshair and what it tells (G2.11 A3, ADR-0038): what it is on, named as a person would
//! (`hud_seen.rs`), whether it is in reach, and what can be done with it now, each with its key
//! from the controls' table; and, low in the middle by the hands, what can be done with what
//! they hold. Only what a person on site would know (plan.md, principle 13).
//!
//! Where the plate of what the crosshair is on goes is the player's setting (G2.11 Q): at the
//! bottom, above what the hands can do, so the screen's centre stays the world's (`art-style.md`
//! §10), or beside the crosshair. The crosshair itself rings in the highlight wherever the plate
//! is, when something can be done.

use crate::bindings::Bindings;
use crate::prompts::{Prompt, Prompts};
use creeer_ui::egui::{self, Align2, Color32, pos2, vec2};
use creeer_ui::paint::{self, Cross, Row};
use creeer_ui::theme::{BODY, TEXT, WARN};

/// The name's size, a little above the prompts'.
const NAME: f32 = 17.0;
/// The room the bottom corners' panels take (the hands' at the right, the vitals' at the left),
/// from each side of the screen (points): the prompts at the bottom stay between them.
pub(crate) const CORNER: f32 = crate::hud_hands::WIDE + 32.0;
/// The gap above the screen's bottom and between the bottom's plates (points).
const EDGE: f32 = 16.0;
const GAP: f32 = 8.0;

/// Draws the crosshair, the plate of what it is on (`beside` it, else at the bottom) and the
/// hands' strips.
pub(crate) fn draw(
    ctx: &egui::Context,
    name: Option<&str>,
    p: &Prompts,
    (keys, beside): (&Bindings, bool),
) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("crosshair"),
    ));
    let screen = ctx.content_rect();
    let centre = screen.center();
    let cross = if !p.aimed.is_empty() {
        Cross::Can
    } else if p.state.iter().any(|s| s == "too far") {
        Cross::Far
    } else {
        Cross::Plain
    };
    paint::crosshair(&painter, centre, cross);
    // What the hands can do, at the bottom between the corners' panels.
    let most = (screen.width() - 2.0 * CORNER).max(240.0);
    let names: Vec<String> = p.hands.iter().map(|x| keys.name(x.action)).collect();
    let hands: Vec<Row<'_>> = prompt_rows(&p.hands, &names).collect();
    let bottom = pos2(centre.x, screen.bottom() - EDGE);
    let taken = paint::strips(&painter, bottom, &hands, most);
    // What the crosshair is on.
    let names: Vec<String> = p.aimed.iter().map(|x| keys.name(x.action)).collect();
    let mut rows: Vec<Row<'_>> = Vec::new();
    if let Some(name) = name {
        rows.push(Row {
            key: None,
            text: name,
            colour: TEXT,
            size: NAME,
        });
    }
    rows.extend(p.state.iter().map(|s| Row {
        key: None,
        text: s,
        colour: WARN,
        size: BODY,
    }));
    rows.extend(prompt_rows(&p.aimed, &names));
    if rows.is_empty() {
        return;
    }
    let at = if beside {
        (Align2::LEFT_TOP, centre + vec2(26.0, 18.0))
    } else {
        let above = if hands.is_empty() {
            bottom.y
        } else {
            taken.top() - GAP
        };
        (Align2::CENTER_BOTTOM, pos2(centre.x, above))
    };
    paint::plate_at(&painter, at, &rows);
}

/// Each prompt as a row: its key cap and its verb.
fn prompt_rows<'a>(prompts: &'a [Prompt], keys: &'a [String]) -> impl Iterator<Item = Row<'a>> {
    prompts.iter().zip(keys).map(|(p, k)| Row {
        key: Some(k),
        text: &p.verb,
        colour: if k == "unbound" {
            Color32::from_gray(140)
        } else {
            TEXT
        },
        size: BODY,
    })
}
