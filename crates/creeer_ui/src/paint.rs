//! Drawing the HUD's pieces in the game's look (`theme.rs`, `art-style.md` §10): raised panels
//! with a light and a dark edge, keys drawn as key caps, text with a shadow so it reads on a
//! bright sky and a dark ground alike, the crosshair and a ring filling as work goes on.

use crate::theme::{BEVEL_DARK, BEVEL_LIGHT, HIGHLIGHT, INSET, RAISED, TEXT};
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, pos2, vec2};

/// A panel's face with a light edge above and left and a dark one below and right: raised.
pub fn raised(painter: &Painter, rect: Rect, face: Color32) {
    painter.rect_filled(rect, 0.0, face);
    edges(painter, rect, BEVEL_LIGHT, BEVEL_DARK);
}

/// The same with the edges swapped: sunk in (a slot, a bar's channel).
pub fn sunken(painter: &Painter, rect: Rect, face: Color32) {
    painter.rect_filled(rect, 0.0, face);
    edges(painter, rect, BEVEL_DARK, BEVEL_LIGHT);
}

fn edges(painter: &Painter, r: Rect, top_left: Color32, bottom_right: Color32) {
    let (a, b) = (Stroke::new(1.0, top_left), Stroke::new(1.0, bottom_right));
    let r = r.shrink(0.5);
    painter.line_segment([r.left_bottom(), r.left_top()], a);
    painter.line_segment([r.left_top(), r.right_top()], a);
    painter.line_segment([r.right_top(), r.right_bottom()], b);
    painter.line_segment([r.right_bottom(), r.left_bottom()], b);
}

/// Text with a dark shadow a pixel below and right: readable over any part of the world.
pub fn text(
    painter: &Painter,
    at: Pos2,
    anchor: Align2,
    text: &str,
    size: f32,
    colour: Color32,
) -> Rect {
    let font = FontId::proportional(size);
    let shadow = Color32::from_black_alpha(colour.a() - colour.a() / 5);
    painter.text(at + vec2(1.0, 1.0), anchor, text, font.clone(), shadow);
    painter.text(at, anchor, text, font, colour)
}

/// The width a line of `text` takes at `size`.
pub fn width(painter: &Painter, text: &str, size: f32) -> f32 {
    painter
        .layout_no_wrap(text.to_owned(), FontId::proportional(size), TEXT)
        .size()
        .x
}

/// A key's name as a raised key cap, its left edge's middle at `at`, the name `size` high;
/// returns the cap's rectangle. A single letter's cap is square.
pub fn keycap(painter: &Painter, at: Pos2, name: &str, size: f32) -> Rect {
    let galley = painter.layout_no_wrap(name.to_owned(), FontId::proportional(size), TEXT);
    let height = size + 6.0;
    let w = (galley.size().x + 10.0).max(height);
    let rect = Rect::from_min_size(pos2(at.x, at.y - height * 0.5), vec2(w, height));
    raised(painter, rect, RAISED);
    let text_at = rect.center() - galley.size() * 0.5;
    painter.galley(text_at, galley, TEXT);
    rect
}

/// A key cap's width for `name` at `size`.
fn cap_width(painter: &Painter, name: &str, size: f32) -> f32 {
    (width(painter, name, size) + 10.0).max(size + 6.0)
}

/// A line of a plate: words, after a key cap if they say what a key does.
#[derive(Clone, Copy, Debug)]
pub struct Row<'a> {
    pub key: Option<&'a str>,
    pub text: &'a str,
    pub colour: Color32,
    pub size: f32,
}

/// The plates' face: the panel's colour, a little see-through over the world.
const PLATE: Color32 = Color32::from_rgba_premultiplied(41, 38, 36, 225);
const PAD: f32 = 8.0;

/// A plate's row's height.
fn row_height(r: &Row<'_>) -> f32 {
    r.size + if r.key.is_some() { 8.0 } else { 4.0 }
}

