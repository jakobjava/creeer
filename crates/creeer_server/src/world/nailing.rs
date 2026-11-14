//! Nailing by hand, quickly (G2.11, ADR-0036 amended 2026-11-14; the user, 2026-11-13: "aim
//! where two parts touch, one key, one nail, about a second and a hammer blow"): which fastener
//! the hand reaches for, where on a held part the nail goes, and how long it takes.
//!
//! - **What is reached for:** of the fasteners at hand, the shortest nail whose joint holds
//!   (`Fastener::law`: a nail's penetration among it), then a peg, then a lashing, as a carpenter
//!   picks a nail by the wood (the client's default was the 75 mm nail, too short for a 60 mm
//!   rail into anything: refused, unless the player knew K chose another).
//! - **Where, on a part in the hands:** where it touches another part nearest the point given
//!   (the crosshair rests the part against what it aims at: the aim is where they touch).
//! - **How long:** a nail the blows its driving work needs (`creeer_building::hammering`); any
//!   other fastener its catalogue time.

use super::tree_joints::Touched;
use super::*;
use creeer_building::fasteners::{Kind, side};
use creeer_building::law::Placement;

/// The most hammer blows told a tick (one a player).
const BLOWS_KEPT: usize = 64;

impl ServerWorld {
    /// The point where part `id` touches another part nearest `at` (any distance along it): a
    /// part in the hands is nailed where it rests against what it is held to.
    pub(super) fn nearest_touch(&self, id: EntityId, at: DVec3) -> Option<DVec3> {
        let i = self.index(id)?;
        (self
            .world
            .touching(i, super::building::TOUCHING)
            .into_iter())
        .filter(|t| {
            t.other
                .is_some_and(|(j, _)| j != i && self.players[j].is_none())
        })
        .map(|t| t.point)
        .min_by(|a, b| (*a - at).length().total_cmp(&(*b - at).length()))
    }

    /// Body `id`'s wood and its thickness (m) where `point` is: its member nearest it if it is
    /// built, else its own (a rod's diameter, a board's least side).
    pub(super) fn side_at(&self, id: EntityId, point: DVec3) -> Result<(String, f64), String> {
        if let Some(g) = self.part_geometry(id)? {
            let thick = match g.part.profile {
                creeer_building::assembly::Profile::Round { start, end } => 0.5 * (start + end),
                creeer_building::assembly::Profile::Board { width, depth } => width.min(depth),
            };
            return Ok((g.wood, thick));
        }
        let s = &self.world.bodies[self.index(id).ok_or("it is gone")?].structure;
        let (e, thick) = (0..s.element_count())
            .filter_map(|e| s.member(e).map(|m| (e, m)))
            .map(|(e, (a, b, thick))| {
                let (pa, pb) = (s.position[a], s.position[b]);
                let d = pb - pa;
                let t = ((point - pa).dot(d) / d.length_squared()).clamp(0.0, 1.0);
                (e, thick, (pa + d * t - point).length())
            })
            .min_by(|x, y| x.2.total_cmp(&y.2))
            .map(|(e, thick, _)| (e, thick))
            .ok_or("no member there")?;
        let wood = (self.building.built.get(&id))
            .and_then(|r| r.woods.get(e).cloned().flatten())
            .ok_or("a joint is no part")?;
        Ok((wood, thick))
    }

    /// The two sides a fastener at `at` goes through: `part`'s (it goes in from there) and what
    /// touches it there (a standing tree's stem: the part's wood, a stem's thickness).
    fn sides_at(
        &self,
        part: EntityId,
        at: DVec3,
    ) -> Result<(creeer_building::law::Side, creeer_building::law::Side), String> {
        let (wa, ta) = self.side_at(part, at)?;
        let (wb, tb) = match self.touching_any(part, at)? {
            (Touched::Part(other), p) => self.side_at(other, p)?,
            (Touched::Tree(_), _) => (wa.clone(), 0.3),
        };
        Ok((side(self.wood(&wa)?, ta)?, side(self.wood(&wb)?, tb)?))
    }

