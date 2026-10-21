//! The palette's loading and the style's checks.

use crate::checks::{self, Finding, Rock, check, closest_players};
use crate::colour::Srgb;
use crate::palette::{Palette, Shade};
use crate::vision::Vision;
use creeer_data::Data;
use creeer_data::merge::ron_list;
use serde::Deserialize;

const PLAYERS: &str = r##"[("orange", "#e07b39"), ("blue", "#3d7fd9")]"##;

fn parse(tokens: &str) -> Result<Palette, String> {
    Palette::parse(tokens, PLAYERS)
}

#[test]
fn the_base_palette_loads_with_its_wear() {
    let palette = Palette::from_data(&Data::base()).expect("the base palette loads");
    let painted = palette.token("steel_painted").expect("painted steel");
    assert_eq!(painted.wears_to, palette.index("steel"));
    assert_eq!(
        palette.token("player_accent").map(|t| t.shade),
        Some(Shade::PerPlayer)
    );
    assert_eq!(palette.players.len(), 16);
}

#[test]
fn malformed_tokens_are_refused_by_name() {
    let cases = [
        (
            r##"[(name: "a", group: Material, colour: Hex("#12345"))]"##,
            "bad colour",
        ),
        (
            r##"[(name: "a", group: Material, colour: Hex("#123456"), roughness: 1.5)]"##,
            "roughness",
        ),
        (
            r##"[(name: "a", group: Material, colour: Hex("#123456"), metalness: -0.1)]"##,
            "metalness",
        ),
        (
            r##"[(name: "a", group: Material, colour: Unset, wears_to: Some("b"))]"##,
            "no token",
        ),
        (
            r##"[(name: "a", group: Material, colour: Unset, wears_to: Some("a"))]"##,
            "itself",
        ),
        (
            r##"[(name: "a", group: Material, colour: Unset, shine: 1.0)]"##,
            "shine",
        ),
    ];
    for (text, why) in cases {
        let error = parse(text).expect_err(text);
        assert!(error.contains(why), "{error:?} should say {why:?}");
    }
    let twice = Palette::parse("[]", r##"[("a", "#123456"), ("a", "#654321")]"##);
    assert!(twice.expect_err("a player twice").contains("twice"));
    assert!(
        Palette::parse("[]", "[]")
            .expect_err("no players")
            .contains("no player")
    );
}

/// The five grounds as tokens, plus `extra` tokens.
fn with_grounds(extra: &str) -> Palette {
    let grounds = r##"
        (name: "soil", group: Vegetation, colour: Hex("#6b4f35"), roughness: 0.9),
        (name: "gravel", group: Vegetation, colour: Hex("#8c8174"), roughness: 0.9),
        (name: "granite", group: Vegetation, colour: Hex("#c4a194"), roughness: 0.9),
        (name: "basalt", group: Vegetation, colour: Hex("#3d3e44"), roughness: 0.9),
        (name: "grass", group: Vegetation, colour: Hex("#6b7a3f"), roughness: 0.9),"##;
    parse(&format!("[{grounds}{extra}]")).expect("parses")
}

#[test]
fn checks_find_what_breaks_the_style_and_nothing_else() {
    // A grey as light as gravel, a copper too saturated, a near-white, a ground missing.
    let palette = with_grounds(
        r##"(name: "dull", group: Material, colour: Hex("#8a8d90")),
            (name: "loud", group: Material, colour: Hex("#e05010")),
            (name: "glare", group: Accent, colour: Hex("#fcfcfc")),
            (name: "neon", group: Accent, colour: Hex("#00ff00")),
            (name: "fine", group: Material, colour: Hex("#8c99a6"))"##,
    );
    let found = check(&palette, &[]);
    let has = |f: &dyn Fn(&Finding) -> bool| found.iter().any(f);
    assert!(has(
        &|f| matches!(f, Finding::Contrast { token, ground, .. } if token == "dull" && ground == "gravel")
    ));
    assert!(has(
        &|f| matches!(f, Finding::Chroma { what, .. } if what == "loud")
    ));
    assert!(has(
        &|f| matches!(f, Finding::Lightness { what, .. } if what == "glare")
    ));
    // Accents may be saturated; a well-chosen material passes everything.
    assert!(!has(
        &|f| matches!(f, Finding::Chroma { what, .. } if what == "neon")
    ));
    assert!(
        !found.iter().any(|f| f.to_string().starts_with("fine")),
        "{found:?}"
    );
    // Without its grounds, the check says so.
    let bare =
        parse(r##"[(name: "x", group: Material, colour: Hex("#8c99a6"))]"##).expect("parses");
    assert_eq!(
        check(&bare, &[])
            .iter()
            .filter(|f| matches!(f, Finding::MissingGround(_)))
            .count(),
        5
    );
}

#[test]
fn contrast_carried_by_wear_is_accepted_only_on_its_named_grounds() {
    let dull = |carried: &str| {
        with_grounds(&format!(
            r##"(name: "dull", group: Material, colour: Hex("#8a8d90"), carried_on: [{carried}])"##
        ))
    };
    let contrast = |p: &Palette| {
        check(p, &[])
            .into_iter()
            .filter(|f| matches!(f, Finding::Contrast { .. }))
            .count()
    };
    assert_eq!(contrast(&dull("")), 1);
    assert_eq!(contrast(&dull(r#""gravel""#)), 0);
    let found = check(&dull(r#""sand""#), &[]);
    assert!(
        found
            .iter()
            .any(|f| matches!(f, Finding::UnknownGround { .. }))
    );
}

#[test]
fn ores_may_be_more_saturated_than_their_host_rocks() {
    let palette = with_grounds("");
    let copper = Srgb::parse("#c0773f").expect("hex");
    let as_host = [Rock {
        name: "host",
        colour: copper,
        ore: false,
    }];
    let as_ore = [Rock {
        name: "ore",
        colour: copper,
        ore: true,
    }];
    assert!(
        check(&palette, &as_host)
            .iter()
            .any(|f| matches!(f, Finding::Chroma { .. }))
    );
    assert!(
        !check(&palette, &as_ore)
            .iter()
            .any(|f| matches!(f, Finding::Chroma { .. }))
    );
}

#[test]
fn player_colours_are_judged_as_each_vision_sees_them() {
    // Blue and violet: apart in normal vision, together under deuteranopia.
    let players = vec![
        ("blue".to_owned(), Srgb([61, 127, 217])),
        ("violet".to_owned(), Srgb([142, 91, 214])),
    ];
    let normal = closest_players(&players, Vision::Normal).expect("a pair").0;
    let deutan = closest_players(&players, Vision::Deuteranopia)
        .expect("a pair")
        .0;
    assert!(normal > checks::PLAYER_DIFFERENCE && deutan < checks::PLAYER_DIFFERENCE);
    let palette = Palette {
        tokens: Vec::new(),
        players,
    };
    let found = check(&palette, &[]);
    assert!(found.iter().any(|f| matches!(
        f,
        Finding::Players {
            vision: Vision::Deuteranopia,
            ..
        }
    )));
    assert!(!found.iter().any(|f| matches!(
        f,
        Finding::Players {
            vision: Vision::Normal,
            ..
        }
    )));
}

#[derive(Deserialize)]
struct RockDef {
    name: String,
    color: String,
    #[serde(default)]
    ore: bool,
}

/// The base pack's findings, printed (`cargo test -p creeer_art -- --nocapture base`).
#[test]
fn base_findings() {
    let data = Data::base();
    let palette = Palette::from_data(&data).expect("the base palette loads");
    let defs = (data.table("rocks", "geology/rocks.ron"))
        .merge(ron_list, |r: &RockDef| &r.name)
        .expect("rocks load");
    let rocks: Vec<Rock> = defs
        .iter()
        .map(|r| Rock {
            name: &r.name,
            colour: Srgb::parse(&r.color).expect("a rock colour"),
            ore: r.ore,
        })
        .collect();
    for f in check(&palette, &rocks) {
        println!("{f}");
    }
    for v in Vision::ALL {
        if let Some((d, i, j)) = closest_players(&palette.players, v) {
            println!(
                "closest players under {v:?}: {} and {} at {d:.1}",
                palette.players[i].0, palette.players[j].0
            );
        }
    }
}
