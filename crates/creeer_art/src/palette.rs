//! The palette in data (`data/art/palette.ron`, `data/art/players.ron`,
//! `data/art/people.ron`): every token of `docs/art-style.md` §3 and §10, which models and the
//! renderer name instead of colours, and the colours each player chooses from. A pack recolours
//! a token by naming it again (ADR-0018's tables); the player colours, and the skin tones and
//! hair colours, are one file each, replaced whole by the last pack that has it.

use crate::colour::Srgb;
use creeer_data::Data;
use creeer_data::merge::ron_list;
use serde::Deserialize;

/// What a token is for; the checks hold each group to its own limits (`art-style.md` §4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
pub enum Group {
    /// What objects are made of (§3.2).
    Material,
    /// What wear and age show on a material: rust, patina (§3.2).
    Wear,
    /// The engineer (§3.3).
    Character,
    /// Trees, grass and cut wood (§3.4).
    Vegetation,
    /// Saturated colour for what matters (§3.5).
    Accent,
    /// The interface (§10); not lit, not in the scene.
    Ui,
}

/// Where a token's colour comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shade {
    /// Fixed.
    Fixed(Srgb),
    /// Each player's own (the accent, skin, hair).
    PerPlayer,
    /// The surface's temperature (blackbody glow).
    FromTemperature,
    /// Named by the style, not coloured yet.
    Unset,
}

impl Shade {
    /// The colour, if it is fixed.
    pub fn fixed(self) -> Option<Srgb> {
        match self {
            Self::Fixed(c) => Some(c),
            _ => None,
        }
    }
}

/// The fine surface a token has (`art-style.md` §6): what the renderer draws within it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Deserialize)]
pub enum Pattern {
    /// Uniform, with only the shared mottling and grain.
    #[default]
    Plain,
    /// Grain lines along the part, growth rings, the odd knot.
    Wood,
    /// Paint over a surface: fine orange peel, chips at the edges.
    Paint,
    /// Bare metal: brushed along the part.
    Metal,
    /// Mineral: speckle at several scales.
    Stone,
    /// A woven cloth.
    Fabric,
    /// Pebbled hide.
    Leather,
    /// Smooth, with faint streaks.
    Glass,
}

/// One token.
#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub name: String,
    pub group: Group,
    pub shade: Shade,
    /// 0 mirror-smooth, 1 fully matte.
    pub roughness: f64,
    /// 0 dielectric, 1 bare metal.
    pub metalness: f64,
    pub pattern: Pattern,
    /// The token that wear on convex edges reveals (painted steel wears to bare steel).
    pub wears_to: Option<usize>,
    /// The token that age turns it into, in patches (timber greys, bare steel rusts).
    pub ages_to: Option<usize>,
    /// The years ageing takes outdoors in temperate air (ISO 9223 C3; `art-style.md` §6b).
    pub weathers_in: Option<f64>,
}

/// The palette: the tokens, in data order, and the player colours.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Palette {
    pub tokens: Vec<Token>,
    /// (name, colour) in the order players are given them.
    pub players: Vec<(String, Srgb)>,
    /// The skin tones a player chooses from for `skin`, (name, colour), lightest first.
    pub skin: Vec<(String, Srgb)>,
    /// The hair colours a player chooses from for `hair`, (name, colour).
    pub hair: Vec<(String, Srgb)>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PeopleDef {
    skin: Vec<(String, String)>,
    hair: Vec<(String, String)>,
}

#[derive(Deserialize)]
enum ShadeDef {
    Hex(String),
    PerPlayer,
    FromTemperature,
    Unset,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TokenDef {
    name: String,
    group: Group,
    colour: ShadeDef,
    #[serde(default = "matte")]
    roughness: f64,
    #[serde(default)]
    metalness: f64,
    #[serde(default)]
    pattern: Pattern,
    #[serde(default)]
    wears_to: Option<String>,
    #[serde(default)]
    ages_to: Option<String>,
    #[serde(default)]
    weathers_in: Option<f64>,
}

fn matte() -> f64 {
    1.0
}

/// Where the palette lives in a pack.
pub const TOKENS_FILE: &str = "art/palette.ron";
/// Where the player colours live in a pack.
pub const PLAYERS_FILE: &str = "art/players.ron";
/// Where the skin tones and hair colours live in a pack.
pub const PEOPLE_FILE: &str = "art/people.ron";

impl Palette {
    /// The palette as the packs of `data` merge it.
    ///
    /// # Errors
    /// If a file does not parse, or a token is malformed (a bad colour, roughness or metalness
    /// outside 0–1, wear to a token that does not exist), each named with its origin.
    pub fn from_data(data: &Data) -> Result<Self, String> {
        let defs = (data.table("palette", TOKENS_FILE))
            .merge_traced(ron_list, |t: &TokenDef| &t.name)
            .map_err(|e| e.to_string())?;
        let players = data.file(PLAYERS_FILE).map_err(|e| e.to_string())?;
        let people = data.file(PEOPLE_FILE).map_err(|e| e.to_string())?;
        Self::build(defs, players)?.with_people(people)
    }

