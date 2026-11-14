//! The meshes things are drawn with (G2.11, ADR-0037): each wooden token's worked round part and
//! sawn board, each species' log, deadwood and stones, registered once from the palette and the
//! species (`creeer_render::parts`), found by what a thing is made of.

use creeer_art::Palette;
use creeer_art::palette::{Group, Pattern};
use creeer_render::parts::{self, Faces};
use creeer_render::{Grain, MeshArena, MeshId};
use std::collections::BTreeMap;

/// The token a log's cut ends and a stump are drawn in (`art-style.md` §3.4).
const CUT: &str = "wood_cut";
/// The token deadwood lying about is drawn in: old, greyed wood.
const DEADWOOD: &str = "timber_weathered";
/// The stone pattern's number in the shader (`surface.wgsl`'s `PATTERN_STONE`).
const STONE: u8 = 4;

/// The things' meshes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PartMeshes {
    /// Each wooden token's part worked round (a pole, a haft) and sawn (a board, a beam).
    round: BTreeMap<u16, MeshId>,
    board: BTreeMap<u16, MeshId>,
    /// Each species' log (by its index in the forest): its bark and marks on its side, cut wood
    /// on its ends; and its foliage's clump, tier and spire.
    logs: Vec<MeshId>,
    foliage: Vec<[MeshId; 3]>,
    /// A dead branch lying about.
    pub deadwood: Option<MeshId>,
    /// A stone, in its instance's colour (its rock's), speckled.
    pub stone: Option<MeshId>,
    /// The materials' tokens by their colour (an item's colour is its material's token's: what
    /// it is made of shows, `creeer_sim::items::kit::Densities::colour`).
    materials: Vec<([u8; 3], u16)>,
}

impl PartMeshes {
    /// Registers the meshes of `palette`'s wooden tokens and of `species` in `arena`.
    pub fn register(
        arena: &mut MeshArena,
        palette: &Palette,
        species: &[creeer_sim::trees::Species],
    ) -> Self {
        let mut out = Self::default();
        let index = |name: &str| palette.index(name).and_then(|k| u16::try_from(k).ok());
        for (k, t) in palette.tokens.iter().enumerate() {
            let Ok(k) = u16::try_from(k) else { continue };
            if t.pattern == Pattern::Wood {
                out.round
                    .insert(k, arena.add_levels(&parts::cylinder(Faces::round(k)), round()));
                out.board.insert(
                    k,
                    arena.add_levels(&parts::board(k), creeer_render::mesh::Round::Box),
                );
            }
            if let (Group::Material, Some(colour)) = (t.group, t.shade.fixed()) {
                out.materials.push((colour.0, k));
            }
        }
        let cut = index(CUT);
        for s in species {
            let log = (index(&s.tokens.bark).zip(cut)).map(|(bark, cut)| {
                let marks = s.tokens.marks.as_deref().and_then(index);
                arena.add_levels(&parts::cylinder(Faces::log(bark, cut, marks)), round())
            });
            // A species whose tokens are missing is drawn as the first's (never: the packs check).
            if let Some(log) = log.or_else(|| out.logs.first().copied()) {
                out.logs.push(log);
            }
            let sphere = creeer_render::mesh::Round::Sphere;
            let leaves = index(&s.tokens.leaves).map(|t| {
                use creeer_render::foliage::{clump_levels, spire_levels, tier_levels};
                [clump_levels(t), tier_levels(t), spire_levels(t)]
                    .map(|levels| arena.add_levels(&levels, sphere))
            });
            if let Some(leaves) = leaves.or_else(|| out.foliage.first().copied()) {
                out.foliage.push(leaves);
            }
        }
        out.deadwood = index(DEADWOOD).map(|t| {
            let faces = Faces {
                side: (t, Grain::Bark),
                ends: (t, Grain::Round),
                marks: None,
            };
            arena.add_levels(&parts::cylinder(faces), round())
        });
        out.stone = Some(arena.add_levels(
            &parts::patterned_sphere(STONE),
            creeer_render::mesh::Round::Sphere,
        ));
        out
    }

    /// The token of the material whose colour is `colour`, if one is.
    pub fn material(&self, colour: [u8; 3]) -> Option<u16> {
        (self.materials.iter())
            .find(|(c, _)| *c == colour)
            .map(|(_, t)| *t)
    }

    /// A round part of the wooden material coloured `colour`, if it is one.
    pub fn round_of(&self, colour: [u8; 3]) -> Option<MeshId> {
        self.round.get(&self.material(colour)?).copied()
    }

    /// A sawn part of the wooden material coloured `colour`, if it is one.
    pub fn board_of(&self, colour: [u8; 3]) -> Option<MeshId> {
        self.board.get(&self.material(colour)?).copied()
    }

    /// Species `k`'s log.
    pub fn log(&self, k: usize) -> Option<MeshId> {
        self.logs.get(k).copied()
    }

    /// Species `k`'s foliage of shape `mass`.
    pub fn foliage(&self, k: usize, mass: crate::tree_draw::Mass) -> Option<MeshId> {
        use crate::tree_draw::Mass;
        let [clump, tier, spire] = *self.foliage.get(k)?;
        Some(match mass {
            Mass::Clump => clump,
            Mass::Tier => tier,
            Mass::Spire => spire,
        })
    }
}

/// The packs' things' meshes registered in `arena` (none if the palette does not load).
pub fn from_packs(arena: &mut MeshArena) -> PartMeshes {
    let data = crate::content::data();
    let species = creeer_sim::trees::species::load(&data).unwrap_or_default();
    crate::models::palette().map_or_else(PartMeshes::default, |palette| {
        PartMeshes::register(arena, &palette, &species)
    })
}

fn round() -> creeer_render::mesh::Round {
    creeer_render::mesh::Round::Cylinder
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The base pack's timber is found by its colour, as items are told; each species has its
    /// log; and no two materials share a colour (else one would be drawn as the other).
    #[test]
    fn things_find_their_meshes_by_what_they_are_made_of() {
        let data = creeer_data::Data::base();
        let palette = Palette::from_data(&data).expect("the palette");
        let species = creeer_sim::trees::species::load(&data).expect("the species");
        let mut arena = MeshArena::default();
        let meshes = PartMeshes::register(&mut arena, &palette, &species);
        let timber = palette.token("timber").and_then(|t| t.shade.fixed());
        let timber = timber.expect("timber's colour").0;
        assert!(meshes.round_of(timber).is_some() && meshes.board_of(timber).is_some());
        assert_eq!(meshes.logs.len(), species.len());
        assert!(meshes.deadwood.is_some() && meshes.stone.is_some());
        let granite = [0xc4, 0xa1, 0x94];
        assert!(meshes.round_of(granite).is_none(), "a rock is no material token");
        let colours: std::collections::BTreeSet<[u8; 3]> =
            meshes.materials.iter().map(|m| m.0).collect();
        assert_eq!(colours.len(), meshes.materials.len());
    }
}
