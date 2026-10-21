//! The palette in data (`data/art/palette.ron`, `data/art/players.ron`): every token of
//! `docs/art-style.md` §3 and §10, which models and the renderer name instead of colours. A
//! pack recolours a token by naming it again (ADR-0018's tables); the player colours are one
//! file, replaced whole by the last pack that has it.

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
    /// The token that wear on convex edges reveals (painted steel wears to bare steel).
    pub wears_to: Option<usize>,
    /// The grounds against which its edge wear and occlusion carry the contrast its colour
    /// lacks (`art-style.md` §4), each approved by the user.
    pub carried_on: Vec<String>,
}

/// The palette: the tokens, in data order, and the player colours.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Palette {
    pub tokens: Vec<Token>,
    /// (name, colour) in the order players are given them.
    pub players: Vec<(String, Srgb)>,
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
    wears_to: Option<String>,
    #[serde(default)]
    carried_on: Vec<String>,
}

fn matte() -> f64 {
    1.0
}

/// Where the palette lives in a pack.
pub const TOKENS_FILE: &str = "art/palette.ron";
/// Where the player colours live in a pack.
pub const PLAYERS_FILE: &str = "art/players.ron";

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
        Self::build(defs, players)
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
            let wears_to = match &def.wears_to {
                None => None,
                Some(name) if name == &def.name => return Err(at("wears to itself".into())),
                Some(name) => Some(
                    defs.iter()
                        .position(|(d, _)| &d.name == name)
                        .ok_or_else(|| at(format!("wears to {name:?}, which is no token")))?,
                ),
            };
            tokens.push(Token {
                name: def.name.clone(),
                group: def.group,
                shade,
                roughness: def.roughness,
                metalness: def.metalness,
                wears_to,
                carried_on: def.carried_on.clone(),
            });
        }
        let pairs: Vec<(String, String)> =
            ron_list(players).map_err(|e| format!("{PLAYERS_FILE}: {e}"))?;
        let mut list = Vec::with_capacity(pairs.len());
        for (name, hex) in pairs {
            let colour = Srgb::parse(&hex)
                .ok_or_else(|| format!("{PLAYERS_FILE}: {name}: bad colour {hex:?}"))?;
            if list.iter().any(|(n, _)| n == &name) {
                return Err(format!("{PLAYERS_FILE}: {name} is listed twice"));
            }
            list.push((name, colour));
        }
        if list.is_empty() {
            return Err(format!("{PLAYERS_FILE}: no player colours"));
        }
        Ok(Self {
            tokens,
            players: list,
        })
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
