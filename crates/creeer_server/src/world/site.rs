//! The bridge site (G2.11, ADR-0037): a place made ready to build a footbridge across a gap,
//! walk over it and load it until it breaks.
//!
//! Beside the spawn (flat ground in its clearing, G1.3i) a gully is dug: 2.3 m across at its
//! rims and 1.8 m deep, 12 m long, round-bottomed as a stream wears one, with a slope out of it
//! at its south end. A 3 m stringer spans it with 35 cm to rest on at each side. The timber for
//! a small footbridge lies in rows a few steps north of where the player starts (the BTCV's
//! layout for a 3 m span: `data/kit.ron`), its loads a few steps south. The player starts on
//! the near bank, the gully 2.9 m ahead of them to the east.
//!
//! Everything here is what players could have done themselves: the gully is dug as tools dig
//! (edits of the terrain, saved with the world's other edits), the parts are the creative kit's
//! items, laid on the ground. Nothing is held up, fixed or helped (principle 13).

use super::*;
use creeer_math::glam::{DMat3, DQuat};
use creeer_terrain::geology::deposits::tangent_frame;

#[path = "site_watch.rs"]
mod watch;
pub(crate) use watch::Watch;
pub use watch::{COLLAPSE, Failure, Inspection};
#[cfg(test)]
#[path = "site_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "site_watch_tests.rs"]
mod watch_tests;

/// The gully: across its rims, deep at its middle, long (m), and its middle's distance east of
/// where the player starts (m).
pub const WIDTH: f64 = 2.3;
pub const DEPTH: f64 = 1.8;
pub const LENGTH: f64 = 12.0;
pub const OUT: f64 = 4.0;
/// The slope out of its south end: how long it runs to the ground (m).
const RAMP: f64 = 6.0;

/// What the site is: where things are, for a script or a camera.
#[derive(Clone, Debug, PartialEq)]
pub struct BridgeSite {
    /// Where the player starts (on the ground), and the level ways across the gully (from the
    /// near bank to the far one) and along it (north).
    pub stand: DVec3,
    pub across: DVec3,
    pub along: DVec3,
    /// The gully's middle at ground level (between its rims).
    pub middle: DVec3,
    /// The kit's items laid, by name, in the order laid.
    pub laid: Vec<(String, EntityId)>,
}

impl BridgeSite {
    /// The first item laid of kit name `name`, and the `k`-th (from 0).
    pub fn item(&self, name: &str, k: usize) -> Option<EntityId> {
        (self.laid.iter())
            .filter(|(n, _)| n == name)
            .nth(k)
            .map(|(_, id)| *id)
    }

    /// A point `x` m across (east) and `y` m along (north) of the start, on the level of the
    /// start (not on the ground).
    pub fn at(&self, x: f64, y: f64) -> DVec3 {
        self.stand + self.across * x + self.along * y
    }

    /// The site as made in world `w` (by [`ServerWorld::prepare_bridge_site`], its first item
    /// entity `first`): its places from the spawn, its items' ids in the order laid.
    pub fn of(w: &ServerWorld, first: u32) -> Self {
        let stand = w.spawn;
        let (across, along) = tangent_frame(stand.normalize());
        // Between the banks beyond the rims (the ground there is as it was).
        let bank = |side: f64| w.ground_under(stand + across * (OUT + side * (WIDTH * 0.5 + 0.3)));
        let names = (TIMBER.iter().chain(&LOADS)).flat_map(|r| std::iter::repeat_n(r.0, r.1));
        Self {
            stand,
            across,
            along,
            middle: (bank(-1.0) + bank(1.0)) * 0.5,
            laid: (names.zip(first..))
                .map(|(n, id)| (n.to_owned(), EntityId(id)))
                .collect(),
        }
    }

    /// Kit item `name`'s place in the kit (as `Command::Kit` asks for it).
    pub fn kit_index(data: &creeer_data::Data, name: &str) -> Option<u8> {
        let kit = creeer_sim::items::kit::kit(data).ok()?;
        u8::try_from(kit.iter().position(|k| k.name == name)?).ok()
    }
}

