//! A builder at the bridge site (G2.11 A5, ADR-0043): what a client's frames hold while building,
//! played by one player through her own connection as a player plays (her eyes, her crosshair,
//! her hands). Gated in CI by her prediction's Newton iterations a tick
//! (`bridge_bench_tests.rs`); played with the frame drawn by `creeer_client --play-bench bridge`.
//!
//! The world is *bridge_demo*'s (ADR-0039): the site's gully with its footbridge built and nailed
//! across it, and the spare timber lying in rows north of the start. The builder:
//! 1. walks in among the rows to the first handrail post (bodies at her feet);
//! 2. takes it in both hands;
//! 3. carries it before her (the hands steered, `place.rs`) to the near bank beside the bridge;
//! 4. stands there holding it, looking at the nailed bridge;
//! 5. puts it down, and stands.

use super::Connection;
use crate::ServerWorld;
use crate::world::site::{BridgeSite, OUT, WIDTH};
use creeer_math::glam::{DMat3, DQuat, DVec3};
use creeer_net::message::{Command, Target};
use creeer_sim::reach;
use creeer_terrain::Terrain;
use std::sync::Arc;

#[cfg(test)]
#[path = "bridge_bench_tests.rs"]
mod tests;

/// The site, its bridge built and nailed (a creative server's), and where its things are.
pub fn site(terrain: Arc<Terrain>) -> (ServerWorld, BridgeSite) {
    let w = crate::play::bridge_demo::bridge_demo_world(terrain);
    let site = BridgeSite::of(&w, 1);
    (w, site)
}

/// A view looking along `forward` (a camera's turn: −z ahead, y up), level with `up`.
pub fn looking(forward: DVec3, up: DVec3) -> DQuat {
    let right = forward.cross(up).normalize();
    DQuat::from_mat3(&DMat3::from_cols(right, right.cross(forward), -forward)).normalize()
}

/// A walk along the ground from `from` towards `to`, stopping `near` m from it (m/s).
fn towards(from: DVec3, to: DVec3, near: f64) -> DVec3 {
    let d = (to - from).reject_from(from.normalize());
    if d.length() > near {
        d.normalize() * (d.length() - near).clamp(0.4, 1.4)
    } else {
        DVec3::ZERO
    }
}

/// What the builder is doing (and for how many ticks).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Doing {
    #[default]
    ToThePost,
    Taking(u32),
    Carrying(u32),
    Holding(u32),
    PutDown(u32),
}

/// The builder, from what her connection knows and where the site's things were laid.
#[derive(Debug)]
pub struct Builder {
    pub doing: Doing,
    site: BridgeSite,
    /// The turn the hands were last asked to hold the part at.
    asked: Option<DQuat>,
}

impl Builder {
    /// A builder at `site`.
    pub fn new(site: BridgeSite) -> Self {
        Self {
            doing: Doing::default(),
            site,
            asked: None,
        }
    }

    /// Whether she has taken the post, and stood beside the bridge holding it.
    pub fn done(&self) -> (bool, bool) {
        (
            !matches!(self.doing, Doing::ToThePost | Doing::Taking(_)),
            matches!(self.doing, Doing::Holding(_) | Doing::PutDown(_)),
        )
    }

    /// This tick's acts on `c` (commands sent, the hands steered): her walk (m/s) and her view.
    pub fn act(&mut self, c: &mut Connection) -> (DVec3, DQuat) {
        let Some(me) = c.avatar().map(|a| a.position) else {
            return (DVec3::ZERO, DQuat::IDENTITY);
        };
        let up = me.normalize();
        let eye = reach::eye(me, up);
        let view_to = |p: DVec3| looking((p - eye).normalize(), up);
        let post = self.site.item("handrail post", 0);
        let at = post
            .and_then(|id| c.poses().get(&id).map(|p| p.position))
            .unwrap_or(me);
        // Beside the bridge's near end (south of its deck, short of the rim), and its middle.
        let beside = self.site.at(OUT - WIDTH * 0.5 - 0.6, -1.0);
        let bridge = self.site.middle;
        match self.doing {
            Doing::ToThePost => {
                let walk = towards(me, at, 0.9);
                if walk == DVec3::ZERO {
                    self.doing = Doing::Taking(0);
                }
                (walk, view_to(at))
            }
            Doing::Taking(t) => {
                if t % 40 == 0
                    && let Some(aimed) = c.aim(eye, (at - eye).normalize(), 3.0)
                {
                    c.command(Command::Grab(Target {
                        body: aimed.body,
                        at: aimed.point,
                        seen: aimed.seen,
                    }));
                }
                let taken = c.own_load().is_some() && c.own_load() == post;
                self.doing = if taken {
                    Doing::Carrying(0)
                } else {
                    Doing::Taking(t + 1)
                };
                (DVec3::ZERO, view_to(at))
            }
            Doing::Carrying(t) => {
                // Carried before her, looking where she goes.
                let walk = towards(me, beside, 0.15);
                let ahead = eye + (beside - me).reject_from(up).normalize_or_zero() * 3.0;
                let view = view_to(ahead);
                self.hold(c, view);
                self.doing = if walk == DVec3::ZERO || t > 900 {
                    Doing::Holding(0)
                } else {
                    Doing::Carrying(t + 1)
                };
                (walk, view)
            }
            Doing::Holding(t) => {
                let view = view_to(bridge);
                self.hold(c, view);
                if t >= 180 {
                    c.set_hold(None);
                    c.command(Command::Drop);
                    self.doing = Doing::PutDown(0);
                } else {
                    self.doing = Doing::Holding(t + 1);
                }
                (DVec3::ZERO, view)
            }
            Doing::PutDown(t) => {
                self.doing = Doing::PutDown(t + 1);
                (DVec3::ZERO, view_to(bridge))
            }
        }
    }

    /// The hands steered to what the view `view` meets (the part lying across it), as the
    /// client's `holding.rs` places it (from the turn it last asked for).
    fn hold(&mut self, c: &mut Connection, view: DQuat) {
        let surface = c.surface_aimed(view, |_, _, _| None);
        let hold = c.place_hold(view, surface, (0.0, 0.0, 0.0), self.asked);
        self.asked = hold.map(|h| h.turn);
        c.set_hold(hold);
    }
}
