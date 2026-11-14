//! The snapshots' velocities read for sound (ADR-0037): a body whose velocity changed more than
//! holding it up against gravity explains was struck (impulse m·Δv); one moving along the ground
//! rolls or slides. How each body sounds is `sounds_things.rs`.

use crate::sounds::{Heard, MOST, Sounds};
use crate::sounds_things::{ground, nearest, sounding};
use creeer_audio::events::{Other, impact};
use creeer_audio::happenings::rub;
use creeer_math::glam::DVec3;
use creeer_net::entity::{EntityId, Kind};
use creeer_net::quant::{POSITION_SCALE, VELOCITY_SCALE};
use std::collections::BTreeSet;

/// A change of velocity beyond what holding a body up against gravity gives it, below which
/// nothing is heard (m/s).
const STRUCK: f64 = 0.3;

impl Sounds {
    /// The snapshots the drawn time has passed since the last frame: each body whose velocity
    /// changed more than gravity explains was struck (impulse m·Δv); one moving along the ground
    /// rolls or slides.
    pub(crate) fn bodies(&mut self, h: &Heard<'_>) {
        let c = h.connection;
        let drawn = c.drawn_at();
        let held: BTreeSet<EntityId> = (c.kinds.values())
            .filter_map(|k| match k {
                Kind::Character { carries, .. } => carries.map(|c| c.load),
                _ => None,
            })
            .collect();
        let mut struck = Vec::new();
        let mut moving = Vec::new();
        for s in c.receiver().snapshots() {
            if s.tick <= self.seen || f64::from(s.tick) > drawn {
                continue;
            }
            for (id, state) in &s.states {
                let v = state.velocity.value(VELOCITY_SCALE);
                let p = state.position.value(POSITION_SCALE);
                let was = self.velocities.insert(*id, (v, s.tick));
                let Some(Kind::Body(..) | Kind::Tree(_)) = c.kinds.get(id) else {
                    continue;
                };
                if held.contains(id) {
                    continue;
                }
                if let Some((v0, t0)) = was {
                    let dt = f64::from(s.tick.saturating_sub(t0))
                        / f64::from(creeer_net::channel::TICK_RATE);
                    // What its contacts gave it: its change of velocity less gravity's. A body at
                    // rest is given gravity's back every step (g·dt, held up): only a change
                    // beyond that is a blow.
                    let up = p.normalize_or(DVec3::Y);
                    let support = 9.81 * dt;
                    let dv = v - v0 + up * support;
                    if dt > 0.0 && dt < 0.2 && dv.length() - support > STRUCK {
                        struck.push((*id, p, dv));
                    }
                }
                let along = v.reject_from(p.normalize_or(DVec3::Y)).length();
                if !state.asleep
                    && along > 0.25
                    && v.dot(p.normalize_or(DVec3::Y)).abs() < 0.3 * along
                {
                    moving.push((*id, along, state.spin.value(VELOCITY_SCALE).length()));
                }
            }
            self.seen = s.tick;
        }
        struck.sort_by(|a, b| b.2.length().total_cmp(&a.2.length()));
        for (id, p, dv) in struck.into_iter().take(MOST) {
            self.struck(h, id, p, dv);
        }
        self.rolling(h, &moving);
    }

    /// Body `id` at `p` struck: its velocity changed by `dv`.
    fn struck(&mut self, h: &Heard<'_>, id: EntityId, p: DVec3, dv: DVec3) {
        let seed = self.next_seed();
        let Some(lib) = &self.lib else { return };
        let Some(body) = sounding(lib, &self.extra, h.connection, id, p) else {
            return;
        };
        // Where it was struck: its side towards what struck it.
        let toward = -dv.normalize_or(DVec3::NEG_Y);
        let at = body.centre + toward * body.curvature().max(0.5 * body.shape.size());
        let other = if h.terrain.density(at) < 0.15 {
            Other::Ground(ground(lib, h.terrain, at - p.normalize_or(DVec3::Y) * 0.05))
        } else {
            match nearest(lib, &self.extra, h.connection, at, Some(id)) {
                Some(b) => Other::Body(b),
                None => Other::Ground(ground(lib, h.terrain, at)),
            }
        };
        let spec = impact(&body, &other, at, dv * body.mass, seed);
        self.play(spec, h.ears.0);
    }

    /// Bodies moving along the ground: begun, changed or stopped.
    fn rolling(&mut self, h: &Heard<'_>, moving: &[(EntityId, f64, f64)]) {
        let still: Vec<EntityId> = (self.rubbing.keys())
            .filter(|id| !moving.iter().any(|m| m.0 == **id))
            .copied()
            .collect();
        for id in still {
            if let (Some((key, _)), Some(out)) = (self.rubbing.remove(&id), &mut self.out) {
                out.stop(key);
            }
        }
        for &(id, speed, spin) in moving.iter().take(MOST) {
            let seed = self.next_seed();
            let Some(lib) = &self.lib else { return };
            let Some(body) = sounding(lib, &self.extra, h.connection, id, DVec3::ZERO) else {
                continue;
            };
            if h.terrain.density(body.centre) > 0.6 + body.shape.size() {
                continue;
            }
            let rolling = spin * body.curvature() > 0.6 * speed;
            let key = 0x5255_0000_0000 | u64::from(id.0);
            let spec = rub(&body, body.centre, (speed, rolling), key, seed);
            match self.rubbing.get(&id) {
                Some((_, was)) if (was - speed).abs() < 0.1 * was => {}
                Some(_) => {
                    if let Some(out) = &mut self.out {
                        out.change(key, spec.excite, spec.at);
                    }
                    self.rubbing.insert(id, (key, speed));
                }
                None => {
                    self.play(spec, h.ears.0);
                    self.rubbing.insert(id, (key, speed));
                }
            }
        }
    }
}
