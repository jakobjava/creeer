//! Where two parts touch, and which way a nail goes there (G2.11 B, ADR-0036 amended
//! 2026-11-14; the joint's law: ADR-0033).
//!
//! - **What touches:** another body (the world's contacts), a standing tree's trunk, or another
//!   member of the same built structure: parts fastened once are one body, whose members do not
//!   collide with each other, so where they touch is found from how they lie (two axes' nearest
//!   points, each part's section about them). A deck's board nailed to one stringer still
//!   touches the next stringer, and is nailed there too.
//! - **Which way the nail goes:** as a carpenter drives it, from the side they stand on, through
//!   the part on that side into the other (a deck board from above into the stringer under it,
//!   a rail from the front into the post behind it). Seen edge-on, through the thinner.
//! - **How thick each part is along it:** its section's reach that way (a stringer on edge is
//!   150 mm deep down a nail, 75 mm across one).

use super::*;
use creeer_physics::structure::nails::MemberFrame;

/// What a part touches where it is to be fastened.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::world) enum Against {
    /// Another body.
    Part(EntityId),
    /// A standing tree's trunk.
    Tree(u64),
    /// Another member of the same built structure: its two members (elements).
    Itself(usize, usize),
}

/// A piece of a part where a fastener meets it: its axis, how its section lies about it, its
/// wood, and the body and member it is (none: a part not built yet, one member).
#[derive(Clone, Debug)]
pub(in crate::world) struct Bar {
    pub(in crate::world) id: EntityId,
    pub(in crate::world) element: Option<usize>,
    pub(in crate::world) ends: (DVec3, DVec3),
    pub(in crate::world) frame: MemberFrame,
    pub(in crate::world) wood: String,
}

impl Bar {
    fn axis(&self) -> DVec3 {
        (self.ends.1 - self.ends.0).normalize_or_zero()
    }

    /// The point of its axis nearest `p`.
    pub(in crate::world) fn nearest(&self, p: DVec3) -> DVec3 {
        let d = self.ends.1 - self.ends.0;
        let t = ((p - self.ends.0).dot(d) / d.length_squared().max(1e-12)).clamp(0.0, 1.0);
        self.ends.0 + d * t
    }

    /// How thick it is along `way` (m).
    pub(in crate::world) fn thickness(&self, way: DVec3) -> f64 {
        2.0 * self.frame.reach(way)
    }

    /// How far its surface reaches from its axis across it, square to `way` (m).
    pub(in crate::world) fn half_across(&self, way: DVec3) -> f64 {
        self.frame.reach(self.axis().cross(way).normalize_or_zero())
    }
}

/// Where a fastener goes between two parts touching: through `head` (the part it goes in from)
/// into `tail`, the points on their axes it runs between (the joint's ends), its way (from
/// `head` into `tail`), and where they touch.
#[derive(Clone, Debug)]
pub(in crate::world) struct Crossing {
    pub(in crate::world) head: Bar,
    pub(in crate::world) tail: Bar,
    pub(in crate::world) on_head: DVec3,
    pub(in crate::world) on_tail: DVec3,
    pub(in crate::world) way: DVec3,
    pub(in crate::world) touch: DVec3,
}

impl Crossing {
    /// The two parts' thicknesses along the fastener (m): the head's, the tail's.
    pub(in crate::world) fn thicknesses(&self) -> (f64, f64) {
        (self.head.thickness(self.way), self.tail.thickness(self.way))
    }
}

/// The gap between bars `a` and `b` (m), the points on their axes it is measured between, and
/// the way from `a`'s to `b`'s; `near`: for bars side by side, where along them.
fn between(a: &Bar, b: &Bar, near: DVec3) -> Option<(f64, DVec3, DVec3, DVec3)> {
    let (pa, pb) = nearest_points(a.ends, b.ends, near);
    let way = (pb - pa).try_normalize()?;
    // Across both (two pieces of one member, end to end, are no crossing).
    if way.dot(a.axis()).abs() > 0.5 || way.dot(b.axis()).abs() > 0.5 {
        return None;
    }
    let gap = (pb - pa).length() - a.frame.reach(way) - b.frame.reach(way);
    Some((gap, pa, pb, way))
}

