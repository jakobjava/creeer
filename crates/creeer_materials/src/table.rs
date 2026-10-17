//! The table of engineering materials, from the packs' `materials/*.ron`.

use crate::engineering::{Material, MaterialDef};
use creeer_data::Data;
use creeer_data::merge::ron_list;

/// Every engineering material, by id.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaterialTable {
    materials: Vec<Material>,
}

/// The reference data shipped with the game (the base pack's, compiled in for tests and
/// tools; a test keeps the two equal).
const REFERENCE: &str = include_str!("../../../data/materials/engineering.ron");

impl MaterialTable {
    /// Reads a materials file.
    ///
    /// # Errors
    /// If it does not parse, a value is invalid, or an id repeats.
    pub fn parse(text: &str) -> Result<Self, String> {
        let defs: Vec<MaterialDef> = ron::from_str(text).map_err(|e| format!("materials: {e}"))?;
        let mut ids: Vec<&str> = defs.iter().map(MaterialDef::id).collect();
        ids.sort_unstable();
        if let Some(pair) = ids.windows(2).find(|w| w[0] == w[1]) {
            return Err(format!("material {} defined twice", pair[0]));
        }
        Self::from_defs(defs)
    }

    /// The materials of every pack of `data` (table `materials`).
    ///
    /// # Errors
    /// As [`creeer_data::Table::merge`], or if a material is invalid.
    pub fn from_data(data: &Data) -> Result<Self, String> {
        let defs = (data.table("materials", "materials/*.ron"))
            .merge_traced(ron_list::<MaterialDef>, MaterialDef::id)
            .map_err(|e| e.to_string())?;
        let mut table = Self::default();
        for (def, origin) in defs {
            let id = def.id().to_owned();
            let material = def
                .build()
                .map_err(|e| format!("{origin}: material {id}: {e}"))?;
            table.materials.push(material);
        }
        Ok(table)
    }

    fn from_defs(defs: Vec<MaterialDef>) -> Result<Self, String> {
        let mut table = Self::default();
        for def in defs {
            let id = def.id().to_owned();
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
