//! The players' characters as the client draws them (G1.3e, ADR-0026 decisions 6–8): the
//! engineer skinned on his ragdoll's fifteen bones and his fingers' thirty (G1.4b), in his look's
//! outfit and colours. Standing,
//! every bone is the body's frame (G1.3g animates the limbs); fallen, each bone is its segment.

use crate::animate::feet::Ground;
use crate::animate::hands::{Hands, Hold};
use crate::animate::{Animator, Body};
use creeer_art::Palette;
use creeer_art::model::{Entry, Model};
use creeer_art::wearable::{Wearable, visible};
use creeer_math::Frame64;
use creeer_net::entity::Look;
use creeer_render::skin::{OutfitId, SkinnedId, SkinnedMeshData};
use creeer_render::{Character, Material, Renderer};
use creeer_sim::character::bones::{SEGMENT_NAMES, STARTING_OUTFIT, finger_bones};
use std::collections::HashMap;

/// The grey the player's colour is laid on at half strength (`art-style.md` §3.3, FFF-218;
/// `art/engineer/outfit.py`).
const JACKET_BASE: [u8; 3] = [0x5c, 0x5a, 0x57];
const JACKET_STRENGTH: f64 = 0.5;

/// A person to draw: who (a key the animation knows them by), how they look, and their body.
#[derive(Clone, Debug, PartialEq)]
pub struct DrawnPerson {
    pub key: u32,
    pub look: Look,
    pub body: Body,
}

/// Where a bone of the model is drawn from: a segment of the ragdoll, or a finger bone
/// (`bones::finger_bones`' order).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Segment(usize),
    Finger(usize),
}

/// The engineer, registered with a renderer, and his outfits.
pub struct People {
    palette: Palette,
    model: Model,
    wearables: Vec<Wearable>,
    id: SkinnedId,
    /// For each of the model's bones, the ragdoll segment it is or the finger bone (G1.4b).
    bone_of: Vec<Source>,
    /// The fingers at rest, if the model has them.
    hands: Option<Hands>,
    outfits: HashMap<Vec<String>, OutfitId>,
    /// Their animation (G1.3g).
    animator: Animator,
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
        let fingers = finger_bones();
        let bone_of = (model.bones.iter())
            .map(|b| {
                (SEGMENT_NAMES
                    .iter()
                    .position(|s| *s == b.name)
                    .map(Source::Segment))
                .or_else(|| {
                    fingers
                        .iter()
                        .position(|f| *f == b.name)
                        .map(Source::Finger)
                })
            })
            .collect::<Option<Vec<Source>>>()
            .filter(|v| v.iter().filter(|s| matches!(s, Source::Segment(_))).count() == 15)
            .ok_or("the engineer's bones are not the ragdoll's segments and the fingers")?;
        let hands = Hands::new(&model.bones);
        let levels = entry.levels.ok_or("the engineer's levels have no errors")?;
        let id = renderer.add_skinned(&SkinnedMeshData::new(&model, levels));
        let mut people = Self {
            palette,
            wearables: Wearable::all(&data)?,
            model,
            id,
            bone_of,
            hands,
            outfits: HashMap::new(),
            animator: Animator::default(),
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

    /// The people as the renderer draws them this frame, `dt` seconds after the last, animated
    /// on `ground` (those no longer drawn are forgotten).
    pub fn characters(
        &mut self,
        people: &[DrawnPerson],
        dt: f64,
        ground: &dyn Ground,
    ) -> Vec<Character> {
        let keys: Vec<u32> = people.iter().map(|p| p.key).collect();
        self.animator.retain(&keys);
        people
            .iter()
            .map(|p| {
                let bones = self.animator.bones(p.key, &p.body, dt, ground);
                self.holding(&p.look, &bones, self.animator.holds(p.key))
            })
            .collect()
    }

    /// Someone looking `look` with `bones` (the ragdoll's order) as the renderer draws them. An
    /// outfit not registered is drawn as the starting one (outfits change from G1.6 on, when
    /// wearables are items).
    pub fn character(&self, look: &Look, bones: &[Frame64]) -> Character {
        self.holding(look, bones, [Hold::Relaxed; 2])
    }

    /// Someone looking `look` with `bones` (the ragdoll's order), the hands held `hold` (right,
    /// left), as the renderer draws them.
    pub fn holding(&self, look: &Look, bones: &[Frame64], hold: [Hold; 2]) -> Character {
        let start: Vec<String> = STARTING_OUTFIT.iter().map(|&w| w.to_owned()).collect();
        let outfit = (self.outfits.get(&look.outfit))
            .or_else(|| self.outfits.get(&start))
            .copied()
            .expect("the starting outfit is registered");
        let fingers = (self.hands.as_ref()).map_or_else(Vec::new, |h| h.pose(bones, hold));
        Character {
            outfit,
            bones: (self.bone_of.iter())
                .map(|source| match *source {
                    Source::Segment(s) => bones.get(s),
                    Source::Finger(f) => fingers.get(f),
                })
                .map(|b| b.copied().unwrap_or(Frame64::IDENTITY))
                .collect(),
            colours: self.colours(look),
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