/// The nearest points of segments `a` and `b`; lying side by side (parallel), those nearest
/// `near`.
fn nearest_points(a: (DVec3, DVec3), b: (DVec3, DVec3), near: DVec3) -> (DVec3, DVec3) {
    let (d1, d2, r) = (a.1 - a.0, b.1 - b.0, a.0 - b.0);
    let (aa, ee, f) = (d1.length_squared(), d2.length_squared(), d2.dot(r));
    let (c, bb) = (d1.dot(r), d1.dot(d2));
    let denom = aa * ee - bb * bb;
    let on = |p: DVec3, s: (DVec3, DVec3)| {
        let d = s.1 - s.0;
        let t = ((p - s.0).dot(d) / d.length_squared().max(1e-12)).clamp(0.0, 1.0);
        s.0 + d * t
    };
    if denom < 1e-6 * aa * ee {
        let pa = on(near, a);
        return (pa, on(pa, b));
    }
    let s = ((bb * f - c * ee) / denom).clamp(0.0, 1.0);
    let t = ((bb * s + f) / ee).clamp(0.0, 1.0);
    let s = ((bb * t - c) / aa).clamp(0.0, 1.0);
    (a.0 + d1 * s, b.0 + d2 * t)
}

impl ServerWorld {
    /// Body `id` as bars: a part not built yet, one; a built structure, each of its members.
    pub(in crate::world) fn bars(&self, id: EntityId) -> Result<Vec<Bar>, String> {
        if let Some(g) = self.part_geometry(id)? {
            let p = &g.part;
            let (half, round) = match p.profile {
                creeer_building::assembly::Profile::Round { start, end } => {
                    let r = 0.25 * (start + end);
                    ([r, r], true)
                }
                creeer_building::assembly::Profile::Board { width, depth } => {
                    ([0.5 * width, 0.5 * depth], false)
                }
            };
            let axis = (p.end - p.start).normalize_or_zero();
            let y =
                p.up.reject_from(axis)
                    .normalize_or(axis.any_orthonormal_vector());
            return Ok(vec![Bar {
                id,
                element: None,
                ends: (p.start, p.end),
                frame: MemberFrame {
                    a: 0,
                    b: 0,
                    y,
                    z: axis.cross(y),
                    half,
                    round,
                },
                wood: g.wood,
            }]);
        }
        let s = &self.world.bodies[self.index(id).ok_or("it is gone")?].structure;
        let record = self.building.built.get(&id).ok_or("no record")?;
        Ok((0..s.element_count())
            .filter_map(|e| {
                let frame = s.member_frame(e)?;
                let wood = record.woods.get(e).cloned().flatten()?;
                Some(Bar {
                    id,
                    element: Some(e),
                    ends: (s.position[frame.a], s.position[frame.b]),
                    frame,
                    wood,
                })
            })
            .collect())
    }

    /// Two members of built structure `id` touching each other nearest `at` (within
    /// `NEAR` of it): their elements and where they touch.
    pub(in crate::world) fn touching_within(
        &self,
        id: EntityId,
        at: DVec3,
    ) -> Option<(usize, usize, DVec3)> {
        if !self.building.built.contains_key(&id) {
            return None;
        }
        let bars = self.bars(id).ok()?;
        let close: Vec<&Bar> = (bars.iter())
            .filter(|b| {
                (b.nearest(at) - at).length() < super::NEAR + b.frame.half[0].max(b.frame.half[1])
            })
            .collect();
        let mut best: Option<(usize, usize, DVec3, f64)> = None;
        for (k, a) in close.iter().enumerate() {
            for b in &close[k + 1..] {
                let shares = |x: &Bar, y: &Bar| x.frame.a == y.frame.a || x.frame.a == y.frame.b;
                if shares(a, b) || shares(b, a) {
                    continue;
                }
                let Some((gap, pa, _, way)) = between(a, b, at) else {
                    continue;
                };
                let touch = pa + way * a.frame.reach(way);
                let off = (touch - at).length();
                if gap < super::TOUCHING && off < super::NEAR && best.is_none_or(|x| off < x.3) {
                    best = Some((a.element?, b.element?, touch, off));
                }
            }
        }
        best.map(|(a, b, touch, _)| (a, b, touch))
    }

