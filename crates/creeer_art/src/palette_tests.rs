//! The palette's loading and the style's checks.

use crate::checks::{self, Finding, Rock, check, closest_players};
use crate::colour::Srgb;
use crate::exception::{Exception, Rule};
use crate::palette::{Palette, Shade};
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
    let found = check(&palette, &[], &[]);
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
        check(&bare, &[], &[])
            .iter()
            .filter(|f| matches!(f, Finding::MissingGround(_)))
            .count(),
        5
    );
}

fn exception(subject: &str, rule: Rule) -> Exception {
    Exception {
        subject: subject.to_owned(),
        rules: vec![rule],
        why: "a test".to_owned(),
        approved: "2026-10-21".to_owned(),
    }
}

#[test]
fn exceptions_excuse_exactly_what_they_name_and_say_when_they_are_needless() {
    let palette = with_grounds(r##"(name: "dull", group: Material, colour: Hex("#8a8d90"))"##);
    let contrast = |e: &[Exception]| {
        check(&palette, &[], e)
            .into_iter()
            .filter(|f| matches!(f, Finding::Contrast { .. }))
            .count()
    };
    assert_eq!(contrast(&[]), 1);
    assert_eq!(
        contrast(&[exception("dull", Rule::Contrast("gravel".into()))]),
        0
    );
    // On another ground, it excuses nothing and is reported as needless.
    let wrong = [exception("dull", Rule::Contrast("soil".into()))];
    assert_eq!(contrast(&wrong), 1);
    let found = check(&palette, &[], &wrong);
    assert!(
        found
            .iter()
            .any(|f| matches!(f, Finding::NeedlessException { .. }))
    );
    // Unknown subjects and grounds are refused.
    for bad in [
        exception("nothing", Rule::Chroma),
        exception("dull", Rule::Contrast("sand".into())),
    ] {
        let found = check(&palette, &[], &[bad]);
        assert!(
            found
                .iter()
                .any(|f| matches!(f, Finding::UnknownException { .. }))
        );
    }
}

#[test]
fn ores_may_be_more_saturated_than_their_host_rocks() {
    let palette = with_grounds("");
    let copper = Srgb::parse("#c0773f").expect("hex");
    let rock = |ore| {
        [Rock {
            name: "rock",
            colour: copper,
            ore,
        }]
    };
    let saturated = |rocks: &[Rock]| {
        check(&palette, rocks, &[])
            .iter()
            .any(|f| matches!(f, Finding::Chroma { .. }))
    };
    assert!(saturated(&rock(false)));
    assert!(!saturated(&rock(true)));
}

#[test]
fn player_colours_too_alike_are_found() {
    let players = vec![
        ("orange".to_owned(), Srgb([224, 123, 57])),
        ("amber".to_owned(), Srgb([226, 128, 60])),
        ("blue".to_owned(), Srgb([61, 127, 217])),
    ];
    let (d, i, j) = closest_players(&players).expect("a pair");
    assert!(d < checks::PLAYER_DIFFERENCE && (i, j) == (0, 1));
    let palette = Palette {
        tokens: Vec::new(),
        players,
    };
    let found = check(&palette, &[], &[]);
    let pairs: Vec<_> = found
        .iter()
        .filter(|f| matches!(f, Finding::Players { .. }))
        .collect();
    assert_eq!(pairs.len(), 1, "{found:?}");
}

#[derive(Deserialize)]
struct RockDef {
    name: String,
    color: String,
    #[serde(default)]
    ore: bool,
}

#[test]
fn the_base_pack_keeps_the_style_with_its_approved_exceptions() {
    let data = Data::base();
    let palette = Palette::from_data(&data).expect("the base palette loads");
    let exceptions = Exception::from_data(&data).expect("the exceptions load");
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
    let found: Vec<String> = check(&palette, &rocks, &exceptions)
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(found.is_empty(), "{found:#?}");
    let (d, i, j) = closest_players(&palette.players).expect("sixteen players");
    let pair = (palette.players[i].0.as_str(), palette.players[j].0.as_str());
    assert_eq!(pair, ("blue", "indigo"));
    assert!((d - 8.5).abs() < 0.05, "{d}");
}