    /// The palette with the skin tones and hair colours in `text` (`people.ron`'s form).
    ///
    /// # Errors
    /// If it does not parse, a list is empty, a colour is malformed or a name repeats.
    pub fn with_people(mut self, text: &str) -> Result<Self, String> {
        let def: PeopleDef = ron::from_str(text).map_err(|e| format!("{PEOPLE_FILE}: {e}"))?;
        self.skin = named(PEOPLE_FILE, "skin tones", def.skin)?;
        self.hair = named(PEOPLE_FILE, "hair colours", def.hair)?;
        Ok(self)
    }

    /// The palette from the texts of its two files (tests, tools).
    ///
    /// # Errors
    /// As [`Palette::from_data`].
    pub fn parse(tokens: &str, players: &str) -> Result<Self, String> {
        let defs: Vec<TokenDef> = ron_list(tokens).map_err(|e| format!("{TOKENS_FILE}: {e}"))?;
        let defs = defs.into_iter().map(|d| (d, TOKENS_FILE.to_owned()));
        Self::build(defs.collect(), players)
    }

    fn build(defs: Vec<(TokenDef, String)>, players: &str) -> Result<Self, String> {
        let mut tokens = Vec::with_capacity(defs.len());
        for (def, origin) in &defs {
            let at = |what: String| format!("{origin}: {}: {what}", def.name);
            let shade = match &def.colour {
                ShadeDef::Hex(text) => Shade::Fixed(
                    Srgb::parse(text).ok_or_else(|| at(format!("bad colour {text:?}")))?,
                ),
                ShadeDef::PerPlayer => Shade::PerPlayer,
                ShadeDef::FromTemperature => Shade::FromTemperature,
                ShadeDef::Unset => Shade::Unset,
            };
            for (what, v) in [("roughness", def.roughness), ("metalness", def.metalness)] {
                if !(0.0..=1.0).contains(&v) {
                    return Err(at(format!("{what} {v} is outside 0–1")));
                }
            }
            // Another token, named in `field` ("wears to", "ages to").
            let other = |field: &str, name: &Option<String>| match name {
                None => Ok(None),
                Some(name) if name == &def.name => Err(at(format!("{field} itself"))),
                Some(name) => defs
                    .iter()
                    .position(|(d, _)| &d.name == name)
                    .map(Some)
                    .ok_or_else(|| at(format!("{field} {name:?}, which is no token"))),
            };
            let ages_to = other("ages to", &def.ages_to)?;
            match (ages_to, def.weathers_in) {
                (Some(_), Some(years)) if years > 0.0 && years.is_finite() => {}
                (None, None) => {}
                (Some(_), _) => {
                    return Err(at(
                        "ages, but weathers_in is not a positive number of years".into(),
                    ));
                }
                (None, Some(_)) => return Err(at("weathers_in without ages_to".into())),
            }
            tokens.push(Token {
                name: def.name.clone(),
                group: def.group,
                shade,
                roughness: def.roughness,
                metalness: def.metalness,
                pattern: def.pattern,
                wears_to: other("wears to", &def.wears_to)?,
                ages_to,
                weathers_in: def.weathers_in,
            });
        }
        let pairs: Vec<(String, String)> =
            ron_list(players).map_err(|e| format!("{PLAYERS_FILE}: {e}"))?;
        Ok(Self {
            tokens,
            players: named(PLAYERS_FILE, "player colours", pairs)?,
            skin: Vec::new(),
            hair: Vec::new(),
        })
    }

    /// The skin tone or hair colour a check names `skin <name>` or `hair <name>`.
    pub fn person(&self, what: &str) -> Option<Srgb> {
        let (list, name) = match what.split_once(' ')? {
            ("skin", name) => (&self.skin, name),
            ("hair", name) => (&self.hair, name),
            _ => return None,
        };
        list.iter().find(|(n, _)| n == name).map(|(_, c)| *c)
    }

    /// The index of token `name`.
    pub fn index(&self, name: &str) -> Option<usize> {
        self.tokens.iter().position(|t| t.name == name)
    }

    /// Token `name`.
    pub fn token(&self, name: &str) -> Option<&Token> {
        self.index(name).map(|k| &self.tokens[k])
    }
}

/// `pairs` of (name, hex) as colours, refused if empty, malformed or a name repeats.
fn named(
    file: &str,
    what: &str,
    pairs: Vec<(String, String)>,
) -> Result<Vec<(String, Srgb)>, String> {
    let mut list = Vec::with_capacity(pairs.len());
    for (name, hex) in pairs {
        let colour =
            Srgb::parse(&hex).ok_or_else(|| format!("{file}: {name}: bad colour {hex:?}"))?;
        if list.iter().any(|(n, _)| n == &name) {
            return Err(format!("{file}: {name} is listed twice"));
        }
        list.push((name, colour));
    }
    if list.is_empty() {
        return Err(format!("{file}: no {what}"));
    }
    Ok(list)
}
