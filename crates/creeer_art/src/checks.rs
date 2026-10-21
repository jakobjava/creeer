//! The style's automated checks (`docs/art-style.md` §4, §3.6): contrast against the grounds,
//! chroma and lightness limits, and player colours told apart under colour-vision
//! deficiencies. Each finding names what failed and by how much.

use crate::colour::{Srgb, ciede2000, difference};
use crate::palette::{Group, Palette};
use crate::vision::Vision;
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
/// The least ΔE₀₀ between any two player colours, as each vision sees them.
pub const PLAYER_DIFFERENCE: f64 = 10.0;

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
    /// Two player colours that `vision` can hardly tell apart.
    Players {
        a: String,
        b: String,
        vision: Vision,
        difference: f64,
    },
    /// A ground the checks need is neither a rock nor a token.
    MissingGround(String),
    /// A token carried on a ground that is not one of [`GROUNDS`].
    UnknownGround { token: String, ground: String },
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
            Self::Players {
                a,
                b,
                vision,
                difference,
            } => write!(
                f,
                "players {a} and {b} under {vision:?}: ΔE00 {difference:.1} (at least {PLAYER_DIFFERENCE})"
            ),
            Self::MissingGround(g) => write!(f, "ground {g} is neither a rock nor a token"),
            Self::UnknownGround { token, ground } => {
                write!(f, "{token} is carried on {ground}, which is no ground")
            }
        }
    }
}

/// Every check over `palette` and the terrain's `rocks`.
pub fn check(palette: &Palette, rocks: &[Rock]) -> Vec<Finding> {
    let mut found = Vec::new();
    let grounds = grounds(palette, rocks, &mut found);
    for token in &palette.tokens {
        let Some(colour) = token.shade.fixed() else {
            continue;
        };
        for ground in &token.carried_on {
            if !GROUNDS.contains(&ground.as_str()) {
                found.push(Finding::UnknownGround {
                    token: token.name.clone(),
                    ground: ground.clone(),
                });
            }
        }
        // Objects stand out from the ground; vegetation is told apart by its shape first
        // (§8), wear lies on its material, accents come in pairs, the interface is not lit.
        if matches!(token.group, Group::Material | Group::Character) {
            for (name, ground) in &grounds {
                let d = difference(colour, *ground);
                if d < GROUND_CONTRAST && !token.carried_on.iter().any(|g| g == name) {
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
    players(&palette.players, &mut found);
    found
}

/// The grounds' colours: rocks first, then tokens (grass).
fn grounds<'a>(
    palette: &Palette,
    rocks: &[Rock],
    found: &mut Vec<Finding>,
) -> Vec<(&'a str, Srgb)> {
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

/// Every pair of player colours, as each vision sees them.
fn players(players: &[(String, Srgb)], found: &mut Vec<Finding>) {
    for vision in Vision::ALL {
        let seen: Vec<_> = players.iter().map(|(_, c)| vision.sees(*c).lab()).collect();
        for i in 0..players.len() {
            for j in i + 1..players.len() {
                let d = ciede2000(seen[i], seen[j]);
                if d < PLAYER_DIFFERENCE {
                    found.push(Finding::Players {
                        a: players[i].0.clone(),
                        b: players[j].0.clone(),
                        vision,
                        difference: d,
                    });
                }
            }
        }
    }
}

/// The least ΔE₀₀ between any two of `players` under `vision`, with the pair.
pub fn closest_players(players: &[(String, Srgb)], vision: Vision) -> Option<(f64, usize, usize)> {
    let seen: Vec<_> = players.iter().map(|(_, c)| vision.sees(*c).lab()).collect();
    let mut best: Option<(f64, usize, usize)> = None;
    for i in 0..players.len() {
        for j in i + 1..players.len() {
            let d = ciede2000(seen[i], seen[j]);
            if best.is_none_or(|(b, _, _)| d < b) {
                best = Some((d, i, j));
            }
        }
    }
    best
}
