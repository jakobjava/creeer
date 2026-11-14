//! The interface's look (G2.11 A3, ADR-0038): `docs/art-style.md` §10's tokens (the authority;
//! `data/art/palette.ron` keeps them, and a test checks these against it) applied to egui:
//! warm dark panels, raised and bevelled with a light and a dark edge, square corners, no
//! glassy blur, one orange for what is chosen. Text sized to read at 1080p from a desk; the UI
//! scale (the settings) makes all of it larger.

use egui::{Color32, CornerRadius, FontFamily, FontId, Shadow, Stroke, TextStyle, Visuals};

/// `art-style.md` §10's tokens.
pub const PANEL: Color32 = Color32::from_rgb(0x2e, 0x2b, 0x29);
pub const INSET: Color32 = Color32::from_rgb(0x23, 0x21, 0x20);
pub const BEVEL_LIGHT: Color32 = Color32::from_rgb(0x4a, 0x46, 0x43);
pub const BEVEL_DARK: Color32 = Color32::from_rgb(0x19, 0x18, 0x17);
pub const TEXT: Color32 = Color32::from_rgb(0xe8, 0xe2, 0xd6);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0xa8, 0xa1, 0x96);
pub const HIGHLIGHT: Color32 = Color32::from_rgb(0xf0, 0xa0, 0x30);
pub const GOOD: Color32 = Color32::from_rgb(0x7f, 0xb2, 0x4a);
pub const WARN: Color32 = Color32::from_rgb(0xe8, 0xb4, 0x00);
pub const BAD: Color32 = Color32::from_rgb(0xd0, 0x45, 0x2f);

/// The tokens by their names in the palette, for the check against it.
pub const TOKENS: [(&str, Color32); 10] = [
    ("ui_panel", PANEL),
    ("ui_inset", INSET),
    ("ui_bevel_light", BEVEL_LIGHT),
    ("ui_bevel_dark", BEVEL_DARK),
    ("ui_text", TEXT),
    ("ui_text_dim", TEXT_DIM),
    ("ui_highlight", HIGHLIGHT),
    ("ui_good", GOOD),
    ("ui_warn", WARN),
    ("ui_bad", BAD),
];

/// A raised button's face: between the panel and its light edge.
pub const RAISED: Color32 = Color32::from_rgb(0x3b, 0x37, 0x34);

/// Text sizes (points; pixels at UI scale 1 on a screen of scale 1).
pub const SMALL: f32 = 12.5;
pub const BODY: f32 = 15.0;
pub const HEADING: f32 = 21.0;

/// Gives `ctx` the game's look.
pub fn apply(ctx: &egui::Context) {
    ctx.set_visuals_of(egui::Theme::Dark, visuals());
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (
                TextStyle::Small,
                FontId::new(SMALL, FontFamily::Proportional),
            ),
            (TextStyle::Body, FontId::new(BODY, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(BODY, FontFamily::Proportional),
            ),
            (
                TextStyle::Heading,
                FontId::new(HEADING, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(13.5, FontFamily::Monospace),
            ),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.spacing.window_margin = egui::Margin::same(10);
        style.spacing.interact_size.y = 22.0;
    });
}

/// The visuals: §10's colours on egui's widgets.
pub fn visuals() -> Visuals {
    let mut v = Visuals::dark();
    let square = CornerRadius::ZERO;
    // A hard shadow below: raised, not blurred.
    let raised = Shadow {
        offset: [0, 2],
        blur: 0,
        spread: 0,
        color: BEVEL_DARK,
    };
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.window_stroke = Stroke::new(1.0, BEVEL_LIGHT);
    v.window_corner_radius = square;
    v.menu_corner_radius = square;
    v.window_shadow = raised;
    v.popup_shadow = raised;
    v.extreme_bg_color = INSET;
    v.faint_bg_color = Color32::from_rgb(0x34, 0x31, 0x2e);
    v.code_bg_color = INSET;
    v.hyperlink_color = HIGHLIGHT;
    v.warn_fg_color = WARN;
    v.error_fg_color = BAD;
    v.weak_text_color = Some(TEXT_DIM);
    v.slider_trailing_fill = true;
    v.selection.bg_fill = Color32::from_rgb(0x6e, 0x4c, 0x1f);
    v.selection.stroke = Stroke::new(1.0, HIGHLIGHT);
    let w = &mut v.widgets;
    for (state, fill, edge, text) in [
        (&mut w.noninteractive, PANEL, BEVEL_DARK, TEXT),
        (&mut w.inactive, RAISED, BEVEL_LIGHT, TEXT),
        (&mut w.hovered, BEVEL_LIGHT, HIGHLIGHT, TEXT),
        (&mut w.active, INSET, HIGHLIGHT, HIGHLIGHT),
        (&mut w.open, BEVEL_LIGHT, BEVEL_LIGHT, TEXT),
    ] {
        state.weak_bg_fill = fill;
        state.bg_fill = if fill == PANEL { PANEL } else { INSET };
        state.bg_stroke = Stroke::new(1.0, edge);
        state.fg_stroke = Stroke::new(1.0, text);
        state.corner_radius = square;
        state.expansion = 0.0;
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §11.2: the UI theme's constants are checked against the palette, as the models' are.
    #[test]
    fn the_themes_colours_are_the_palettes() {
        let data = creeer_data::Data::base();
        let palette = creeer_art::palette::Palette::from_data(&data).expect("palette");
        for (name, colour) in TOKENS {
            let token = palette
                .token(name)
                .unwrap_or_else(|| panic!("{name} in the palette"));
            let [r, g, b] = token.shade.fixed().expect("a fixed colour").0;
            assert_eq!(Color32::from_rgb(r, g, b), colour, "{name}");
        }
    }

    #[test]
    fn the_theme_applies_and_keeps_square_corners() {
        let ctx = egui::Context::default();
        apply(&ctx);
        let style = ctx.style_of(egui::Theme::Dark);
        assert_eq!(style.visuals.window_fill, PANEL);
        assert_eq!(style.visuals.window_corner_radius, CornerRadius::ZERO);
        assert_eq!(style.text_styles[&TextStyle::Body].size, BODY);
    }
}
