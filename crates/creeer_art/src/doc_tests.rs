//! The palette in data against `docs/art-style.md`, the authority (§11.2): every token,
//! colour, roughness and metalness in its tables, every player colour in §3.6 and every
//! terrain colour in §3.1 are in the data, and the data has nothing the file does not.

use crate::colour::Srgb;
use crate::palette::{Group, Palette, Shade};
use creeer_data::Data;
use creeer_data::merge::ron_list;
use serde::Deserialize;

const STYLE: &str = include_str!("../../../docs/art-style.md");

/// A token as the file's tables give it.
#[derive(Debug)]
struct Row {
    name: String,
    shade: Shade,
    roughness: Option<f64>,
    metalness: Option<f64>,
}

/// The `backticked` items of a cell.
fn backticked(cell: &str) -> Vec<&str> {
    cell.split('`').skip(1).step_by(2).collect()
}

/// A colour cell: one or more `#rrggbb`, or words saying where the colour comes from.
fn shades(cell: &str) -> Vec<Shade> {
    let hexes: Vec<Shade> = backticked(cell)
        .into_iter()
        .filter(|s| s.starts_with('#'))
        .map(|s| Shade::Fixed(Srgb::parse(s).unwrap_or_else(|| panic!("bad colour {s}"))))
        .collect();
    if !hexes.is_empty() {
        return hexes;
    }
    let words = cell.to_lowercase();
    vec![if words == "per player" {
        Shade::PerPlayer
    } else if words == "from temperature" {
        Shade::FromTemperature
    } else if words.starts_with("set in") {
        Shade::Unset
    } else {
        panic!("colour cell {cell:?} is neither colours nor a known source")
    }]
}

/// Every row of every table whose first column is "Token".
fn doc_tokens() -> Vec<Row> {
    let mut rows = Vec::new();
    let mut header: Option<Vec<String>> = None;
    for line in STYLE.lines().map(str::trim) {
        if !line.starts_with('|') {
            header = None;
            continue;
        }
        let cells: Vec<String> = line
            .trim_matches('|')
            .split('|')
            .map(|c| c.trim().to_owned())
            .collect();
        if cells
            .iter()
            .all(|c| c.chars().all(|ch| ch == '-' || ch == ':'))
        {
            continue;
        }
        let Some(h) = &header else {
            header = Some(cells);
            continue;
        };
        if h[0] != "Token" {
            continue;
        }
        let column = |name: &str| h.iter().position(|c| c == name);
        let number = |name: &str| column(name).and_then(|k| cells[k].parse::<f64>().ok());
        let names = backticked(&cells[0]);
        let colours = shades(&cells[column("Colour").expect("a Colour column")]);
        assert!(
            colours.len() == names.len() || colours.len() == 1,
            "row {line:?}: {} names, {} colours",
            names.len(),
            colours.len()
        );
        for (k, name) in names.iter().enumerate() {
            rows.push(Row {
                name: (*name).to_owned(),
                shade: colours[if colours.len() == 1 { 0 } else { k }],
                roughness: number("Roughness"),
                metalness: number("Metalness"),
            });
        }
    }
    rows
}

/// The text of the section that starts with `heading`, up to the next heading.
fn section(heading: &str) -> &'static str {
    let start = STYLE
        .find(heading)
        .unwrap_or_else(|| panic!("{heading} not found"));
    let rest = &STYLE[start + heading.len()..];
    let end = rest.find("\n#").unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn every_token_in_the_style_is_in_the_data_and_nothing_else() {
    let palette = Palette::from_data(&Data::base()).expect("the base palette loads");
    let rows = doc_tokens();
    assert!(
        rows.len() > 40,
        "only {} tokens found in the file",
        rows.len()
    );
    for row in &rows {
        let token = palette
            .token(&row.name)
            .unwrap_or_else(|| panic!("{} is in art-style.md but not in the data", row.name));
        assert_eq!(token.shade, row.shade, "{}", row.name);
        if let Some(r) = row.roughness {
            assert!(
                (token.roughness - r).abs() < 1e-12,
                "{} roughness",
                row.name
            );
        }
        // Tables without a metalness column are of dielectrics.
        let metal = row.metalness.unwrap_or(0.0);
        assert!(
            (token.metalness - metal).abs() < 1e-12,
            "{} metalness",
            row.name
        );
    }
    for token in &palette.tokens {
        assert!(
            rows.iter().any(|r| r.name == token.name),
            "{} is in the data but not in art-style.md",
            token.name
        );
        if token.group != Group::Ui && token.shade != Shade::FromTemperature {
            let row = rows
                .iter()
                .find(|r| r.name == token.name)
                .expect("found above");
            assert!(
                row.roughness.is_some(),
                "{} has no roughness in the file",
                token.name
            );
        }
    }
}

#[test]
fn player_colours_are_the_styles() {
    let palette = Palette::from_data(&Data::base()).expect("the base palette loads");
    let text = section("### 3.6 Player colours");
    let mut listed = Vec::new();
    for piece in text.split('`').collect::<Vec<_>>().windows(2) {
        if let Some(colour) = Srgb::parse(piece[0]) {
            let name = piece[1].trim_start().split([',', '.']).next().unwrap_or("");
            listed.push((name.trim().to_owned(), colour));
        }
    }
    assert_eq!(listed, palette.players);
}

/// Every `.rs` and `.wgsl` file under `dir`, with its text.
fn sources(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
    for entry in std::fs::read_dir(dir)
        .expect("a source directory")
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs" || e == "wgsl") {
            let text = std::fs::read_to_string(&path).expect("a source file");
            out.push((path.display().to_string(), text));
        }
    }
}

#[test]
fn the_renderer_has_no_forbidden_effect() {
    let render = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../creeer_render/src");
    let mut files = Vec::new();
    sources(&render, &mut files);
    assert!(files.len() > 20, "the renderer's sources were not found");
    // Identifiers and words alike: `film_grain`, `FilmGrain`, "film grain"; colour or color.
    let squash = |s: &str| {
        s.to_lowercase()
            .replace("colour", "color")
            .replace(['_', '-', ' '], "")
    };
    for (path, text) in &files {
        let text = squash(text);
        for effect in crate::FORBIDDEN_EFFECTS {
            assert!(
                !text.contains(&squash(effect)),
                "{path} mentions {effect:?}, which art-style.md §5 forbids"
            );
        }
    }
}

#[derive(Deserialize)]
struct RockColour {
    name: String,
    color: String,
}

#[test]
fn terrain_colours_named_in_the_style_are_the_rocks() {
    let data = Data::base();
    let rocks = (data.table("rocks", "geology/rocks.ron"))
        .merge(ron_list, |r: &RockColour| &r.name)
        .expect("rocks load");
    let named: Vec<&str> = backticked(section("### 3.1 Terrain"));
    assert!(named.len() >= 8);
    for hex in named.into_iter().filter(|s| s.starts_with('#')) {
        assert!(
            rocks.iter().any(|r| r.color == hex),
            "art-style.md §3.1 names {hex}, which no rock has"
        );
    }
}
