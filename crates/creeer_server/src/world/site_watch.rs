//! What a creative server shows of its built structures (G2.11, ADR-0037; plan.md principle 13:
//! debug views are a creative or test server's information, never a survival tool): what each
//! member and joint carries (the stress overlay), and, when one breaks, what it was, where and
//! what it carried, told in words to the players near (a note) and kept for the overlay to mark.
//!
//! After each step and before a broken structure falls apart (`step.rs`), each awake built
//! structure's elements are read (`Structure::carried`); one broken since the step before is a
//! failure, told with what it carried in that step before.

use super::*;
use creeer_physics::structure::Carried;
use creeer_sim::items::{Form, Item};

/// How many failures are kept, and how far from one players are told of it (m).
const KEPT: usize = 16;
const TOLD: f64 = 40.0;
/// Failures within this of the first of a collapse belong to it (ticks: 2 s): only the first is
/// told.
pub const COLLAPSE: u64 = 120;

/// What the watch keeps.
#[derive(Debug, Default)]
pub(crate) struct Watch {
    /// Each built structure's elements as of the last step it was awake in.
    last: BTreeMap<EntityId, Vec<Carried>>,
    failures: Vec<Failure>,
    /// The kit's parts by their sections, to name members: name, material, the section's
    /// larger half extent and its thickness (m).
    names: Option<Vec<(String, String, f64, f64)>>,
}

/// Something that broke in a built structure.
#[derive(Clone, Debug, PartialEq)]
pub struct Failure {
    /// The tick it broke in, the structure it was of and where it was.
    pub tick: u64,
    pub structure: EntityId,
    pub at: DVec3,
    /// A joint (else a member).
    pub joint: bool,
    /// What it was and what it carried, in words: "the joint between a stringer and a deck board
    /// pulled apart: 1.9 kN of pull, 0.3 kN across".
    pub words: String,
}

/// What a creative server shows a player of its built structures.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inspection {
    /// Each built structure's elements' shares of what breaks them (0–1, in its elements'
    /// order: as its `Frame`'s members).
    pub shares: Vec<(EntityId, Vec<f32>)>,
    /// What failed, the oldest first.
    pub failures: Vec<Failure>,
}

use std::collections::BTreeMap;

impl ServerWorld {
    /// After the step, before what broke falls apart: the built structures read, what broke
    /// since the step before kept and told. A creative server's alone (nothing on others).
    pub(crate) fn watch_failures(&mut self) {
        if !self.creative {
            return;
        }
        let built: Vec<EntityId> = self.building.built.keys().copied().collect();
        let mut new = Vec::new();
        for id in built {
            let Some(i) = self.index(id) else { continue };
            let body = &self.world.bodies[i];
            if body.sleep.asleep && self.watch.last.contains_key(&id) {
                continue;
            }
            let now = body.structure.carried();
            if let Some(before) = self.watch.last.get(&id).filter(|b| b.len() == now.len()) {
                for (k, (b, n)) in before.iter().zip(&now).enumerate() {
                    if n.broken && !b.broken {
                        let s = &body.structure;
                        let at = (s.position[n.a] + s.position[n.b]) * 0.5;
                        new.push((id, k, *b, at));
                    }
                }
            }
            self.watch.last.insert(id, now);
        }
        self.watch.last.retain(|id, _| self.index.contains_key(id));
        for (id, k, carried, at) in new {
            let words = self.failure_words(id, k, &carried);
            let first = !(self.watch.failures.iter()).any(|f| self.tick - f.tick < COLLAPSE);
            if first {
                self.tell_near(at, &words);
            }
            self.watch.failures.push(Failure {
                tick: self.tick,
                structure: id,
                at,
                joint: carried.joint,
                words,
            });
        }
        let over = self.watch.failures.len().saturating_sub(KEPT);
        self.watch.failures.drain(..over);
    }

    /// What a creative server shows: the failures kept, and with `shares` each built
    /// structure's elements' shares (the overlay's, as of the last step it was awake in). On
    /// other servers, nothing.
    pub fn inspection(&self, shares: bool) -> Inspection {
        if !self.creative {
            return Inspection::default();
        }
        Inspection {
            shares: (self.watch.last.iter())
                .filter(|_| shares)
                .map(|(id, c)| {
                    #[allow(clippy::cast_possible_truncation)] // shown
                    let s = c.iter().map(|c| c.share as f32).collect();
                    (*id, s)
                })
                .collect(),
            failures: self.watch.failures.clone(),
        }
    }

