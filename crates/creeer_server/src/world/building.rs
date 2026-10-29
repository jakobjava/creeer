//! Building by hand (G2.3, `docs/g2-plan.md`, ADR-0033): two parts touching are fastened where
//! the player points, with a fastener of the catalogue (`data/fasteners.ron`), after the time
//! it takes. Fastened, they are one structure (`building_parts.rs`): a part's item becomes a
//! member, its fastenings joints with laws from the fasteners and the parts' woods.
//!
//! Survival: a lashing takes a bast strip from the pockets; nails need iron and pegs a bored
//! hole, which come with G3, so they are a creative server's. Nothing snaps, previews or
//! undoes (plan.md, principle 13): a part is fastened where it lies. A part touching a standing
//! tree's trunk is fastened to the tree alike (G2.9, `tree_joints.rs`).

use super::tree_joints::Touched;
use super::*;
use creeer_building::fasteners::{Fastener, Kind};
use creeer_materials::MaterialTable;
use creeer_sim::items::Item;
use std::collections::BTreeMap;

/// Ticks a second.
const TICKS: f64 = 60.0;
/// How far from the point aimed at two parts may touch to be fastened there (m): faces lying on
/// each other touch over their whole overlap, its contact's point anywhere in it.
pub(super) const NEAR: f64 = 0.3;
/// A gap two parts may have and still be fastened (m: they must touch, as near as a nail draws
/// them together; a part resting on another rocks by millimetres).
pub(super) const TOUCHING: f64 = 0.01;

/// What is being built, and what was.
#[derive(Debug, Default)]
pub(super) struct Building {
    pub(super) fasteners: Vec<Fastener>,
    pub(super) materials: Option<MaterialTable>,
    /// Each built structure: what it is made of.
    pub(super) built: BTreeMap<EntityId, Built>,
    /// Fastenings under way, by the player at work.
    pub(super) under_way: BTreeMap<EntityId, UnderWay>,
    /// Cuts into built members (G2.6): by structure and element, the wood taken out (m³).
    pub(super) cuts: BTreeMap<(EntityId, usize), f64>,
}

/// A built structure's record: the items its parts and fasteners were (for salvage, G2.6),
/// and each element's wood (an engineering material's id; none for a joint).
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Built {
    pub(super) items: Vec<Item>,
    pub(super) woods: Vec<Option<String>>,
}

/// Work under way on a part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Work {
    /// Fastening it to what it touches (G2.3).
    Fasten,
    /// Tacking it with a lashing's first turns while one hand holds it (G2.10).
    Tack,
    /// Drawing the rest of a tacked lashing's turns, both hands free (G2.10).
    Finish,
    /// Taking a joint of it apart (G2.6).
    Undo,
    /// Backfilling round it where it stands in a hole (G2.4).
    Post,
}

/// A fastening under way: the part aimed at, where, with which fastener, done at which tick.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct UnderWay {
    pub(super) part: EntityId,
    /// Where (the world's frame: parts settle by millimetres while the work goes on, and a part
    /// made a structure meanwhile has another node 0, so a point in its frame went astray).
    pub(super) at: DVec3,
    pub(super) fastener: usize,
    pub(super) done: u64,
    /// What is being done.
    pub(super) work: Work,
}

impl Building {
    pub(super) fn from_data(data: &creeer_data::Data) -> Self {
        Self {
            fasteners: Fastener::all(data).unwrap_or_default(),
            materials: MaterialTable::from_data(data).ok(),
            ..Self::default()
        }
    }
}

impl ServerWorld {
    /// Player `by` begins fastening the parts touching nearest `at` on `part` with one of
    /// `fastener`: refused unless they can (the parts, the fastener and their hands).
    pub(super) fn begin_fastening(
        &mut self,
        by: EntityId,
        part: EntityId,
        at: DVec3,
        fastener: &str,
    ) -> Result<(), String> {
        let k = (self.building.fasteners.iter())
            .position(|f| f.name == fastener)
            .ok_or_else(|| format!("no fastener {fastener}"))?;
        let work = self.fastening_work(part, at, k)?;
        let whole = &self.building.fasteners[k];
        let tack = whole.tacked().map_or(0.0, |t| t.seconds);
        let (seconds, doing) = match work {
            Work::Tack => (tack, "tacking"),
            Work::Finish => (whole.seconds - tack, "finishing the lashing"),
            _ => (whole.seconds, "fastening"),
        };
        if work != Work::Finish {
            // (A tack's strip was taken when it was made.)
            self.fastener_at_hand(by, k)?;
        }
        // (Validated under ten minutes: a few thousand ticks.)
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let done = self.tick + (seconds * TICKS).ceil() as u64;
        self.building.under_way.insert(
            by,
            UnderWay {
                part,
                at,
                fastener: k,
                done,
                work,
            },
        );
        self.note(by, format!("{doing}: {seconds:.0} s"));
        Ok(())
    }

