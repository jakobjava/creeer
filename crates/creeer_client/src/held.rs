//! Tools in hand (G1.4c, ADR-0027 decision 3): the packs' tools (`creeer_sim::tools`), each
//! drawn by its model held in a character's right hand, its handle where the fingers close on
//! it (`animate::hands`). Which a character holds the server says (`Kind::Character::wields`).

use crate::animate::hands::Hands;
use creeer_math::Frame64;
use creeer_math::glam::{DQuat, DVec3, Vec3};
use creeer_render::{Instance, Material, MeshArena, MeshId};
use creeer_sim::tools::Tool;

/// The packs' tools and their models' meshes, in the tools' table's order.
#[derive(Debug, Default)]
pub struct Held {
    tools: Vec<(Tool, Option<MeshId>)>,
}

impl Held {
    /// The packs' tools, their models registered in `meshes`.
    pub fn register(meshes: &mut MeshArena) -> Self {
        let tools = Tool::all(&crate::content::data()).unwrap_or_default();
        let tools = (tools.into_iter())
            .map(|t| {
                let mesh = crate::models::load(meshes, &t.model).map(|(id, _)| id);
                (t, mesh)
            })
            .collect();
        Self { tools }
    }

    /// Tool `index` of the table, if there is one.
    pub fn tool(&self, index: u8) -> Option<&Tool> {
        self.tools.get(usize::from(index)).map(|(t, _)| t)
    }

    /// Tool `index` as drawn carried in the right hand of `hands`, whose bone's frame is `hand`:
    /// held at its balance (the model's origin is at its swing's grip, its handle along +y).
    pub fn instance(&self, index: u8, hands: &Hands, hand: Frame64) -> Option<(MeshId, Instance)> {
        let (tool, mesh) = self.tools.get(usize::from(index))?;
        Some((
            (*mesh)?,
            Instance {
                frame: hand
                    * hands.grip(0, tool.carry_radius)
                    * Frame64::new(
                        DVec3::new(0.0, tool.grip - tool.carry, 0.0),
                        DQuat::IDENTITY,
                    ),
                scale: Vec3::ONE,
                material: Material::dielectric([1.0; 3], 0.8),
                heat: None,
            },
        ))
    }
}

impl Held {
    /// Where in its swing (`animate::upper::swing`) a character holding tool `index` is, `seconds`
    /// after the swing began: raising until its downswing (−1 to 0), striking (0 to 1, the head's
    /// angle as the square of the time: its spin builds evenly), freeing the tool after the blow
    /// (1 to 2); none past its cycle.
    pub fn swing_progress(&self, index: u8, seconds: f64) -> Option<f64> {
        use creeer_sim::strike::{CYCLE, TO_BLOW, swing};
        let s = swing(self.tool(index)?);
        Some(match seconds {
            t if !(0.0..CYCLE).contains(&t) => return None,
            t if t < s.raise => t / s.raise - 1.0,
            t if t < TO_BLOW => {
                let x = (t - s.raise) / s.strike;
                x * x
            }
            t => 1.0 + (t - TO_BLOW) / (CYCLE - TO_BLOW),
        })
    }
}
