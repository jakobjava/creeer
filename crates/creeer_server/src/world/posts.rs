//! Setting a post (G2.4, ADR-0033): a part standing with its foot in a hole is backfilled, and
//! the ground holds its buried length as the soil round a post does (`creeer_building::soil`):
//! a spring at every node below the surface, every [`SPAN`] at most, from the ground's own
//! density, cohesion and friction there (`data/geology/rocks.ron`). Solid rock holds nothing
//! this way: a post stands in loose ground.

use super::*;
use creeer_building::soil::Soil;
use creeer_terrain::geology::rocks::Strength;

/// The longest buried length one soil spring stands for (m).
const SPAN: f64 = 0.1;
/// How long backfilling a hole round a post takes by hand (s: short, as the user chose for all
/// the building work, 2026-10-30; the post held upright meanwhile).
const BACKFILLING: f64 = 8.0;
/// How far beside a post its ground is judged (m): beyond the hole dug for it.
const BESIDE: f64 = 0.6;

impl ServerWorld {
    /// Part (or built structure) `id` set in the ground where it stands in it: its buried
    /// nodes held by the soil. Refused if none of it is in loose ground.
    pub(super) fn set_post(&mut self, id: EntityId) -> Result<(), String> {
        self.make_built(id)?;
        let i = self.index(id).ok_or("it is gone")?;
        let g = (self.world.planet.as_ref()).map_or(9.81, |p| {
            p.acceleration(self.world.bodies[i].structure.position[0])
                .length()
        });
        // Every member reaching below the surface: nodes put in along its buried length.
        let count = self.world.bodies[i].structure.element_count();
        for e in 0..count {
            self.bury_member(i, e);
        }
        let s = &self.world.bodies[i].structure;
        let mut springs = Vec::new();
        for n in 0..s.node_count() {
            let p = s.position[n];
            if s.held_fast(n) {
                continue;
            }
            let Some((soil, depth, up)) = self.soil_at(p) else {
                continue;
            };
            let (span, thick) = self.buried_span(i, n);
            if span <= 0.0 {
                continue;
            }
            springs.push((n, soil, depth, span, thick, up));
        }
        if springs.is_empty() {
            return Err("none of it stands in loose ground: dig a hole and stand it in it".into());
        }
        let deepest = (springs.iter().enumerate())
            .max_by(|a, b| a.1.2.total_cmp(&b.1.2))
            .map(|(k, _)| k);
        let s = &mut self.world.bodies[i].structure;
        for (k, (n, soil, depth, span, thick, up)) in springs.into_iter().enumerate() {
            let mut law = soil.spring(thick, depth, span, g);
            if Some(k) == deepest {
                law.bearing += soil.tip(thick, depth, g);
            }
            s.anchor(n, up, &law);
        }
        let body = &mut self.world.bodies[i];
        body.solver = creeer_physics::structure::ImplicitSolver::new(
            &body.structure,
            creeer_physics::structure::Scheme::Newmark,
            DT,
            1,
        );
        body.wake();
        if let Some(r) = self.building.built.get_mut(&id) {
            r.woods
                .resize(self.world.bodies[i].structure.element_count(), None);
        }
        Ok(())
    }

    /// Member `e` of body `i`, if it reaches below the surface: nodes put into its buried length
    /// every [`SPAN`] at most (the structure's record kept in step).
    fn bury_member(&mut self, i: usize, e: usize) {
        let s = &self.world.bodies[i].structure;
        let Some((a, b, _)) = s.member(e) else { return };
        let (pa, pb) = (s.position[a], s.position[b]);
        let (below_a, below_b) = (self.soil_at(pa).is_some(), self.soil_at(pb).is_some());
        if !below_a && !below_b {
            return;
        }
        // Where it crosses the surface (bisection), or all of it buried.
        let (mut lo, mut hi) = (0.0, 1.0);
        if below_a != below_b {
            for _ in 0..30 {
                let t = 0.5 * (lo + hi);
                let under = self.soil_at(pa + (pb - pa) * t).is_some();
                if under == below_a {
                    lo = t;
                } else {
                    hi = t;
                }
            }
        }
        let (from, to) = match (below_a, below_b) {
            (true, true) => (0.0, 1.0),
            (true, false) => (0.0, lo),
            _ => (hi, 1.0),
        };
        let length = (pb - pa).length() * (to - from);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few
        let pieces = (length / SPAN).ceil() as usize;
        // Put in from the far end back, so each split leaves the rest of the member at `e`.
        let id = self.ids[i];
        // Element `e` covers [0, end] of the member as it was; each split shortens it.
        let mut end = 1.0;
        for k in (1..pieces).rev() {
            let t = from + (to - from) * k as f64 / pieces as f64;
            if t > 1e-3 && t < 1.0 - 1e-3 {
                let wood =
                    (self.building.built.get(&id)).and_then(|r| r.woods.get(e).cloned().flatten());
                self.world.bodies[i].structure.insert_node(e, t / end);
                end = t;
                if let Some(r) = self.building.built.get_mut(&id) {
                    r.woods.push(wood);
                }
            }
        }
    }

