//! Colour-scale legend for engineering overlays (bottom-right corner).

use creeer_render::Heatmap;
use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, pos2, vec2};

const BAR_WIDTH: f32 = 260.0;
const BAR_HEIGHT: f32 = 14.0;
const LABEL_HEIGHT: f32 = 16.0;
const STEPS: u16 = 64;

/// Draws the colour bar of `heatmap` with its value range, e.g. "Temperature (°C)".
pub fn heatmap_legend(ctx: &egui::Context, heatmap: &Heatmap, title: &str, unit: &str) {
    egui::Window::new("overlay legend")
        .anchor(Align2::RIGHT_BOTTOM, [-12.0, -12.0])
        .title_bar(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.strong(format!("{title} ({unit})"));
            let (rect, _) =
                ui.allocate_exact_size(vec2(BAR_WIDTH, BAR_HEIGHT + LABEL_HEIGHT), Sense::hover());
            let bar = Rect::from_min_size(rect.min, vec2(BAR_WIDTH, BAR_HEIGHT));
            let painter = ui.painter();
            for i in 0..STEPS {
                let (t0, t1) = (
                    f32::from(i) / f32::from(STEPS),
                    f32::from(i + 1) / f32::from(STEPS),
                );
                let [r, g, b] = heatmap.palette.color((t0 + t1) * 0.5);
                let segment = Rect::from_min_max(
                    pos2(bar.left() + t0 * BAR_WIDTH, bar.top()),
                    pos2(bar.left() + t1 * BAR_WIDTH, bar.bottom()),
                );
                painter.rect_filled(segment, 0.0, color(r, g, b));
            }
            painter.rect_stroke(
                bar,
                0.0,
                Stroke::new(1.0, Color32::GRAY),
                StrokeKind::Outside,
            );
            let text_color = ui.visuals().text_color();
            for (t, anchor) in [
                (0.0, Align2::LEFT_TOP),
                (0.5, Align2::CENTER_TOP),
                (1.0, Align2::RIGHT_TOP),
            ] {
                let value = heatmap.min + (heatmap.max - heatmap.min) * t;
                painter.text(
                    pos2(bar.left() + t * BAR_WIDTH, bar.bottom() + 2.0),
                    anchor,
                    format!("{value:.0}"),
                    FontId::proportional(12.0),
                    text_color,
                );
            }
        });
}

/// sRGB float colour → egui colour.
fn color(r: f32, g: f32, b: f32) -> Color32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped to 0..=255
    let channel = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(channel(r), channel(g), channel(b))
}
