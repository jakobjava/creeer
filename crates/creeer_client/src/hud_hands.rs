//! What the player has, at a glance (G2.11 A3, ADR-0038): at the bottom right, what the hands
//! hold, the tool in hand, and how full each pocket is, with the key that shows all of it (I).
//! The inventory window (`inventory_ui.rs`) has the rest.

use crate::bindings::{Action, Bindings};
use creeer_net::message::Inventory;
use creeer_ui::egui::{self, Align2, Color32, FontId, Rect, pos2, vec2};
use creeer_ui::paint;
use creeer_ui::theme::{BODY, HIGHLIGHT, SMALL, TEXT, TEXT_DIM};

/// A pocket's name as the player would say it: "thigh, left (coverall)" for
/// `coverall.thigh.L`.
pub(crate) fn pocket_name(id: &str) -> String {
    let Some((wearable, pocket)) = id.split_once('.') else {
        return id.replace('_', " ");
    };
    let (pocket, side) = match pocket.rsplit_once('.') {
        Some((p, "L")) => (p, ", left"),
        Some((p, "R")) => (p, ", right"),
        _ => (pocket, ""),
    };
    format!(
        "{}{side} ({})",
        pocket.replace('_', " "),
        wearable.replace('_', " ")
    )
}

/// A pocket's fill: the room its things take of its volume (0 to 1).
pub(crate) fn fill(pocket: &creeer_net::message::StoreView) -> f32 {
    let used: f64 = pocket.items.iter().map(|i| i.room).sum();
    #[allow(clippy::cast_possible_truncation)] // a share
    let share = (used / pocket.volume.max(1e-9)) as f32;
    share.clamp(0.0, 1.0)
}

/// The panel's width (points).
const WIDE: f32 = 300.0;

/// Draws the panel at the bottom right: `held` names what the hands hold (`hud_seen::held_name`).
pub(crate) fn draw(
    ctx: &egui::Context,
    (inventory, held): (&Inventory, Option<&str>),
    tool: Option<&str>,
    keys: &Bindings,
) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("hands"),
    ));
    let line = BODY + 6.0;
    let rows = 2 + usize::from(tool.is_some()) + inventory.pockets.len().div_ceil(2);
    #[allow(clippy::cast_precision_loss)] // a few rows
    let tall = line * rows as f32 + 16.0;
    let screen = ctx.content_rect();
    let rect = Rect::from_min_size(
        screen.right_bottom() - vec2(WIDE + 16.0, tall + 16.0),
        vec2(WIDE, tall),
    );
    paint::raised(
        &painter,
        rect,
        Color32::from_rgba_premultiplied(41, 38, 36, 225),
    );
    let left = rect.left() + 10.0;
    let mut y = rect.top() + 8.0 + line * 0.5;
    let text = |at: egui::Pos2, s: &str, size: f32, colour: Color32| {
        painter.text(
            at,
            Align2::LEFT_CENTER,
            s,
            FontId::proportional(size),
            colour,
        )
    };
    let word = held.unwrap_or("nothing");
    let label = text(pos2(left, y), "In your hands", SMALL, TEXT_DIM);
    text(pos2(label.right() + 10.0, y), word, BODY, TEXT);
    y += line;
    if let Some(tool) = tool {
        let label = text(pos2(left, y), "Tool in hand", SMALL, TEXT_DIM);
        text(pos2(label.right() + 10.0, y), tool, BODY, TEXT);
        y += line;
    }
    // The pockets two to a row: a bar each, its name's first word beside it.
    for pair in inventory.pockets.chunks(2) {
        for (k, pocket) in pair.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)] // 0 or 1
            let x = left + k as f32 * (WIDE - 20.0) * 0.5;
            let bar = Rect::from_min_size(pos2(x, y - 4.0), vec2(36.0, 9.0));
            let colour = if fill(pocket) >= 0.95 {
                HIGHLIGHT
            } else {
                TEXT_DIM
            };
            paint::bar(&painter, bar, fill(pocket), colour);
            let name = pocket_name(&pocket.name);
            let short = name.split(" (").next().unwrap_or(&name);
            text(pos2(bar.right() + 6.0, y), short, SMALL, TEXT_DIM);
        }
        y += line;
    }
    let key = keys.name(Action::Inventory);
    let cap = paint::keycap(&painter, pos2(left, y), &key, SMALL);
    text(pos2(cap.right() + 8.0, y), "What you carry", SMALL, TEXT);
}

#[cfg(test)]
mod tests {
    use super::*;
    use creeer_net::message::{ItemView, StoreView};

    #[test]
    fn pockets_are_named_as_a_person_would() {
        assert_eq!(pocket_name("coverall.thigh.L"), "thigh, left (coverall)");
        assert_eq!(pocket_name("jacket.chest.R"), "chest, right (jacket)");
        assert_eq!(pocket_name("belt.pouch"), "pouch (belt)");
        assert_eq!(pocket_name("knapsack"), "knapsack");
    }

    #[test]
    fn a_pockets_fill_is_the_room_its_things_take() {
        let item = ItemView {
            text: String::new(),
            word: "stone".into(),
            colour: None,
            mass: 0.04,
            room: 0.0001,
            swing: 0.0,
        };
        let pocket = StoreView {
            name: "belt.pouch".into(),
            volume: 0.0004,
            items: vec![item.clone(), item],
        };
        assert!((fill(&pocket) - 0.5).abs() < 1e-6);
        let empty = StoreView::default();
        assert!(fill(&empty).abs() < 1e-6);
    }
}
