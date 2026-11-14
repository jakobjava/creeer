//! What sounds are made of (`data/sound/`, ADR-0037 decision 1): every engineering material as
//! it rings (its E, G and density, its loss factor), how it cracks and creaks, the grounds, what
//! drives a fastener, and how the game hears.

use crate::acoustic::{Acoustic, pressure};
use crate::mixer::Settings;
use creeer_data::Data;
use creeer_data::merge::ron_list;
use creeer_materials::MaterialTable;
use serde::Deserialize;

/// How a material breaks (`materials.ron`).
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Crack {
    pub seconds: f64,
    pub grains: f64,
    pub exponent: f64,
    pub vibration: f64,
}

/// How a material creaks under load (`materials.ron`).
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Creak {
    pub onset: f64,
    pub rate: f64,
    pub energy: f64,
}

/// An entry of `materials.ron`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundEntry {
    pub material: String,
    #[serde(default)]
    pub loss: Option<f64>,
    #[serde(default)]
    pub crack: Option<Crack>,
    #[serde(default)]
    pub creak: Option<Creak>,
}

/// A ground underfoot (`grounds.ron`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ground {
    pub rock: String,
    pub modulus: f64,
    pub grain: f64,
    pub grains: f64,
    pub shift: f64,
    pub grain_modulus: f64,
}

/// What drives a fastener home (`drivers.ron`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Driver {
    pub fastener: String,
    pub head: String,
    pub mass: f64,
    pub size: (f64, f64, f64),
    pub speed: f64,
    pub blows: u32,
}

/// The heel's strike (`hearing.ron`).
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Heel {
    pub mass: f64,
    pub speed: f64,
    pub sole: f64,
}

/// The wind (`hearing.ron`).
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wind {
    pub speed: f64,
    pub roughness: f64,
    pub level: f64,
}

/// How the game hears (`hearing.ron`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hearing {
    pub voices: usize,
    pub floor: f64,
    pub headroom: f64,
    pub window: f64,
    pub release: f64,
    pub threshold: f64,
    pub range: f64,
    pub structures: String,
    pub tools: String,
    pub heel: Heel,
    pub wind: Wind,
    pub default_crack: Crack,
}

/// A material as it sounds.
#[derive(Clone, Debug, PartialEq)]
pub struct Matter {
    pub id: String,
    pub acoustic: Acoustic,
    /// Its strength (Pa): the stress it breaks at (for the strain energy a breaking member held).
    pub strength: f64,
    pub crack: Crack,
    pub creak: Option<Creak>,
}

/// Everything sounds are made of.
#[derive(Clone, Debug, PartialEq)]
pub struct Library {
    pub matters: Vec<Matter>,
    pub grounds: Vec<Ground>,
    pub drivers: Vec<Driver>,
    pub hearing: Hearing,
}

/// Room temperature (°C): the material's constants taken there.
const ROOM: f64 = 20.0;

impl Library {
    /// The sound data of `data`'s packs.
    ///
    /// # Errors
    /// If a file does not load, or an entry names what is not there.
    pub fn from_data(data: &Data) -> Result<Self, String> {
        let materials = MaterialTable::from_data(data)?;
        let entries = (data.table("sound materials", "sound/materials.ron"))
            .merge(ron_list, |e: &SoundEntry| &e.material)
            .map_err(|e| e.to_string())?;
        let grounds = (data.table("sound grounds", "sound/grounds.ron"))
            .merge(ron_list, |g: &Ground| &g.rock)
            .map_err(|e| e.to_string())?;
        let drivers = (data.table("sound drivers", "sound/drivers.ron"))
            .merge(ron_list, |d: &Driver| &d.fastener)
            .map_err(|e| e.to_string())?;
        let hearing: Hearing =
            ron::from_str(data.file("sound/hearing.ron").map_err(|e| e.to_string())?)
                .map_err(|e| format!("sound/hearing.ron: {e}"))?;
        for e in &entries {
            if materials.get(&e.material).is_none() {
                return Err(format!("sound/materials.ron: no material {}", e.material));
            }
        }
        let matters = (materials.all().iter())
            .map(|m| {
                let entry = entries.iter().find(|e| e.material == m.id);
                let s = m.structural(ROOM + 273.15);
                Matter {
                    id: m.id.clone(),
                    acoustic: Acoustic {
                        youngs: s.youngs_modulus,
                        shear: s.shear_modulus,
                        density: s.density,
                        loss: entry.and_then(|e| e.loss).unwrap_or(s.damping),
                    },
                    strength: s.ultimate_strength,
                    crack: entry.and_then(|e| e.crack).unwrap_or(hearing.default_crack),
                    creak: entry.and_then(|e| e.creak),
                }
            })
            .collect();
        let library = Self {
            matters,
            grounds,
            drivers,
            hearing,
        };
        library.check()?;
        Ok(library)
    }

