//! The players' characters as the client draws them (G1.3e, ADR-0026 decisions 6–8): the
//! engineer skinned on his ragdoll's fifteen bones, in his look's outfit and colours. Standing,
//! every bone is the body's frame (G1.3g animates the limbs); fallen, each bone is its segment.

use creeer_art::Palette;
use creeer_art::model::{Entry, Model};
use creeer_art::wearable::{Wearable, visible};
use creeer_math::Frame64;
use creeer_net::entity::Look;
use creeer_render::skin::{OutfitId, SkinnedId, SkinnedMeshData};
use creeer_render::{Character, Material, Renderer};
use creeer_sim::character::bones::{SEGMENT_NAMES, STARTING_OUTFIT};
use std::collections::HashMap;

/// The grey the player's colour is laid on at half strength (`art-style.md` §3.3, FFF-218;
/// `art/engineer/outfit.py`).
const JACKET_BASE: [u8; 3] = [0x5c, 0x5a, 0x57];
const JACKET_STRENGTH: f64 = 0.5;

/// A person to draw: how they look, and each segment's bone (the ragdoll's order).
#[derive(Clone, Debug, PartialEq)]
pub struct DrawnPerson {
    pub look: Look,
    pub bones: Vec<Frame64>,
}

/// The engineer, registered with a renderer, and his outfits.
pub struct People {
    palette: Palette,
    model: Model,
    wearables: Vec<Wearable>,
    id: SkinnedId,
    /// For each of the model's bones, the ragdoll segment it is.
    segment_of: Vec<usize>,
    outfits: HashMap<Vec<String>, OutfitId>,
}

impl std::fmt::Debug for People {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("People")
            .field("outfits", &self.outfits.len())
            .finish_non_exhaustive()
    }
}

impl People {
    /// Loads the engineer from the packs into `renderer`, with the outfits worn so far: the
    /// starting outfit and the base layer alone ("naked").
    ///
    /// # Errors
    /// If the packs have no engineer, or his bones are not the ragdoll's segments.
    pub fn register(renderer: &mut Renderer) -> Result<Self, String> {
        let data = crate::content::data();
        let palette = Palette::from_data(&data)?;
        let entry = (Entry::all(&data)?.into_iter())
            .find(|e| e.name == "engineer")
            .ok_or("the packs have no engineer")?;
        let bytes = data.binary(&entry.file).map_err(|e| e.to_string())?;
        let model = Model::read(bytes, &palette)?;
        let segment_of = (model.bones.iter())
            .map(|b| SEGMENT_NAMES.iter().position(|s| *s == b.name))
            .collect::<Option<Vec<usize>>>()
            .filter(|v| v.len() == SEGMENT_NAMES.len())
            .ok_or("the engineer's bones are not the ragdoll's segments")?;
        let levels = entry.levels.ok_or("the engineer's levels have no errors")?;
        let id = renderer.add_skinned(&SkinnedMeshData::new(&model, levels));
        let mut people = Self {
            palette,
            wearables: Wearable::all(&data)?,
            model,
            id,
            segment_of,
            outfits: HashMap::new(),
        };
        for worn in [&STARTING_OUTFIT[..], &["base_layer"]] {
            people.add_outfit(renderer, worn);
        }
        Ok(people)
    }

    /// Makes the outfit `worn` (wearables by name) drawable: its visible parts merged into one
    /// index list a level (`wearable::visible`). Once per outfit.
    pub fn add_outfit(&mut self, renderer: &mut Renderer, worn: &[&str]) {
        let names: Vec<String> = worn.iter().map(|&w| w.to_owned()).collect();
        if !self.outfits.contains_key(&names) {
            let parts = visible(&self.model, &self.wearables, worn);
            let outfit = renderer.add_outfit(self.id, &parts);
            self.outfits.insert(names, outfit);
        }
    }

    /// `person` as the renderer draws them. An outfit not registered is drawn as the starting
    /// one (outfits change from G1.6 on, when wearables are items).
    pub fn character(&self, person: &DrawnPerson) -> Character {
        let start: Vec<String> = STARTING_OUTFIT.iter().map(|&w| w.to_owned()).collect();
        let outfit = (self.outfits.get(&person.look.outfit))
            .or_else(|| self.outfits.get(&start))
            .copied()
            .expect("the starting outfit is registered");
        Character {
            outfit,
            bones: (self.segment_of.iter())
                .map(|&s| person.bones.get(s).copied().unwrap_or(Frame64::IDENTITY))
                .collect(),
            colours: self.colours(&person.look),
            material: Material::dielectric([1.0; 3], 0.8),
        }
    }

    /// The linear colours of `look`'s jacket, skin and hair: the player's colour laid on the
    /// jacket's grey at half strength, as the preview does; an index past a list its last.
    pub fn colours(&self, look: &Look) -> [[f32; 3]; 3] {
        let pick = |list: &[(String, creeer_art::colour::Srgb)], k: u8| {
            list.get(usize::from(k))
                .or(list.last())
                .map_or([0.5; 3], |(_, c)| c.linear())
        };
        let grey = creeer_art::colour::Srgb(JACKET_BASE).linear();
        let player = pick(&self.palette.players, look.colour);
        let jacket = std::array::from_fn(|i| grey[i] + (player[i] - grey[i]) * JACKET_STRENGTH);
        #[allow(clippy::cast_possible_truncation)] // colours to the GPU's f32
        let narrow = |c: [f64; 3]| c.map(|x| x as f32);
        [
            narrow(jacket),
            narrow(pick(&self.palette.skin, look.skin)),
            narrow(pick(&self.palette.hair, look.hair)),
        ]
    }
}
