//! A dig spot's hole (G2.9, ADR-0032 amended): one brush that grows with every blow, a dent,
//! then a shaft a post stands in, then wider, sized so the ground loses exactly what was dug
//! there, measured only where it grows (`creeer_terrain::dug`).

use super::digging::{HOLE, Pit, REACH, WIDEST, spot};
use super::*;
use creeer_terrain::dug::Field;

/// A spot's hole at `centre` grown to `t` (G2.9): a dent (0–1: a ball up to the hole's radius),
/// then a shaft down to [`REACH`] (1–2), then wider to [`WIDEST`] (2–3). Each contains the
/// ones before, so a hole only grows.
fn hole(centre: DVec3, t: f64) -> Brush {
    let down = -centre.normalize();
    match t {
        t if t <= 1.0 => Brush::Sphere {
            center: centre,
            radius: HOLE * t.max(1e-3),
        },
        t if t <= 2.0 => Brush::Capsule {
            a: centre,
            b: centre + down * (REACH * (t - 1.0)),
            radius: HOLE,
        },
        t => Brush::Capsule {
            a: centre,
            b: centre + down * REACH,
            radius: HOLE + (WIDEST - HOLE) * (t - 2.0).min(1.0),
        },
    }
}

impl ServerWorld {
    /// Spot `key`'s hole grown to take what was dug there (G2.9: every blow shows). Grown in
    /// its place in the edit log while no later edit comes near it; else a new brush begins
    /// (a hole backfilled and dug again: grown in place it would lie under the fill).
    pub(super) fn cut_pit(&mut self, key: [i32; 3]) {
        let Some(pit) = self.digging.pits.get(&key).cloned() else {
            return;
        };
        let (own, from) = match pit.brush {
            Some((k, t)) if self.last_near(k) => (Some(k), t),
            _ => (None, 0.0),
        };
        // A new brush takes what is owed; grown, what it owes beyond what it takes.
        let (want, took_before) = match own {
            Some(_) => (pit.volume, pit.took),
            None => (pit.owed(), 0.0),
        };
        // Measured about where it will reach, on a grid of a fifth of its radius: a new brush, a
        // third more than flat ground would need; grown, three times the growth flat ground
        // would need, and only where it reaches beyond what it was (what it falls short of is
        // owed, taken by the next blow). (Bracketed by the whole hole, a deep shaft's every blow
        // reached into widening it, its whole wall measured: 2.5 ms a blow at 0.9 m, G2.9.)
        let flat = match own {
            Some(_) => from + 3.0 * (flat_hole(want) - flat_hole(took_before)).max(0.0) + 0.01,
            None => flat_hole(want * 1.33),
        }
        .max(from + 1e-3)
        .min(3.0);
        let box_ = measured(pit.centre, flat);
        let shape = |t| hole(pit.centre, t);
        let (t, took) = match own {
            Some(_) => {
                let (old, new) = (shape(from), shape(flat));
                let field = Field::beyond_without(&self.terrain, (old, new), box_, own);
                let (t, more) = field.grown_for(want - took_before, (shape, true), (from, flat));
                (t, took_before + more)
            }
            None => {
                let field = Field::in_box_without(&self.terrain, box_.0, (box_.1, box_.2), own);
                field.grown_for(want, (shape, false), (0.0, flat))
            }
        };
        let edit = Edit::Dig(hole(pit.centre, t));
        let index = match own {
            Some(k) => {
                self.terrain.replace_edit(k, edit);
                k
            }
            None => {
                self.terrain.edit(edit);
                self.terrain.edits().len() - 1
            }
        };
        self.digging.cut += took - took_before;
        if let Some(p) = self.digging.pits.get_mut(&key) {
            if own.is_none() {
                p.volume = want;
            }
            (p.took, p.brush) = (took, Some((index, t)));
        }
        // Grown as wide as it grows: what it could not take digs on in its floor, a new spot
        // (else every blow there measured it again, to cut nothing).
        if t >= 3.0 - 1e-9
            && let Some(done) = self.digging.pits.remove(&key)
        {
            let floor = done.centre - done.centre.normalize() * (REACH + WIDEST);
            let mut next = Pit::new(floor, done.owed().max(0.0));
            next.heap = done.heap;
            self.digging.pits.entry(spot(floor)).or_insert(next);
        }
        let (lo, hi) = edit.brush().bounds();
        self.world
            .wake_near(0.5 * (lo + hi), 0.5 * (hi - lo).length() + 1.0);
    }

    /// Whether no fill after edit `k` comes near where it is or may grow to: digs after it
    /// take air from the same ground in either order (taken as any edit, a hole beside another
    /// began anew every blow: 3,000 edits in 3,000 blows, each blow slower, G2.9).
    fn last_near(&self, k: usize) -> bool {
        let edits = self.terrain.edits();
        let Some(edit) = edits.log().get(k) else {
            return false;
        };
        let (lo, hi) = edit.brush().bounds();
        let (lo, hi) = (lo - WIDEST - 1.0, hi + WIDEST + 1.0);
        !(edits.log()[k + 1..].iter()).any(|e| {
            let (blo, bhi) = e.brush().bounds();
            matches!(e, Edit::Fill(..)) && blo.cmple(hi).all() && bhi.cmpge(lo).all()
        })
    }
}

/// Where a hole at `centre` grown to `t` is measured: the box about its axis (its centre, its
/// half sizes east, north and up) with its round, and the grid's step (a fifth of its radius).
fn measured(centre: DVec3, t: f64) -> (DVec3, DVec3, f64) {
    let brush = hole(centre, t);
    let r = brush.radius();
    let step = (0.2 * r).clamp(0.015, 0.1);
    // A brush narrower than the finest grid's round is rounded over its own radius at most.
    let round = creeer_terrain::edits::round(creeer_terrain::edits::FINEST_CELL).min(r);
    let depth = match brush {
        Brush::Capsule { a, b, .. } => (a - b).length(),
        Brush::Sphere { .. } => 0.0,
    };
    let side = r + round + step;
    let up = centre.normalize();
    (
        centre - up * (0.5 * depth),
        DVec3::new(side, side, 0.5 * depth + side),
        step,
    )
}

/// The hole's parameter that takes `volume` m³ out of flat ground (a first guess): a half ball,
/// then a shaft with a half-ball end, then wider.
fn flat_hole(volume: f64) -> f64 {
    let pi = std::f64::consts::PI;
    let dent = 2.0 / 3.0 * pi * HOLE * HOLE * HOLE;
    if volume <= dent {
        return creeer_math::det::cbrt(volume / dent);
    }
    let shaft = dent + pi * HOLE * HOLE * REACH;
    if volume <= shaft {
        return 1.0 + (volume - dent) / (pi * HOLE * HOLE * REACH);
    }
    let wide = |r: f64| 2.0 / 3.0 * pi * r * r * r + pi * r * r * REACH;
    // Its radius, by bisection.
    let (mut lo, mut hi) = (HOLE, WIDEST);
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if wide(mid) < volume {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    2.0 + (lo - HOLE) / (WIDEST - HOLE)
}