/// The timber's rows: kit name, how many, the first's place (m east, m north of the start),
/// the step to the next (m east, m north), and whether it lies along the gully (north).
type Row = (&'static str, usize, (f64, f64), (f64, f64), bool);
const TIMBER: [Row; 6] = [
    ("stringer", 3, (0.0, 1.4), (0.0, 0.3), false),
    ("sleeper", 2, (0.0, 2.4), (0.0, 0.3), false),
    ("deck board", 12, (-1.3, 3.45), (0.22, 0.0), true),
    ("handrail post", 4, (-1.6, 1.9), (-0.2, 0.0), true),
    ("handrail", 2, (-2.5, 1.9), (-0.2, 0.0), true),
    ("brace", 4, (-1.6, 3.45), (-0.2, 0.0), true),
];
const LOADS: [Row; 7] = [
    ("granite block", 4, (-0.6, -1.4), (0.45, 0.0), false),
    ("granite block", 4, (-0.6, -1.75), (0.45, 0.0), false),
    ("granite block", 4, (-0.6, -2.1), (0.45, 0.0), false),
    ("granite block", 4, (-0.6, -2.45), (0.45, 0.0), false),
    ("gravel sack", 4, (-1.6, -1.4), (0.0, -0.4), false),
    ("load crate", 2, (-0.35, -3.2), (1.0, 0.0), false),
    ("boulder", 1, (-1.1, -3.9), (0.0, 0.0), false),
];

impl ServerWorld {
    /// Makes the bridge site at the spawn: digs the gully, lays the timber and the loads, and
    /// has players appear on the near bank. Returns where everything is.
    pub fn prepare_bridge_site(&mut self) -> BridgeSite {
        let mut site = BridgeSite::of(self, self.next_id);
        site.laid.clear();
        let (across, along) = (site.across, site.along);
        self.dig_gully(&site);
        for (name, n, (x, y), (dx, dy), lengthwise) in TIMBER.into_iter().chain(LOADS) {
            for k in 0..n {
                #[allow(clippy::cast_precision_loss)] // a few
                let kf = k as f64;
                let at = site.at(x + dx * kf, y + dy * kf);
                let way = if lengthwise { along } else { across };
                if let Some(id) = self.lay_kit(name, at, way) {
                    site.laid.push((name.to_owned(), id));
                }
            }
        }
        site
    }

    /// The gully: a capsule along it whose circle cuts the ground [`WIDTH`] apart and reaches
    /// [`DEPTH`] down (a round bottom, its walls 65° steep at the rims), and its slope out, the
    /// same circle rising south to the ground over [`RAMP`].
    fn dig_gully(&mut self, site: &BridgeSite) {
        // r² − d² = (w/2)², r + d = D.
        let half = WIDTH * 0.5;
        let radius = 0.5 * (DEPTH + half * half / DEPTH);
        let below = DEPTH - radius;
        let axis = |y: f64, lift: f64| {
            let ground = self.ground_under(site.middle + site.along * y);
            ground + ground.normalize() * lift
        };
        let (north, south) = (axis(LENGTH * 0.5, -below), axis(-LENGTH * 0.5, -below));
        let out = axis(-LENGTH * 0.5 - RAMP, radius);
        for (a, b) in [(north, south), (south, out)] {
            self.terrain
                .edit(Edit::Dig(Brush::Capsule { a, b, radius }));
        }
    }

    /// Kit item `name` laid on the ground at `at` (under or over it), its length along `way`
    /// and its height up: a millimetre clear of the ground. Its id, if the kit has it.
    fn lay_kit(&mut self, name: &str, at: DVec3, way: DVec3) -> Option<EntityId> {
        let item = self.kit_named(name)?;
        let ground = self.ground_under(at);
        let up = ground.normalize();
        let x = way.reject_from(up).normalize();
        let turn = DQuat::from_mat3(&DMat3::from_cols(x, up, x.cross(up)));
        let height = super::laid::half(&item).y;
        let id = self.add_item(item, ground + up * (height + 0.001), false);
        let i = self.index(id)?;
        let body = &mut self.world.bodies[i];
        body.structure.orientation[0] = turn;
        body.solver.reset_history(&body.structure);
        // Clear of the ground where it slopes under its length.
        self.lift_clear(i);
        Some(id)
    }

    /// Part `y` nailed to part `x` where they cross at `point` with fastener `fastener`, at once
    /// (a scene assembling what it builds, as the tower is: the joint, its law and the merged
    /// structure are those a player's J makes). One structure under `x`'s id.
    ///
    /// # Errors
    /// If there is no such fastener, or the joint would not hold.
    pub fn fasten_now(
        &mut self,
        (x, y): (EntityId, EntityId),
        point: DVec3,
        fastener: &str,
    ) -> Result<(), String> {
        let f = (self.building.fasteners.iter())
            .find(|f| f.name == fastener)
            .cloned()
            .ok_or("no such fastener")?;
        self.fasten(x, y, point, &f)
    }

    /// A second nail between two parts already of one structure `x` (a deck board nailed to a
    /// stringer the deck already holds), where they cross at `point`: the member nearest it
    /// running along `ways.0` and the one along `ways.1`. As [`Self::fasten_now`], for a scene
    /// assembling what it builds: a player's J does not close such a loop yet (stream B).
    ///
    /// # Errors
    /// If there is no such fastener or member, or the joint would not hold.
    pub fn nail_within(
        &mut self,
        x: EntityId,
        point: DVec3,
        ways: (DVec3, DVec3),
        fastener: &str,
    ) -> Result<(), String> {
        let f = (self.building.fasteners.iter())
            .find(|f| f.name == fastener)
            .cloned()
            .ok_or("no such fastener")?;
        let i = self.index(x).ok_or("it is gone")?;
        let member = |s: &creeer_physics::structure::Structure, way: DVec3| {
            (0..s.element_count())
                .filter_map(|e| s.member(e).map(|m| (e, m)))
                .filter(|(_, (a, b, _))| {
                    let d = (s.position[*b] - s.position[*a]).normalize();
                    d.dot(way).abs() > 0.87
                })
                .map(|(e, (a, b, _))| {
                    let (pa, pb) = (s.position[a], s.position[b]);
                    let d = pb - pa;
                    let t = ((point - pa).dot(d) / d.length_squared()).clamp(0.0, 1.0);
                    (e, (pa + d * t - point).length())
                })
                .min_by(|p, q| p.1.total_cmp(&q.1))
                .map(|(e, _)| e)
        };
        let s = &self.world.bodies[i].structure;
        let (ea, eb) = (member(s, ways.0), member(s, ways.1));
        let (ea, eb) = (ea.zip(eb)).ok_or("no members crossing there")?;
        let a = self.node_at(x, point, ea..ea + 1)?;
        let b = self.node_at(x, point, eb..eb + 1)?;
        let law = self.joint_law(x, [a, b], &f)?;
        let body = &mut self.world.bodies[i];
        body.structure.join(a.0, b.0, &law);
        body.solver = creeer_physics::structure::ImplicitSolver::new(
            &body.structure,
            creeer_physics::structure::Scheme::Newmark,
            DT,
            1,
        );
        let count = body.structure.element_count();
        if let Some(r) = self.building.built.get_mut(&x) {
            r.woods.resize(count, None);
        }
        Ok(())
    }
}