    /// The base pack's.
    ///
    /// # Panics
    /// If the shipped sound data is broken (a test keeps it whole).
    pub fn base() -> Self {
        Self::from_data(&Data::base()).expect("the shipped sound data loads")
    }

    /// Every entry within its range, and what it names there.
    fn check(&self) -> Result<(), String> {
        let h = &self.hearing;
        for name in [&h.structures, &h.tools] {
            self.matter(name)
                .ok_or(format!("sound/hearing.ron: no material {name}"))?;
        }
        if h.voices == 0 || h.voices > 128 || h.release <= 0.0 || h.range <= 0.0 {
            return Err("sound/hearing.ron: voices 1–128, release and range above 0".into());
        }
        if !self.grounds.iter().any(|g| g.rock == "default") {
            return Err("sound/grounds.ron: no default ground".into());
        }
        for g in &self.grounds {
            if g.modulus <= 0.0 || g.grain <= 0.0 || g.grains < 0.0 || g.grain_modulus <= 0.0 {
                return Err(format!("sound/grounds.ron: {} out of range", g.rock));
            }
        }
        for d in &self.drivers {
            self.matter(&d.head)
                .ok_or(format!("sound/drivers.ron: no material {}", d.head))?;
            if d.mass <= 0.0 || d.speed <= 0.0 || d.blows == 0 {
                return Err(format!("sound/drivers.ron: {} out of range", d.fastener));
            }
        }
        for m in &self.matters {
            let c = m.crack;
            let a = m.acoustic;
            if !(a.loss > 0.0 && a.loss < 1.0)
                || c.seconds <= 0.0
                || c.grains < 1.0
                || c.exponent <= 1.0
                || c.exponent >= 2.0
                || !(0.0..=1.0).contains(&c.vibration)
            {
                return Err(format!("sound: material {} out of range", m.id));
            }
            if let Some(k) = m.creak
                && (!(0.0..1.0).contains(&k.onset) || k.rate <= 0.0 || k.energy <= 0.0)
            {
                return Err(format!("sound: {}'s creak out of range", m.id));
            }
        }
        Ok(())
    }

    /// The material `id`.
    pub fn matter(&self, id: &str) -> Option<&Matter> {
        self.matters.iter().find(|m| m.id == id)
    }

    /// The material nearest a body's Young's modulus (Pa) and density (kg/m³), by their ratios:
    /// what a body the client is told only these of is made of.
    pub fn nearest(&self, youngs: f64, density: f64) -> &Matter {
        let off = |m: &Matter| {
            let e = (youngs.max(1.0) / m.acoustic.youngs).ln();
            let d = (density.max(1.0) / m.acoustic.density).ln();
            e * e + d * d
        };
        (self.matters.iter())
            .min_by(|a, b| off(a).total_cmp(&off(b)))
            .expect("the materials are never empty (check)")
    }

    /// The ground of rock `name` (or the default).
    pub fn ground(&self, name: &str) -> &Ground {
        (self.grounds.iter().find(|g| g.rock == name))
            .or_else(|| self.grounds.iter().find(|g| g.rock == "default"))
            .expect("a default ground (check)")
    }

    /// What drives fastener `name`, if anything does.
    pub fn driver(&self, name: &str) -> Option<&Driver> {
        self.drivers.iter().find(|d| d.fastener == name)
    }

    /// The material built structures sound as.
    pub fn structures(&self) -> &Matter {
        self.matter(&self.hearing.structures).expect("checked")
    }

    /// The material tools' heads sound as.
    pub fn tools(&self) -> &Matter {
        self.matter(&self.hearing.tools).expect("checked")
    }

    /// The mixer's settings.
    #[allow(clippy::cast_possible_truncation)] // Pa and factors
    pub fn settings(&self) -> Settings {
        let h = &self.hearing;
        Settings {
            voices: h.voices,
            floor: pressure(h.floor) as f32,
            headroom: 10f64.powf(h.headroom / 20.0) as f32,
            window: 10f64.powf(h.window / 20.0) as f32,
            release: h.release as f32,
            threshold: pressure(h.threshold) as f32,
        }
    }
}