    /// What part `part` touches nearest `at` to be fastened to (another part, a standing tree,
    /// or another member of its own structure), and the point between them.
    pub(in crate::world) fn touching_at(
        &self,
        part: EntityId,
        at: DVec3,
    ) -> Result<(Against, DVec3), String> {
        let outside = self.touching_any(part, at).map(|(t, p)| {
            let against = match t {
                super::tree_joints::Touched::Part(id) => Against::Part(id),
                super::tree_joints::Touched::Tree(tree) => Against::Tree(tree),
            };
            (against, p)
        });
        let within = self.touching_within(part, at);
        match (outside, within) {
            (Ok((t, p)), Some((a, b, q))) if (q - at).length() < (p - at).length() => {
                let _ = t;
                Ok((Against::Itself(a, b), q))
            }
            (Ok(found), _) => Ok(found),
            (Err(_), Some((a, b, q))) => Ok((Against::Itself(a, b), q)),
            (Err(e), None) => Err(e),
        }
    }

    /// Where a fastener between part `part` and what it touches (`against`, at `touch`) goes,
    /// driven by `by` (none: a scene's world, through `part`): from the side they stand on, or
    /// through the thinner seen edge-on.
    pub(in crate::world) fn crossing(
        &self,
        by: Option<EntityId>,
        part: EntityId,
        against: Against,
        touch: DVec3,
    ) -> Result<Crossing, String> {
        let nearest = |bars: Vec<Bar>| {
            bars.into_iter()
                .min_by(|x, y| {
                    (x.nearest(touch) - touch)
                        .length()
                        .total_cmp(&(y.nearest(touch) - touch).length())
                })
                .ok_or_else(|| "no member there".to_owned())
        };
        let (a, b) = match against {
            // Of the two bodies' members, the two that touch nearest where they touch (a deck
            // board laid beside a nailed one: nearest the touch on the stringer under it, the
            // nailed board's member was taken, and they were to be pegged side by side).
            Against::Part(other) => {
                let (mine, theirs) = (self.bars(part)?, self.bars(other)?);
                let touching = (mine.iter())
                    .flat_map(|a| theirs.iter().map(move |b| (a, b)))
                    .filter_map(|(a, b)| {
                        let (gap, pa, _, way) = between(a, b, touch)?;
                        let at = pa + way * a.frame.reach(way);
                        (gap < super::TOUCHING + 0.02).then(|| (a, b, (at - touch).length()))
                    })
                    .min_by(|x, y| x.2.total_cmp(&y.2))
                    .map(|(a, b, _)| (a.clone(), b.clone()));
                match touching {
                    Some(pair) => pair,
                    None => (nearest(mine)?, nearest(theirs)?),
                }
            }
            Against::Itself(ea, eb) => {
                let bars = self.bars(part)?;
                let of = |e: usize| {
                    (bars.iter().find(|b| b.element == Some(e)))
                        .cloned()
                        .ok_or_else(|| "no member there".to_owned())
                };
                (of(ea)?, of(eb)?)
            }
            Against::Tree(_) => return Err("a tree is no part".into()),
        };
        let (_, pa, pb, way) = between(&a, &b, touch).ok_or("they do not cross there")?;
        let touch = pa + way * a.frame.reach(way);
        // From the side the player stands on: the part between their eyes and the other. Seen
        // edge-on (the eyes within a fifth of the way of the joint's plane), through the thinner.
        let eye = by.and_then(|by| self.index(by)).map(|i| {
            let centre = self.world.bodies[i].structure.position[0];
            creeer_sim::reach::eye(centre, character::plumb(&self.world, centre))
        });
        let seen = eye.map_or(0.0, |e| (e - touch).normalize_or_zero().dot(way));
        let a_first = if seen.abs() > 0.2 {
            seen < 0.0
        } else if by.is_some() {
            a.thickness(way) <= b.thickness(way)
        } else {
            true
        };
        Ok(if a_first {
            Crossing {
                head: a,
                tail: b,
                on_head: pa,
                on_tail: pb,
                way,
                touch,
            }
        } else {
            Crossing {
                head: b,
                tail: a,
                on_head: pb,
                on_tail: pa,
                way: -way,
                touch,
            }
        })
    }
}