    /// The buried length node `n` of body `i` stands for (half each member at it below the
    /// surface) and the thickest member's thickness there.
    fn buried_span(&self, i: usize, n: usize) -> (f64, f64) {
        let s = &self.world.bodies[i].structure;
        let (mut span, mut thick) = (0.0f64, 0.0f64);
        for e in 0..s.element_count() {
            if let Some((a, b, t)) = s.member(e)
                && (a == n || b == n)
            {
                let other = if a == n { b } else { a };
                if self.soil_at(s.position[other]).is_some() {
                    span += 0.5 * (s.position[a] - s.position[b]).length();
                }
                thick = thick.max(t);
            }
        }
        (span.min(SPAN), thick)
    }

    /// The loose ground beside where `p` is, if `p` is below its surface: a hole dug for a post
    /// is air, and the backfill is that ground put back, so it is judged [`BESIDE`] away: its
    /// soil, how deep `p` is below its surface (m), and the way out of it.
    fn soil_at(&self, p: DVec3) -> Option<(Soil, f64, DVec3)> {
        let up = p.normalize();
        let q = p + up.any_orthonormal_vector() * BESIDE;
        // How deep q is below the ground there (the density is the distance above it).
        let depth = -self.terrain.density(q);
        if depth <= 0.0 {
            return None;
        }
        let rock = self.terrain.rock_at(q)?;
        let r = &self.terrain.planet().geology.rocks.rocks[usize::from(rock)];
        let Strength::Loose { cohesion, friction } = r.strength else {
            return None;
        };
        let soil = Soil {
            density: r.density,
            friction,
            cohesion,
        };
        Some((soil, depth.max(0.01), up))
    }
}

impl ServerWorld {
    /// Player `by` begins backfilling round part `id` where it stands in a hole: hands free, and
    /// in survival some dug ground in the pockets to fill with.
    pub(super) fn begin_post(
        &mut self,
        by: EntityId,
        id: EntityId,
        at: DVec3,
    ) -> Result<(), String> {
        // Held upright in its hole, the hands keep the post still while it is backfilled (the
        // user, 2026-10-30: "i do not understand how to set a log in a hole"); anything else
        // held takes the hands.
        if self.carried(by).is_some_and(|held| held != id) {
            return Err("put down what you hold, or hold the post itself".into());
        }
        self.part_geometry(id)?;
        if !self.creative && self.dug_ground(by).is_none() {
            return Err("backfilling needs dug ground in your pockets".into());
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // half a minute
        let done = self.tick + (BACKFILLING * 60.0).ceil() as u64;
        let u = super::building::UnderWay {
            part: id,
            at,
            fastener: 0,
            done,
            work: super::building::Work::Post,
        };
        self.building.under_way.insert(by, u);
        self.note(by, format!("backfilling: {BACKFILLING:.0} s"));
        Ok(())
    }

    /// The backfilling's time up: if its maker still reaches the post, it is set, the ground
    /// it was filled with taken from the pockets.
    pub(super) fn post_due(
        &mut self,
        by: EntityId,
        u: &super::building::UnderWay,
    ) -> Result<(), String> {
        self.index(u.part).ok_or("it is gone")?;
        let at = u.at;
        self.within_reach(by, at)?;
        let from = if self.creative {
            None
        } else {
            Some(
                self.dug_ground(by)
                    .ok_or("no dug ground left to fill with")?,
            )
        };
        self.set_post(u.part)?;
        // Set, the ground holds it: the hands let go (a part made a structure has other nodes).
        self.hands_off(&[u.part]);
        if let Some((pocket, k)) = from {
            self.change_pockets(by, |p| p[pocket].take(k));
        }
        Ok(())
    }

    /// Dug ground in avatar `by`'s pockets (its pocket and place): a heap's scoop.
    fn dug_ground(&self, by: EntityId) -> Option<(usize, usize)> {
        let pockets = self.items.pockets.get(&by)?;
        (pockets.iter().enumerate()).find_map(|(p, s)| {
            (s.items
                .iter()
                .position(|i| matches!(i, creeer_sim::items::Item::Bulk(_))))
            .map(|k| (p, k))
        })
    }
}
