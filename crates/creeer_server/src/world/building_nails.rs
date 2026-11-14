//! A fastener driven where two parts cross (G2.11 B, ADR-0036 amended 2026-11-14; the law:
//! ADR-0033): the parts made one structure if they are not, and the joint between their axes at
//! the crossing made, or, if they are nailed there already, made the joint of all its nails.
//!
//! - **Where:** nearest where the player aims, within the crossing: each nail at least two of
//!   its diameters inside both parts' edges (a nail aimed at a stringer's side under a board goes
//!   down through the board into the stringer's middle).
//! - **Several at one crossing** are one joint: ADR-0033's law for their number and their
//!   spread about the crossing (Σr²: two nails 8 cm apart hold twice the slip of one and turn
//!   far stiffer). The joint stays where the first was made: its nails' count and spread are
//!   read back from what it holds (its withdrawal is n nails', its twist k·(Σr² + n·d²)).
//! - **Refused plainly** what would not hold: a nail too short to reach eight of its diameters
//!   into the second part (EN 1995-1-1 §8.3.1.2: a smooth nail's least pointside penetration;
//!   below it, it does not resist pulling, and lets go at once).

use super::crossing::Crossing;
use super::*;
use creeer_building::fasteners::{Fastener, Kind, side};
use creeer_building::law::Placement;
use creeer_physics::structure::{ImplicitSolver, JointLaw, Scheme};

/// A joint this close (m) to the two axes' points at a crossing is the crossing's.
const SAME_CROSSING: f64 = 0.03;
/// How far inside a part's edges a nail goes, in its diameters (EN 1995-1-1 Table 8.2 asks
/// 5·d from an unloaded edge for the nail's full strength; within 2·d it splits the edge).
const FROM_EDGE: f64 = 2.0;

impl ServerWorld {
    /// The law of `count` of fastener `f` at crossing `c`, `spread` (Σr², m²) about it.
    pub(in crate::world) fn crossing_law(
        &self,
        c: &Crossing,
        f: &Fastener,
        (count, spread): (u32, f64),
    ) -> Result<JointLaw, String> {
        let (th, tt) = c.thicknesses();
        let (sa, sb) = (
            side(self.wood(&c.head.wood)?, th)?,
            side(self.wood(&c.tail.wood)?, tt)?,
        );
        let materials = self.building.materials.as_ref().ok_or("no materials")?;
        // The two bear on each other over about the narrower's thickness squared (an estimate).
        let at = Placement {
            count,
            spread,
            contact: th.min(tt) * th.min(tt),
        };
        let between = (c.on_tail - c.on_head).length().max(1e-3);
        f.law(materials, (&sa, &sb), &at, between)
            .map_err(|why| plain_refusal(&why, f, th))
    }

    /// Where a fastener aimed at `at` goes in crossing `c`: on the plane where the parts touch,
    /// within both, at least [`FROM_EDGE`] of its diameters inside their edges. Its offset from
    /// the crossing's middle (m).
    fn nail_offset(c: &Crossing, f: &Fastener, at: DVec3) -> DVec3 {
        let d = match f.kind {
            Kind::Nail { diameter, .. } | Kind::Peg { diameter, .. } => diameter,
            Kind::Lashing { .. } => return DVec3::ZERO,
        };
        let middle = c.touch;
        let mut r = (at - middle).reject_from(c.way);
        for bar in [&c.head, &c.tail] {
            let along = (bar.ends.1 - bar.ends.0).normalize_or_zero();
            let across = along.cross(c.way).normalize_or_zero();
            let room = (bar.half_across(c.way) - FROM_EDGE * d).max(0.0);
            let now = r.dot(across);
            r += across * (now.clamp(-room, room) - now);
        }
        r
    }

    /// One of fastener `f` driven at crossing `c` where aimed at `at`, the parts made one
    /// structure under `keep`'s id (one of the two). Refused, before anything is changed, if
    /// it would not hold.
    pub(in crate::world) fn drive(
        &mut self,
        keep: EntityId,
        c: &Crossing,
        f: &Fastener,
        at: DVec3,
    ) -> Result<(), String> {
        let other = if c.head.id == keep {
            c.tail.id
        } else {
            c.head.id
        };
        let r = Self::nail_offset(c, f, at);
        // Whether one would hold there, the parts still apart (refused after they were made one,
        // they stayed one with no joint: G2.10).
        let one = self.crossing_law(c, f, (1, r.length_squared()))?;
        self.make_built(keep)?;
        self.make_built(other)?;
        let xi = self.index(keep).ok_or("it is gone")?;
        let first = self.world.bodies[xi].structure.element_count();
        if other != keep {
            let theirs = self.remove_entity(other).ok_or("what it touches is gone")?;
            let xi = self.index(keep).ok_or("it is gone")?;
            let body = &mut self.world.bodies[xi];
            let offset = body.structure.append(&theirs.structure);
            body.shapes
                .extend(theirs.shapes.iter().map(|(n, s)| (n + offset, s.clone())));
            let record = self.building.built.remove(&other).unwrap_or_default();
            let mine = self.building.built.entry(keep).or_default();
            mine.items.extend(record.items);
            mine.woods.extend(record.woods);
        }
        let xi = self.index(keep).ok_or("it is gone")?;
        let end = self.world.bodies[xi].structure.element_count();
        // Each side's members: its own element if it was built in this body already, else all
        // of its body's (made a structure just now, or taken in).
        let range = |bar: &super::crossing::Bar| match bar.element {
            Some(e) if bar.id == keep => e..e + 1,
            Some(e) => first + e..first + e + 1,
            None if bar.id == keep => 0..first,
            None => first..end,
        };
        let (hr, tr) = (range(&c.head), range(&c.tail));
        let made = self.nail_joint(keep, c, (hr, tr), f, (one, r));
        let body = &mut self.world.bodies[xi];
        body.solver = ImplicitSolver::new(&body.structure, Scheme::Newmark, DT, 1);
        body.wake();
        let count = body.structure.element_count();
        if let Some(rec) = self.building.built.get_mut(&keep) {
            rec.woods.resize(count, None);
        }
        made
    }

