//! The crosshair and what it tells (G2.11 A3, ADR-0038): what it is on, named as a person would
//! (`hud_seen.rs`), whether it is in reach, and what can be done with it now, each with its key
//! from the controls' table; and, low in the middle by the hands, what can be done with what
//! they hold. Only what a person on site would know (plan.md, principle 13).

use crate::bindings::Bindings;
use crate::prompts::{Prompt, Prompts};
use creeer_ui::egui::{self, Color32, pos2, vec2};
use creeer_ui::paint::{self, Cross, Row};
use creeer_ui::theme::{BODY, TEXT, WARN};

/// The name's size, a little above the prompts'.
const NAME: f32 = 17.0;

/// Draws the crosshair, the plate beside it and the hands' strip.
pub(crate) fn draw(ctx: &egui::Context, name: Option<&str>, p: &Prompts, keys: &Bindings) {
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
    if !rows.is_empty() {
        paint::plate(&painter, centre + vec2(26.0, 18.0), &rows);
    }
    let names: Vec<String> = p.hands.iter().map(|x| keys.name(x.action)).collect();
    let rows: Vec<Row<'_>> = prompt_rows(&p.hands, &names).collect();
    if !rows.is_empty() {
        paint::strip(&painter, pos2(centre.x, screen.bottom() - 16.0), &rows);
    }
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
