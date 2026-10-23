//! The style's automated checks (`docs/art-style.md` §4, §3.3, §3.6): contrast against the
//! grounds, chroma and lightness limits, and player colours told apart. Each finding names what failed
//! and by how much; the user's approved exceptions are honoured, and reported once they are
//! no longer needed.

use crate::colour::{Srgb, difference};
use crate::exception::{Exception, Rule};
use crate::palette::{Group, Palette};
use std::fmt;

/// The most common grounds, which objects must stand out from (§4).
pub const GROUNDS: [&str; 5] = ["soil", "gravel", "granite", "basalt", "grass"];
/// The least ΔE₀₀ between an object's colour and each ground.
pub const GROUND_CONTRAST: f64 = 12.0;
/// The most chroma C\* of terrain that is not an ore.
pub const TERRAIN_CHROMA: f64 = 35.0;
/// The most chroma of an ore.
pub const ORE_CHROMA: f64 = 55.0;
/// The most chroma of materials, wear, the character and vegetation.
pub const OBJECT_CHROMA: f64 = 45.0;
/// The range of L\* of every colour in the scene: no pure black or white.
pub const LIGHTNESS: (f64, f64) = (8.0, 92.0);
/// The least ΔE₀₀ between any two player colours (normal colour vision; the closest pair of
/// §3.6's sixteen, blue and indigo, is 8.5 apart).
pub const PLAYER_DIFFERENCE: f64 = 8.0;

/// A rock as the checks need it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rock<'a> {
    pub name: &'a str,
    pub colour: Srgb,
    pub ore: bool,
}

/// One thing that breaks the style.
#[derive(Clone, Debug, PartialEq)]
pub enum Finding {
    /// An object token too close to a ground.
    Contrast {
        token: String,
        ground: String,
        difference: f64,
    },
    /// A colour more saturated than its group allows.
    Chroma {
        what: String,
        chroma: f64,
        limit: f64,
    },
    /// A colour too near black or white.
    Lightness { what: String, lightness: f64 },
    /// Two player colours too alike.
    Players {
        a: String,
        b: String,
        difference: f64,
    },
    /// A ground the checks need is neither a rock nor a token.
    MissingGround(String),
    /// An exception for something that is neither a token nor a rock, or for a ground that is
    /// not one of [`GROUNDS`].
    UnknownException { subject: String, rule: Rule },
    /// An exception whose limit is met anyway: it should be removed.
    NeedlessException { subject: String, rule: Rule },
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Contrast {
                token,
                ground,
                difference,
            } => write!(
                f,
                "{token} against {ground}: ΔE00 {difference:.1} (at least {GROUND_CONTRAST})"
            ),
            Self::Chroma {
                what,
                chroma,
                limit,
            } => write!(f, "{what}: chroma {chroma:.1} (at most {limit})"),
            Self::Lightness { what, lightness } => write!(
                f,
                "{what}: L* {lightness:.1} (between {} and {})",
                LIGHTNESS.0, LIGHTNESS.1
            ),
            Self::Players { a, b, difference } => write!(
                f,
                "players {a} and {b}: ΔE00 {difference:.1} (at least {PLAYER_DIFFERENCE})"
            ),
            Self::MissingGround(g) => write!(f, "ground {g} is neither a rock nor a token"),
            Self::UnknownException { subject, rule } => {
                write!(f, "exception {rule:?} for {subject}, which is not there")
            }
            Self::NeedlessException { subject, rule } => {
                write!(f, "exception {rule:?} for {subject} is no longer needed")
            }
        }
    }
}

/// Every check over `palette` and the terrain's `rocks`, less what `exceptions` excuse.
pub fn check(palette: &Palette, rocks: &[Rock], exceptions: &[Exception]) -> Vec<Finding> {
    let mut found = Vec::new();
    let grounds = grounds(palette, rocks, &mut found);
    for token in &palette.tokens {
        let Some(colour) = token.shade.fixed() else {
            continue;
        };
        // Objects stand out from the ground; vegetation is told apart by its shape first
        // (§8), wear lies on its material, accents come in pairs, the interface is not lit.
        if matches!(token.group, Group::Material | Group::Character) {
            for (name, ground) in &grounds {
                let d = difference(colour, *ground);
                if d < GROUND_CONTRAST {
                    found.push(Finding::Contrast {
                        token: token.name.clone(),
                        ground: (*name).to_owned(),
                        difference: d,
                    });
                }
            }
        }
        let limit = match token.group {
            Group::Material | Group::Wear | Group::Character | Group::Vegetation => {
                Some(OBJECT_CHROMA)
            }
            Group::Accent | Group::Ui => None,
        };
        chroma(&token.name, colour, limit, &mut found);
        if token.group != Group::Ui {
            lightness(&token.name, colour, &mut found);
        }
    }
    for rock in rocks {
        let limit = if rock.ore { ORE_CHROMA } else { TERRAIN_CHROMA };
        chroma(rock.name, rock.colour, Some(limit), &mut found);
        lightness(rock.name, rock.colour, &mut found);
    }
    for (name, colour) in &palette.players {
        lightness(&format!("player {name}"), *colour, &mut found);
    }
    // The skin tones and hair colours are the character's: its chroma limit and the lightness
    // range. Like the player colours they are not held against the grounds: the clothes are.
    for (kind, list) in [("skin", &palette.skin), ("hair", &palette.hair)] {
        for (name, colour) in list {
            let what = format!("{kind} {name}");
            chroma(&what, *colour, Some(OBJECT_CHROMA), &mut found);
            lightness(&what, *colour, &mut found);
        }
    }
    players(&palette.players, &mut found);
    excuse(found, palette, rocks, exceptions)
}