    /// The catalogue's fastener `by` reaches for to fasten `part` where it touches at `at`: of
    /// those at hand, the shortest nail that holds there, then a peg, then a lashing.
    pub(super) fn reach_for(
        &self,
        by: EntityId,
        part: EntityId,
        at: DVec3,
    ) -> Result<usize, String> {
        let (a, b) = self.sides_at(part, at)?;
        let materials = self.building.materials.as_ref().ok_or("no materials")?;
        let fs = &self.building.fasteners;
        let order = |k: &usize| match fs[*k].kind {
            Kind::Nail { length, .. } => (0, length),
            Kind::Peg { length, .. } => (1, length),
            Kind::Lashing { .. } => (2, 0.0),
        };
        let mut ks: Vec<usize> = (0..fs.len()).collect();
        ks.sort_by(|x, y| {
            order(x)
                .partial_cmp(&order(y))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut why = "nothing at hand to fasten with".to_owned();
        for k in ks {
            if let Err(e) = self.fastener_at_hand(by, k) {
                why = e;
                continue;
            }
            let at = Placement {
                count: 1,
                spread: 0.0,
                contact: a.thickness.min(b.thickness) * a.thickness.min(b.thickness),
            };
            let between = 0.5 * (a.thickness + b.thickness);
            match fs[k].law(materials, (&a, &b), &at, between) {
                Ok(_) => return Ok(k),
                Err(e) => why = e,
            }
        }
        Err(why)
    }

    /// How long fastener `k` takes `part` where it touches at `at` (s), and the hammer blows
    /// (none for what is not nailed).
    pub(super) fn fastening_time(&self, k: usize, part: EntityId, at: DVec3) -> (f64, u32) {
        let f = &self.building.fasteners[k];
        match (f.kind, &self.building.hammering) {
            (
                Kind::Nail {
                    diameter, length, ..
                },
                Some(h),
            ) => {
                let blows = (self.sides_at(part, at))
                    .map_or(1, |(a, b)| h.blows((diameter, length), (&a, &b)));
                (h.seconds(blows), blows)
            }
            _ => (f.seconds.unwrap_or(2.0), 0),
        }
    }

    /// Each tick: the hammer blows landing now, of nails being driven (`Building::hammer`: evenly
    /// over its time, the last as it is done): who strikes and where.
    pub(super) fn blows_now(&mut self) {
        let tick = self.tick;
        for (by, u) in &self.building.under_way {
            let Some(&(begun, blows)) = self.building.hammer.get(by) else {
                continue;
            };
            if blows == 0 || self.building.blows.len() >= BLOWS_KEPT {
                continue;
            }
            let span = u.done.saturating_sub(begun);
            let n = u64::from(blows);
            if (1..=n).any(|k| begun + span * k / n == tick) {
                self.building.blows.push((*by, u.at));
            }
        }
    }

    /// The nails `by` drove into `part` while holding it, made joints as the hands let go: held
    /// still by the hands meanwhile, it lay where they were driven (G2.11). One refused (a
    /// second at a crossing already fastened, what it touched gone) is told.
    pub(super) fn make_driven(&mut self, by: EntityId, part: EntityId) {
        let Some(driven) = self.building.driven.remove(&by) else {
            return;
        };
        for (other, point, k) in driven {
            let fastener = self.building.fasteners[k].clone();
            if let Err(why) = self.fasten(part, other, point, &fastener) {
                self.note(by, format!("not fastened there: {why}"));
            }
        }
    }

    /// The hammer blows that landed since last asked (G2.11: for sound and drawing): who struck
    /// and where.
    pub fn take_hammer_blows(&mut self) -> Vec<(EntityId, DVec3)> {
        std::mem::take(&mut self.building.blows)
    }
}
