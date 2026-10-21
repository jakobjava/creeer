//! The authored models and the palette they name (G1.3b, ADR-0026), from the packs
//! ([`crate::content`]): registered in the renderer's arena, the palette uploaded with them.

use creeer_art::Palette;
use creeer_art::model::{Entry, Model};
use creeer_render::{MeshArena, MeshData, MeshId, Renderer};

/// The models the sandboxes draw, if the packs have them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Models {
    /// The 1 m crate, its origin at the centre of its base.
    pub crate_box: Option<MeshId>,
}

/// The palette of the current packs, if it loads.
pub fn palette() -> Option<Palette> {
    Palette::from_data(&crate::content::data()).ok()
}

/// Registers the packs' models in `arena`, their tokens indices into [`palette`].
pub fn register(arena: &mut MeshArena) -> Models {
    let data = crate::content::data();
    let Some(palette) = palette() else {
        return Models::default();
    };
    let entries = Entry::all(&data).unwrap_or_default();
    let mut load = |name: &str| {
        let entry = entries.iter().find(|e| e.name == name)?;
        let bytes = data.binary(&entry.file).ok()?;
        let model = Model::read(bytes, &palette).ok()?;
        Some(arena.add(&MeshData::from_model(&model)))
    };
    Models {
        crate_box: load("crate"),
    }
}

/// Uploads the packs' palette to `renderer`, for the models [`register`] added.
pub fn upload_palette(renderer: &mut Renderer) {
    if let Some(palette) = palette() {
        renderer.set_palette(&palette);
    }
}

impl crate::DemoMeshes {
    /// As [`crate::DemoMeshes::register`] into `renderer`'s arena, with the palette its models
    /// name.
    pub fn register_for(renderer: &mut Renderer) -> Self {
        upload_palette(renderer);
        Self::register(&mut renderer.meshes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crate_model_registers_from_the_base_pack() {
        let mut arena = MeshArena::default();
        let models = register(&mut arena);
        assert!(models.crate_box.is_some());
        assert_eq!(arena.len(), 1);
    }
}