/// The key caps' column on a plate of `rows`, and the plate's size.
fn plate_layout(painter: &Painter, rows: &[Row<'_>]) -> (f32, egui::Vec2) {
    let column = (rows.iter())
        .map(|r| r.key.map_or(0.0, |k| cap_width(painter, k, r.size * 0.9)))
        .fold(0.0, f32::max);
    let indent = |r: &Row<'_>| if r.key.is_some() { column + 8.0 } else { 0.0 };
    let wide = (rows.iter())
        .map(|r| indent(r) + width(painter, r.text, r.size))
        .fold(0.0, f32::max);
    let tall: f32 = rows.iter().map(row_height).sum();
    (column, vec2(wide + 2.0 * PAD, tall + 2.0 * PAD))
}

/// The size of the plate `rows` make (`plate`).
pub fn plate_size(painter: &Painter, rows: &[Row<'_>]) -> egui::Vec2 {
    plate_layout(painter, rows).1
}

/// `rows` stacked on a raised plate whose top-left corner is at `at`: the key caps in a column,
/// their words after them aligned. Returns the plate.
pub fn plate(painter: &Painter, at: Pos2, rows: &[Row<'_>]) -> Rect {
    let (column, size) = plate_layout(painter, rows);
    let indent = |r: &Row<'_>| if r.key.is_some() { column + 8.0 } else { 0.0 };
    let height = row_height;
    let rect = Rect::from_min_size(at, size);
    raised(painter, rect, PLATE);
    let mut y = rect.top() + PAD;
    for r in rows {
        let middle = y + height(r) * 0.5;
        if let Some(k) = r.key {
            keycap(painter, pos2(rect.left() + PAD, middle), k, r.size * 0.9);
        }
        let at = pos2(rect.left() + PAD + indent(r), middle);
        let font = FontId::proportional(r.size);
        painter.text(at, Align2::LEFT_CENTER, r.text, font, r.colour);
        y += height(r);
    }
    rect
}

/// The gap between two rows of a strip (points).
const SPACE: f32 = 18.0;

/// A strip's row's width: its key cap and its words.
fn item_width(painter: &Painter, r: &Row<'_>) -> f32 {
    r.key
        .map_or(0.0, |k| cap_width(painter, k, r.size * 0.9) + 6.0)
        + width(painter, r.text, r.size)
}

/// `rows` side by side on raised plates no wider than `most` (points), as many lines as they
/// need, the last line's bottom middle at `at` and the others stacked above it, in order.
/// Returns the space they take (`at` alone when there are none).
pub fn strips(painter: &Painter, at: Pos2, rows: &[Row<'_>], most: f32) -> Rect {
    let mut lines: Vec<Vec<Row<'_>>> = vec![Vec::new()];
    let mut wide = 0.0;
    for r in rows {
        let w = item_width(painter, r) + SPACE;
        let line = lines.last_mut().expect("a line");
        if !line.is_empty() && wide + w > most - 2.0 * PAD {
            lines.push(Vec::new());
            wide = 0.0;
        }
        lines.last_mut().expect("a line").push(*r);
        wide += w;
    }
    let mut taken = Rect::from_min_max(at, at);
    let mut bottom = at;
    for line in lines.iter().rev().filter(|l| !l.is_empty()) {
        let plate = strip(painter, bottom, line);
        taken = taken.union(plate);
        bottom = pos2(at.x, plate.top() - 4.0);
    }
    taken
}

/// `rows` side by side on one raised plate, its bottom's middle at `at`: each key cap, then its
/// words. Returns the plate.
pub fn strip(painter: &Painter, at: Pos2, rows: &[Row<'_>]) -> Rect {
    let tall = rows.iter().map(|r| r.size + 8.0).fold(0.0, f32::max);
    #[allow(clippy::cast_precision_loss)] // a few rows
    let wide: f32 = rows.iter().map(|r| item_width(painter, r)).sum::<f32>()
        + SPACE * rows.len().saturating_sub(1) as f32;
    let size = vec2(wide + 2.0 * PAD, tall + 2.0 * PAD);
    let rect = Rect::from_min_size(at - vec2(size.x * 0.5, size.y), size);
    raised(painter, rect, PLATE);
    let (mut x, middle) = (rect.left() + PAD, rect.center().y);
    for r in rows {
        if let Some(k) = r.key {
            x = keycap(painter, pos2(x, middle), k, r.size * 0.9).right() + 6.0;
        }
        let font = FontId::proportional(r.size);
        let words = painter.text(pos2(x, middle), Align2::LEFT_CENTER, r.text, font, r.colour);
        x = words.right() + SPACE;
    }
    rect
}

/// How the crosshair is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cross {
    /// Nothing to do where it is.
    Plain,
    /// Something within reach can be done: ringed in the highlight.
    Can,
    /// What it is on is out of reach: faint.
    Far,
}

/// The crosshair at `centre`: four short arms with a gap at the middle (the point aimed at is
/// left to be seen), each dark-edged so it shows on any ground.
pub fn crosshair(painter: &Painter, centre: Pos2, cross: Cross) {
    let alpha = if cross == Cross::Far { 120 } else { 235 };
    let light = Color32::from_rgba_unmultiplied(TEXT.r(), TEXT.g(), TEXT.b(), alpha);
    let dark = Color32::from_black_alpha(alpha / 2);
    for d in [
        vec2(1.0, 0.0),
        vec2(-1.0, 0.0),
        vec2(0.0, 1.0),
        vec2(0.0, -1.0),
    ] {
        let seg = [centre + d * 3.0, centre + d * 9.0];
        painter.line_segment(seg, Stroke::new(3.5, dark));
        painter.line_segment(seg, Stroke::new(1.5, light));
    }
    if cross == Cross::Can {
        painter.circle_stroke(centre, 13.0, Stroke::new(3.5, dark));
        painter.circle_stroke(centre, 13.0, Stroke::new(1.5, HIGHLIGHT));
    }
}

/// A ring filling clockwise from the top round `centre` as work goes on (`done`, 0 to 1): its
/// channel dark, the part done in the highlight.
pub fn ring(painter: &Painter, centre: Pos2, radius: f32, done: f32) {
    painter.circle_stroke(
        centre,
        radius,
        Stroke::new(5.0, Color32::from_black_alpha(150)),
    );
    let steps = 64;
    let points: Vec<Pos2> = (0..=steps)
        .map(|k| {
            #[allow(clippy::cast_precision_loss)] // 64 steps
            let a = std::f32::consts::TAU * done.clamp(0.0, 1.0) * (k as f32) / (steps as f32)
                - std::f32::consts::FRAC_PI_2;
            centre + vec2(a.cos(), a.sin()) * radius
        })
        .collect();
    painter.add(egui::Shape::line(points, Stroke::new(3.0, HIGHLIGHT)));
}

/// A bar `rect` filled `share` (0 to 1) in `colour`, in a sunk channel: a pocket's fill.
pub fn bar(painter: &Painter, rect: Rect, share: f32, colour: Color32) {
    sunken(painter, rect, INSET);
    let inner = rect.shrink(2.0);
    let filled = Rect::from_min_size(
        inner.min,
        vec2(inner.width() * share.clamp(0.0, 1.0), inner.height()),
    );
    painter.rect_filled(filled, 0.0, colour);
}
