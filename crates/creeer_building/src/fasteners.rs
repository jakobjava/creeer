//! The fasteners' catalogue (`data/fasteners.ron`, G2.2): each entry's kind, material and
//! sizes, and the joint law they make between two parts (`law.rs`).

use crate::law::{self, Dowel, Lashing, Placement, Side};
use creeer_data::Data;
use creeer_data::merge::ron_list;
use creeer_materials::{Material, MaterialTable};
use creeer_physics::structure::JointLaw;
use serde::Deserialize;

pub const TABLE: &str = "fasteners.ron";

/// What a fastener is (sizes in m).
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub enum Kind {
    Nail {
        diameter: f64,
        length: f64,
        head: f64,
    },
    Peg {
        diameter: f64,
        length: f64,
    },
    Lashing {
        width: f64,
        thickness: f64,
        wraps: u32,
        pretension: f64,
        friction: f64,
    },
}

/// A catalogue entry.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fastener {
    pub name: String,
    pub kind: Kind,
    pub material: String,
    /// A nail wire's own tensile strength (Pa), where it is not its material's.
    #[serde(default)]
    pub strength: Option<f64>,
    pub peak_slip: f64,
    pub failure_slip: f64,
}

/// Room temperature (K): the strengths a joint is made with.
const ROOM: f64 = 293.15;

impl Fastener {
    /// Every fastener of `data`'s packs.
    ///
    /// # Errors
    /// If the table does not load.
    pub fn all(data: &Data) -> Result<Vec<Self>, String> {
        (data.table("fasteners", TABLE))
            .merge(ron_list, |f: &Self| &f.name)
            .map_err(|e| e.to_string())
    }

    /// `count` of these, `at` as placed, through part `a` (its side the fasteners go in from)
    /// into `b`, their axes `between` (m) apart: the joint's law, or why they cannot hold.
    ///
    /// # Errors
    /// If its material is unknown or it cannot reach far enough into `b`.
    pub fn law(
        &self,
        materials: &MaterialTable,
        (a, b): (&Side, &Side),
        at: &Placement,
        between: f64,
    ) -> Result<JointLaw, String> {
        let m = materials
            .get(&self.material)
            .ok_or_else(|| format!("no material {}", self.material))?;
        let s = m.structural(ROOM);
        match self.kind {
            Kind::Nail {
                diameter,
                length,
                head,
            } => law::dowels(
                &Dowel {
                    diameter,
                    length,
                    strength: self.strength.unwrap_or(s.ultimate_strength),
                    driven: true,
                    head,
                    peak_slip: self.peak_slip,
                    failure_slip: self.failure_slip,
                },
                a,
                b,
                at,
                between,
            ),
            Kind::Peg { diameter, length } => law::dowels(
                &Dowel {
                    diameter,
                    length,
                    strength: self.strength.unwrap_or(s.yield_strength),
                    driven: false,
                    head: 0.0,
                    peak_slip: self.peak_slip,
                    failure_slip: self.failure_slip,
                },
                a,
                b,
                at,
                between,
            ),
            Kind::Lashing {
                width,
                thickness,
                wraps,
                pretension,
                friction,
            } => Ok(law::lashing(
                &Lashing {
                    area: width * thickness,
                    strength: self.strength.unwrap_or(s.ultimate_strength),
                    stiffness: s.youngs_modulus,
                    wraps,
                    pretension,
                    friction,
                    peak_slip: self.peak_slip,
                    failure_slip: self.failure_slip,
                },
                a,
                b,
                at,
                between,
            )),
        }
    }
}

/// A part of `wood` where fasteners meet it, `thickness` (m) along them: EN 1995-1-1's
/// density at 12 % moisture (a green wood's seasoned one), its strength and stiffness across
/// the grain (EN 338: E₉₀ = E₀/30).
///
/// # Errors
/// If `wood` has no strength across its grain (it is no wood).
pub fn side(wood: &Material, thickness: f64) -> Result<Side, String> {
    let s = wood.structural(ROOM);
    let across = wood
        .across
        .or(wood.compressive_strength.map(|c| c / 10.0))
        .ok_or_else(|| format!("{} has no grain to fasten into", wood.id))?;
    Ok(Side {
        density: wood.seasoned.unwrap_or(wood.density),
        across,
        across_stiffness: s.youngs_modulus / 30.0,
        thickness,
        damping: wood.damping,
    })
}

/// What is wrong with the catalogue: unknown materials, sizes and slips out of reason.
pub fn problems(fasteners: &[Fastener], materials: &MaterialTable) -> Vec<String> {
    let mut out = Vec::new();
    for f in fasteners {
        let at = &f.name;
        if materials.get(&f.material).is_none() {
            out.push(format!("fastener {at}: no material {}", f.material));
        }
        let sizes_ok = match f.kind {
            Kind::Nail {
                diameter,
                length,
                head,
            } => (0.001..0.02).contains(&diameter) && length > 10.0 * diameter && head > diameter,
            Kind::Peg { diameter, length } => {
                (0.005..0.06).contains(&diameter) && length > 4.0 * diameter
            }
            Kind::Lashing {
                width,
                thickness,
                wraps,
                pretension,
                friction,
            } => {
                width > 0.0
                    && thickness > 0.0
                    && wraps > 0
                    && (0.0..=1.0).contains(&pretension)
                    && (0.0..=1.5).contains(&friction)
            }
        };
        if !sizes_ok {
            out.push(format!("fastener {at}: sizes out of reason: {:?}", f.kind));
        }
        if !(f.peak_slip > 0.0 && f.failure_slip >= f.peak_slip && f.failure_slip < 0.2) {
            out.push(format!(
                "fastener {at}: slips {} m at its peak, {} m at failure",
                f.peak_slip, f.failure_slip
            ));
        }
        if f.strength.is_some_and(|s| !(1e6..5e9).contains(&s)) {
            out.push(format!("fastener {at}: strength {:?} Pa", f.strength));
        }
    }
    out
}
