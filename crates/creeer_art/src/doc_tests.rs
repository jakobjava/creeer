//! The palette in data against `docs/art-style.md`, the authority (§11.2): every token,
//! colour, roughness and metalness in its tables, every player colour in §3.6 and every
//! terrain colour in §3.1 are in the data, and the data has nothing the file does not.

use crate::colour::Srgb;
use crate::exception::{Exception, Rule};
use crate::palette::{Group, Palette, Shade};
use creeer_data::Data;
use creeer_data::merge::ron_list;
use serde::Deserialize;

const STYLE: &str = include_str!("../../../docs/art-style.md");

/// A token as the file's tables give it; a column the table lacks reads as its default.
#[derive(Debug)]
struct Row {
    name: String,
    shade: Shade,
    roughness: Option<f64>,
    metalness: Option<f64>,
    pattern: String,
    wears_to: Option<String>,
    ages_to: Option<String>,
    weathers_in: Option<f64>,
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
        let token = |name: &str| {
            column(name).and_then(|k| backticked(&cells[k]).first().map(|s| (*s).to_owned()))
        };
        let pattern = column("Pattern").map_or_else(|| "Plain".to_owned(), |k| cells[k].clone());
        for (k, name) in names.iter().enumerate() {
            rows.push(Row {
                name: (*name).to_owned(),
                shade: colours[if colours.len() == 1 { 0 } else { k }],
                roughness: number("Roughness"),
                metalness: number("Metalness"),
                pattern: pattern.clone(),
                wears_to: token("Wears to"),
                ages_to: token("Ages to"),
                weathers_in: number("Weathers in"),
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
        assert_eq!(
            format!("{:?}", token.pattern),
            row.pattern,
            "{} pattern",
            row.name
        );
        let name = |k: Option<usize>| k.map(|k| palette.tokens[k].name.clone());
        assert_eq!(name(token.wears_to), row.wears_to, "{} wears to", row.name);
        assert_eq!(name(token.ages_to), row.ages_to, "{} ages to", row.name);
        assert_eq!(
            token.weathers_in, row.weathers_in,
            "{} weathers in",
            row.name
        );
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

/// The colours listed in `text` as "`#rrggbb` name, …", in order.
fn listed(text: &str) -> Vec<(String, Srgb)> {
    let mut out = Vec::new();
    for piece in text.split('`').collect::<Vec<_>>().windows(2) {
        if let Some(colour) = Srgb::parse(piece[0]) {
            let name = piece[1].trim_start().split([',', '.']).next().unwrap_or("");
            out.push((name.trim().to_owned(), colour));
        }
    }
    out
}

/// The paragraph of `section` that begins with `start`.
fn paragraph(section: &'static str, start: &str) -> &'static str {
    let from = section
        .find(start)
        .unwrap_or_else(|| panic!("{start} not found"));
    let rest = &section[from..];
    &rest[..rest.find("\n\n").unwrap_or(rest.len())]
}

#[test]
fn player_colours_are_the_styles() {
    let palette = Palette::from_data(&Data::base()).expect("the base palette loads");
    assert_eq!(listed(section("### 3.6 Player colours")), palette.players);
}

#[test]
fn skin_tones_and_hair_colours_are_the_styles() {
    let palette = Palette::from_data(&Data::base()).expect("the base palette loads");
    let text = section("### 3.3 The character");
    assert_eq!(listed(paragraph(text, "**Skin tones.**")), palette.skin);
    assert_eq!(listed(paragraph(text, "**Hair colours.**")), palette.hair);
    assert_eq!((palette.skin.len(), palette.hair.len()), (10, 9));
}

#[test]
fn the_approved_exceptions_are_the_styles() {
    let data = Data::base();
    let mut in_data: Vec<(String, Rule, String)> = Exception::from_data(&data)
        .expect("the exceptions load")
        .into_iter()
        .flat_map(|e| {
            let (subject, approved) = (e.subject, e.approved);
            e.rules
                .into_iter()
                .map(move |r| (subject.clone(), r, approved.clone()))
        })
        .collect();
    // The rows of §4's table: Subject | Limit lifted | Why | Approved.
    let mut in_doc = Vec::new();
    for line in section("## 4. Value, saturation and contrast").lines() {
        let cells: Vec<&str> = line
            .trim()
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        if cells.len() != 4 || cells[0] == "Subject" || cells[0].starts_with("---") {
            continue;
        }
        let subject = cells[0].trim_matches('`').to_owned();
        let rule = match cells[1].strip_prefix("contrast against ") {
            Some(ground) => Rule::Contrast(ground.to_owned()),
            None if cells[1] == "chroma" => Rule::Chroma,
            None if cells[1] == "lightness" => Rule::Lightness,
            None => panic!("unknown limit {:?}", cells[1]),
        };
        in_doc.push((subject, rule, cells[3].to_owned()));
    }
    let key = |e: &(String, Rule, String)| format!("{e:?}");
    in_data.sort_by_key(key);
    in_doc.sort_by_key(key);
    assert!(!in_doc.is_empty(), "§4's table was not found");
    assert_eq!(in_doc, in_data);
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
