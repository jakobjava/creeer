//! The table of engineering materials, from `data/materials/*.ron`.

use crate::engineering::{Material, MaterialDef};

/// Every engineering material, by id.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaterialTable {
    materials: Vec<Material>,
}

/// The reference data shipped with the game.
const REFERENCE: &str = include_str!("../../../data/materials/engineering.ron");

impl MaterialTable {
    /// Reads a materials file.
    ///
    /// # Errors
    /// If it does not parse, a value is invalid, or an id repeats.
    pub fn parse(text: &str) -> Result<Self, String> {
        let defs: Vec<MaterialDef> = ron::from_str(text).map_err(|e| format!("materials: {e}"))?;
        let mut table = Self::default();
        for def in defs {
            let id = def.id().to_owned();
            if table.get(&id).is_some() {
                return Err(format!("material {id} defined twice"));
            }
            let material = def.build().map_err(|e| format!("material {id}: {e}"))?;
            table.materials.push(material);
        }
        Ok(table)
    }

    /// The materials the game ships with.
    ///
    /// # Panics
    /// If the shipped data is broken (a test keeps it whole).
    pub fn reference() -> Self {
        Self::parse(REFERENCE).expect("the shipped materials parse")
    }

    /// The material `id`.
    pub fn get(&self, id: &str) -> Option<&Material> {
        self.materials.iter().find(|m| m.id == id)
    }

    /// All materials.
    pub fn all(&self) -> &[Material] {
        &self.materials
    }
}
