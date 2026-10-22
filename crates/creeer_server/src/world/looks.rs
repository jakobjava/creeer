//! How the players' characters look and what clients draw each body as (G1.3e, ADR-0026
//! decisions 7 and 8). Each player gets the first of the sixteen player colours no one else
//! present has, the order players are given them (`art-style.md` §3.6), and wears the
//! starting outfit; they choose their skin tone and hair colour (`Command::Look`), until then
//! one picked from their name. A fallen character's segments say whose they are and which.

use super::{ServerWorld, people};
use creeer_net::entity::{EntityId, Look};
use creeer_sim::character::bones::STARTING_OUTFIT;

/// The player colours, skin tones and hair colours of the base pack (`data/art`): what a name
/// picks from. A client draws an index past its own lists as its last entry.
const COLOURS: u8 = 16;
const SKINS: u8 = 10;
const HAIRS: u8 = 9;

/// What a client draws a body as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Drawn {
    /// A thing, as its shape is.
    Thing,
    /// A player's character, standing or (its pelvis) fallen.
    Person {
        player: String,
        look: Look,
        fallen: bool,
        /// The tool in its right hand (G1.4c), if any, and whether it carries something
        /// (G1.4f).
        wields: Option<u8>,
        carries: Option<EntityId>,
    },
    /// Segment `index` of the fallen character whose avatar is `person`.
    Segment { person: EntityId, index: u8 },
    /// An item (G1.6c), in its material's colour.
    Item { colour: [u8; 3] },
}

/// A name's hash (FNV-1a): the same on every machine.
fn hash(name: &str) -> u32 {
    name.bytes().fold(0x811c_9dc5, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    })
}

impl ServerWorld {
    /// How `player`'s character looks: as chosen, or, the first time, the first colour free
    /// and a skin tone and hair colour from their name, in the starting outfit.
    pub fn look_of(&mut self, player: &str) -> Look {
        if let Some(look) = self.looks.get(player) {
            return look.clone();
        }
        let look = self.new_look(player);
        self.looks.insert(player.to_owned(), look.clone());
        look
    }

    /// The look `player` is first given.
    fn new_look(&self, player: &str) -> Look {
        let present: Vec<&str> = self.players.iter().flatten().map(String::as_str).collect();
        let taken: Vec<u8> = (self.looks.iter())
            .filter(|(name, _)| present.contains(&name.as_str()))
            .map(|(_, look)| look.colour)
            .collect();
        let colour = (0..COLOURS).find(|c| !taken.contains(c)).unwrap_or(0);
        let h = hash(player);
        #[allow(clippy::cast_possible_truncation)] // under ten and nine
        let look = Look {
            colour,
            skin: (h % u32::from(SKINS)) as u8,
            hair: ((h / u32::from(SKINS)) % u32::from(HAIRS)) as u8,
            outfit: STARTING_OUTFIT.iter().map(|&w| w.to_owned()).collect(),
        };
        look
    }

    /// `player` chose a skin tone and hair colour: whoever has their character is told again.
    pub fn set_look(&mut self, player: &str, skin: u8, hair: u8) {
        let mut look = self.look_of(player);
        (look.skin, look.hair) = (skin, hair);
        self.looks.insert(player.to_owned(), look);
        let avatar = (self.players.iter())
            .position(|p| p.as_deref() == Some(player))
            .map(|i| self.ids[i]);
        if let Some(id) = avatar {
            self.changed.push(id);
        }
    }

    /// What body `index` is drawn as.
    pub fn drawn(&self, index: usize) -> Drawn {
        let id = self.ids[index];
        if let Some(player) = self.players[index].clone() {
            let fallen = self.people.get(&id).is_some_and(people::Person::fallen);
            return Drawn::Person {
                // Given when the avatar was (`avatar_of`).
                look: (self.looks.get(&player).cloned()).unwrap_or_else(|| self.new_look(&player)),
                wields: self.wields.get(&player).copied(),
                carries: self.carried(id),
                player,
                fallen,
            };
        }
        for (person, p) in &self.people {
            if let Some(k) = p.parts.iter().position(|e| *e == id) {
                return Drawn::Segment {
                    person: *person,
                    index: u8::try_from(k).expect("fifteen segments"),
                };
            }
        }
        self.item_colour(id)
            .map_or(Drawn::Thing, |colour| Drawn::Item { colour })
    }
}

impl ServerWorld {
    /// `player` took tool `tool` in hand (an index into the tools' table), or put it away:
    /// whoever has their character is told again.
    ///
    /// # Errors
    /// If there is no such tool, or (in survival) none made.
    pub fn set_wield(&mut self, player: &str, tool: Option<u8>) -> Result<(), String> {
        // In survival a player holds only what they made (G1.6d), not the catalogue's tools.
        if !self.creative && tool.is_some() {
            return Err("no such tool to hand: make one".into());
        }
        match tool {
            Some(t) if usize::from(t) >= self.tools.len() => {
                return Err(format!("no tool {t} ({} known)", self.tools.len()));
            }
            Some(t) => {
                self.wields.insert(player.to_owned(), t);
            }
            None => {
                self.wields.remove(player);
            }
        }
        let avatar = (self.players.iter())
            .position(|p| p.as_deref() == Some(player))
            .map(|i| self.ids[i]);
        if let Some(id) = avatar {
            self.changed.push(id);
        }
        Ok(())
    }
}