/// `found` less what an exception covers, plus each exception that is unknown or covers
/// nothing.
fn excuse(
    found: Vec<Finding>,
    palette: &Palette,
    rocks: &[Rock],
    exceptions: &[Exception],
) -> Vec<Finding> {
    let covers = |e: &Exception, rule: &Rule, f: &Finding| match (rule, f) {
        (Rule::Contrast(g), Finding::Contrast { token, ground, .. }) => {
            token == &e.subject && ground == g
        }
        (Rule::Chroma, Finding::Chroma { what, .. }) => what == &e.subject,
        (Rule::Lightness, Finding::Lightness { what, .. }) => what == &e.subject,
        _ => false,
    };
    let mut out: Vec<Finding> = found
        .iter()
        .filter(|f| {
            !exceptions
                .iter()
                .any(|e| e.rules.iter().any(|r| covers(e, r, f)))
        })
        .cloned()
        .collect();
    for e in exceptions {
        let known = palette.token(&e.subject).is_some()
            || rocks.iter().any(|r| r.name == e.subject)
            || palette.person(&e.subject).is_some();
        for rule in &e.rules {
            let ground_known = match rule {
                Rule::Contrast(g) => GROUNDS.contains(&g.as_str()),
                Rule::Chroma | Rule::Lightness => true,
            };
            let (subject, rule) = (e.subject.clone(), rule.clone());
            if !known || !ground_known {
                out.push(Finding::UnknownException { subject, rule });
            } else if !found.iter().any(|f| covers(e, &rule, f)) {
                out.push(Finding::NeedlessException { subject, rule });
            }
        }
    }
    out
}

/// The grounds' colours: rocks first, then tokens (grass).
fn grounds(
    palette: &Palette,
    rocks: &[Rock],
    found: &mut Vec<Finding>,
) -> Vec<(&'static str, Srgb)> {
    let mut out = Vec::new();
    for name in GROUNDS {
        let rock = rocks.iter().find(|r| r.name == name).map(|r| r.colour);
        let token = palette.token(name).and_then(|t| t.shade.fixed());
        match rock.or(token) {
            Some(c) => out.push((name, c)),
            None => found.push(Finding::MissingGround(name.to_owned())),
        }
    }
    out
}

fn chroma(what: &str, colour: Srgb, limit: Option<f64>, found: &mut Vec<Finding>) {
    let c = colour.lab().chroma();
    if let Some(limit) = limit.filter(|&l| c > l) {
        found.push(Finding::Chroma {
            what: what.to_owned(),
            chroma: c,
            limit,
        });
    }
}

fn lightness(what: &str, colour: Srgb, found: &mut Vec<Finding>) {
    let l = colour.lab().l;
    if !(LIGHTNESS.0..=LIGHTNESS.1).contains(&l) {
        found.push(Finding::Lightness {
            what: what.to_owned(),
            lightness: l,
        });
    }
}

/// Every pair of player colours.
fn players(players: &[(String, Srgb)], found: &mut Vec<Finding>) {
    for (i, (a, x)) in players.iter().enumerate() {
        for (b, y) in &players[i + 1..] {
            let d = difference(*x, *y);
            if d < PLAYER_DIFFERENCE {
                found.push(Finding::Players {
                    a: a.clone(),
                    b: b.clone(),
                    difference: d,
                });
            }
        }
    }
}

/// The least ΔE₀₀ between any two of `players`, with the pair's indices.
pub fn closest_players(players: &[(String, Srgb)]) -> Option<(f64, usize, usize)> {
    let mut best: Option<(f64, usize, usize)> = None;
    for i in 0..players.len() {
        for j in i + 1..players.len() {
            let d = difference(players[i].1, players[j].1);
            if best.is_none_or(|(b, _, _)| d < b) {
                best = Some((d, i, j));
            }
        }
    }
    best
}