    /// The joint at crossing `c` in structure `x` given one more of `f` at `r` from its middle,
    /// or made with the one (`one`: its law alone) between nodes put on the head's members
    /// (`ranges.0`) and the tail's (`ranges.1`).
    fn nail_joint(
        &mut self,
        x: EntityId,
        c: &Crossing,
        ranges: (std::ops::Range<usize>, std::ops::Range<usize>),
        f: &Fastener,
        (one, r): (JointLaw, DVec3),
    ) -> Result<(), String> {
        let i = self.index(x).ok_or("it is gone")?;
        let s = &self.world.bodies[i].structure;
        let near = |n: usize, p: DVec3| (s.position[n] - p).length() < SAME_CROSSING;
        let there = (0..s.element_count()).find(|&e| {
            s.joint_ends(e).is_some_and(|(a, b)| {
                (near(a, c.on_head) && near(b, c.on_tail))
                    || (near(a, c.on_tail) && near(b, c.on_head))
            })
        });
        if let Some(e) = there {
            let (n, spread) = self.nails_in(x, e, f, one)?;
            let law = self.crossing_law(c, f, (n + 1, spread + r.length_squared()))?;
            let body = &mut self.world.bodies[i];
            body.structure.refasten(e, &law);
            return Ok(());
        }
        let (a, _, _) = self.node_at(x, c.on_head, ranges.0)?;
        let (b, _, _) = self.node_at(x, c.on_tail, ranges.1)?;
        let i = self.index(x).ok_or("it is gone")?;
        self.world.bodies[i].structure.join(a, b, &one);
        Ok(())
    }

    /// How many of fastener `f` joint `e` of structure `x` holds by, and their spread about it
    /// (Σr², m²), read back from what it holds: `one`, the law of one of them at its middle
    /// (its spread aside), holds a withdrawal of w and turns by k·(r² + d²).
    fn nails_in(
        &self,
        x: EntityId,
        e: usize,
        f: &Fastener,
        one: JointLaw,
    ) -> Result<(u32, f64), String> {
        let s = &self.world.bodies[self.index(x).ok_or("it is gone")?].structure;
        let (withdrawal, twist) = (s.joint_withdrawal(e), s.joint_twist(e));
        let (Some(withdrawal), Some(twist)) = (withdrawal, twist) else {
            return Err("that joint is gone".into());
        };
        let d = match f.kind {
            Kind::Nail { diameter, .. } | Kind::Peg { diameter, .. } => diameter,
            Kind::Lashing { .. } => return Err("it is lashed there already".into()),
        };
        // (Within 15 %: the parts settle by a millimetre or so between nails, and a nail that
        // just reaches 12 diameters into the part behind loses a share of its grip by it; the
        // catalogue's other sizes hold a third and more less.)
        let n = withdrawal / one.withdrawal;
        if (n - n.round()).abs() > 0.15 || n < 0.5 {
            return Err(format!(
                "it is fastened there with something else already (as {n:.2} of these)"
            ));
        }
        // (A handful of nails.)
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = n.round() as u32;
        // k·d²: one's twist less its own spread's part (k = one's slip modulus).
        let k = one.lateral;
        let spread = (twist / k - f64::from(n) * d * d).max(0.0);
        Ok((n, spread))
    }
}

/// The law's refusal in the player's words: `why` as `law::dowels` gives it ("reaches N mm into
/// the second part, M mm needed") for fastener `f` through `through` (m) of the first part.
pub(in crate::world) fn plain_refusal(why: &str, f: &Fastener, through: f64) -> String {
    let length = match f.kind {
        Kind::Nail { length, .. } | Kind::Peg { length, .. } => length,
        Kind::Lashing { .. } => return why.to_owned(),
    };
    let mm = |m: f64| (m * 1000.0).round();
    let Some(reached) = why
        .strip_prefix("reaches ")
        .and_then(|w| w.split(" mm").next())
        .and_then(|n| n.parse::<f64>().ok())
    else {
        return why.to_owned();
    };
    let needed = why.rsplit(", ").next().unwrap_or(why);
    if reached <= 0.0 {
        return format!(
            "too short: a {} mm nail does not come through the {} mm of this part. Nail from \
             the other side",
            mm(length),
            mm(through)
        );
    }
    format!(
        "too short to hold: a {} mm nail through {} mm of this part reaches {reached:.0} mm \
         into the one behind ({needed}). Nail from the other side",
        mm(length),
        mm(through)
    )
}