    /// The part touching `part` nearest `at`, and the point between them.
    pub(super) fn touching_part(
        &self,
        part: EntityId,
        at: DVec3,
    ) -> Result<(EntityId, DVec3), String> {
        let i = self.index(part).ok_or("that part is gone")?;
        self.part_geometry(part)?;
        let touches = self.world.touching(i, TOUCHING);
        let (other, point) = touches
            .iter()
            .filter_map(|t| t.other.map(|(j, _)| (j, t.point)))
            .filter(|(j, p)| *j != i && (*p - at).length() < NEAR)
            .min_by(|x, y| (x.1 - at).length().total_cmp(&(y.1 - at).length()))
            .ok_or("nothing touches it there to fasten to")?;
        let id = self.ids[other];
        self.part_geometry(id)
            .map_err(|e| format!("what it touches: {e}"))?;
        Ok((id, point))
    }

    /// Whether `by` has fastener `k` at hand: on a creative server always; in survival a
    /// lashing's strip in the pockets, and no nails or pegs before G3.
    fn fastener_at_hand(&self, by: EntityId, k: usize) -> Result<Option<(usize, usize)>, String> {
        if self.creative {
            return Ok(None);
        }
        match self.building.fasteners[k].kind {
            Kind::Lashing { .. } => (self.find_pocket_strip(by))
                .map(Some)
                .ok_or_else(|| "a lashing needs a strip of bast in your pockets".into()),
            Kind::Nail { .. } => Err("you have no nails: they need iron (G3)".into()),
            Kind::Peg { .. } => Err("a peg needs a bored hole: no auger yet (G3)".into()),
        }
    }

    /// Each tick: fastenings whose time is up are made, if their maker still reaches the
    /// parts and the parts still touch; told either way.
    pub(super) fn fastenings_due(&mut self) {
        let due: Vec<(EntityId, UnderWay)> = (self.building.under_way.iter())
            .filter(|(_, u)| u.done <= self.tick)
            .map(|(by, u)| (*by, u.clone()))
            .collect();
        for (by, u) in due {
            self.building.under_way.remove(&by);
            let undone = match u.work {
                Work::Fasten | Work::Tack => None,
                Work::Finish => Some(self.finish_due(by, &u).map(|()| "lashed fast")),
                Work::Undo => Some(self.unfasten_due(by, &u).map(|()| "taken apart")),
                Work::Post => Some(self.post_due(by, &u).map(|()| "set in the ground")),
            };
            if let Some(outcome) = undone {
                let note = outcome.map_or_else(|why| format!("not done: {why}"), str::to_owned);
                self.note(by, note);
                continue;
            }
            let outcome = self.fasten_due(by, &u);
            let note = match outcome {
                Ok(()) if u.work == Work::Tack => format!(
                    "tacked with {}: let go, then J again to finish it",
                    self.building.fasteners[u.fastener].name
                ),
                Ok(()) => format!("fastened with {}", self.building.fasteners[u.fastener].name),
                Err(why) => format!("not fastened: {why}"),
            };
            self.note(by, note);
        }
    }

    fn fasten_due(&mut self, by: EntityId, u: &UnderWay) -> Result<(), String> {
        self.index(u.part).ok_or("that part is gone")?;
        let at = u.at;
        self.within_reach(by, at)?;
        let (other, point) = self.touching_any(u.part, at)?;
        let from = self.fastener_at_hand(by, u.fastener)?;
        let whole = &self.building.fasteners[u.fastener];
        let fastener = match u.work {
            Work::Tack => whole.tacked().ok_or("it cannot be tacked")?,
            _ => whole.clone(),
        };
        match other {
            Touched::Part(id) => {
                self.fasten(u.part, id, point, &fastener)?;
                self.hands_off(&[u.part, id]);
            }
            Touched::Tree(tree) => {
                self.fasten_to_tree(u.part, tree, point, &fastener)?;
                self.hands_off(&[u.part]);
            }
        }
        if let Some((pocket, k)) = from {
            let item = self.change_pockets(by, |p| p[pocket].take(k));
            if let (Some(item), Some(b)) = (item, self.building.built.get_mut(&u.part)) {
                b.items.push(item);
            }
        }
        Ok(())
    }
}

impl ServerWorld {
    /// Whether `at` is within reach of avatar `by`'s hands now.
    pub(super) fn within_reach(&self, by: EntityId, at: DVec3) -> Result<(), String> {
        let i = self.index(by).ok_or("no such avatar")?;
        let centre = self.world.bodies[i].structure.position[0];
        let up = character::plumb(&self.world, centre);
        let far = (at - creeer_sim::reach::hands(centre, up)).length();
        if far > creeer_sim::reach::REACH {
            return Err(format!("{far:.2} m from the hands: you walked away"));
        }
        Ok(())
    }
}