    /// Every player within [`TOLD`] of `at` is told `words`.
    fn tell_near(&mut self, at: DVec3, words: &str) {
        let near: Vec<EntityId> = (self.avatars().into_iter())
            .filter(|&a| {
                (self.index(a)).is_some_and(|i| {
                    (self.world.bodies[i].structure.position[0] - at).length() < TOLD
                })
            })
            .collect();
        for avatar in near {
            self.note(avatar, words.to_owned());
        }
    }

    /// Element `k` of built structure `id`, broken, in words with what it carried before.
    fn failure_words(&mut self, id: EntityId, k: usize, c: &Carried) -> String {
        let kn = |n: f64| format!("{:.1} kN", n.abs() * 1e-3);
        if c.joint {
            let names: Vec<String> = [c.a, c.b]
                .iter()
                .filter_map(|&node| self.part_at(id, node, k))
                .collect();
            let between = match names.as_slice() {
                [a, b] => format!("the joint between {a} and {b}"),
                [a] => format!("the joint holding {a}"),
                _ => "a joint".to_owned(),
            };
            let how = if c.slipped >= 0.9 {
                "slipped until it tore"
            } else {
                "pulled apart"
            };
            let along = if c.along > 0.0 {
                format!("{} of pull, ", kn(c.along))
            } else {
                String::new()
            };
            format!("{between} {how}: {along}{} across", kn(c.across))
        } else {
            let part = self.part_named(id, k).unwrap_or_else(|| "a member".into());
            let axial = if c.along.abs() > 1000.0 {
                let way = if c.along > 0.0 {
                    "tension"
                } else {
                    "compression"
                };
                format!(", {} {way}", kn(c.along))
            } else {
                String::new()
            };
            format!(
                "{part} broke: {:.1} kN·m bending{axial}",
                c.across.abs() * 1e-3
            )
        }
    }

    /// The part a member at node `node` of structure `id` is (not element `not`), named.
    fn part_at(&mut self, id: EntityId, node: usize, not: usize) -> Option<String> {
        let i = self.index(id)?;
        let s = &self.world.bodies[i].structure;
        let e = (0..s.element_count())
            .filter(|&e| e != not)
            .find(|&e| s.member(e).is_some_and(|(a, b, _)| a == node || b == node))?;
        self.part_named(id, e)
    }

    /// Member `e` of structure `id` named as the kit's part of its section and wood ("a
    /// stringer"), or by its section ("a 150 × 75 mm timber").
    fn part_named(&mut self, id: EntityId, e: usize) -> Option<String> {
        let i = self.index(id)?;
        let s = &self.world.bodies[i].structure;
        let (_, _, thickness) = s.member(e)?;
        let half = s.members().get(e)?.2;
        let wood = self
            .building
            .built
            .get(&id)?
            .woods
            .get(e)
            .cloned()
            .flatten();
        let names = self
            .watch
            .names
            .get_or_insert_with(|| kit_sections(&self.data));
        let close = |a: f64, b: f64| (a - b).abs() < 0.002;
        let named = names.iter().find(|(_, m, h, t)| {
            close(*h, half) && close(*t, thickness) && wood.as_deref().is_none_or(|w| w == m)
        });
        Some(named.map_or_else(
            || {
                let mm = |m: f64| (m * 1000.0).round();
                format!("a {} × {} mm timber", mm(2.0 * half), mm(thickness))
            },
            |(n, ..)| format!("a {n}"),
        ))
    }
}

/// The creative kit's items by their sections: name, material, the section's larger half
/// extent and its thickness (m), as a member made of each has them.
fn kit_sections(data: &creeer_data::Data) -> Vec<(String, String, f64, f64)> {
    let (Ok(kit), Ok(densities)) = (
        creeer_sim::items::kit::kit(data),
        creeer_sim::items::kit::Densities::from_data(data),
    ) else {
        return Vec::new();
    };
    (kit.iter())
        .filter_map(|k| {
            let Ok(Item::Piece(p, _)) = densities.make(k) else {
                return None;
            };
            let (half, thickness) = match p.form {
                Form::Block { size } => (0.5 * size.y.max(size.z), size.y.min(size.z)),
                Form::Rod { radius, .. } => (radius, 2.0 * radius),
                _ => return None,
            };
            Some((k.name.clone(), k.material.clone(), half, thickness))
        })
        .collect()
}
