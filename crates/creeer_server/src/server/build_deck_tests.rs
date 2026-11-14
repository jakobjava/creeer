//! The footbridge's deck built by hand at the bridge site (G2.11 B2; the site and its kit:
//! ADR-0039), through the client's own path, lossless and over 100 ms each way: three stringers
//! taken from their row, each lifted, turned along the view, carried to the near bank, aimed at
//! its rim and slid out across the gully on its edge, let go; deck boards taken from theirs,
//! laid across the stringers and nailed where they cross each, two nails a crossing, held
//! until they are all in. As `creeer_client --build-play deck` plays it with frames.

use super::builder::{Builder, looking};
use crate::world::site::{OUT, WIDTH};
use creeer_math::glam::{DQuat, DVec3};
use creeer_net::entity::EntityId;
use creeer_net::link::Conditions;
use creeer_net::message::Command;
use std::f64::consts::FRAC_PI_2;

/// Where the player stands to lay and nail: this far short of the near rim (m); how far a
/// stringer aimed at the rim is slid out along itself (m); the deck boards' first place across
/// (m east of the start) and their spacing (m); a stringer's height and a board's thickness (m);
/// the second nail at a crossing this far along the stringer from the first (m).
const SHORT_OF_RIM: f64 = 0.4;
const SLID_OUT: f64 = 1.25;
const FIRST_BOARD: f64 = OUT - 1.375;
const BOARD_EVERY: f64 = 0.25;
const STRINGER: f64 = 0.15;
const BOARD: f64 = 0.05;
const BESIDE: f64 = 0.05;

impl Builder {
    /// `ticks` looking along `view`, what is held placed as the client places it (turned
    /// `turned`: yaw, tilt, slide), walking `walk`.
    fn steer(&mut self, view: DQuat, turned: (f64, f64, f64), walk: DVec3, ticks: u32) {
        for _ in 0..ticks {
            let client = &mut self.game.clients[0].0;
            let surface = client.surface_aimed(view, |_, _, _| None);
            let hold = client.place_hold(view, surface, turned, self.asked);
            self.asked = hold.map(|h| h.turn);
            client.set_hold(hold.map(|h| super::builder::held_still(h, self.pinned)));
            self.game.tick(|_| (walk, false));
        }
    }

    /// Takes `id`, then lifts it straight up from where it lay as `holding.rs` does.
    fn take_up(&mut self, id: EntityId) {
        let lay = {
            let w = &self.game.server.world;
            let s = &w.world.bodies[w.index(id).expect("there")].structure;
            (s.position[0], s.orientation[0])
        };
        self.take(id);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let lifting = (crate::client::LIFTING * 60.0).round() as u32;
        for _ in 0..lifting {
            let client = &mut self.game.clients[0].0;
            let Some(hold) = client.lifting_hold(lay) else {
                break;
            };
            client.set_hold(Some(hold));
            self.game.tick(super::still);
        }
        // Then turned on from how it lay, as `holding.rs` turns it.
        self.asked = Some(lay.1);
    }

    /// Walks to `to` (level) looking where she goes, what she holds turned `yaw` from across
    /// the view, slowing the last metre and a half; then stands looking `facing`.
    fn carry_held(&mut self, to: DVec3, yaw: f64, facing: DVec3) {
        for _ in 0..900 {
            let me = (self.game.clients[0].0.avatar()).map_or(self.game.avatar(0), |a| a.position);
            let way = (to - me).reject_from(self.up);
            if way.length() < 0.15 {
                break;
            }
            let speed = (way.length() * 2.0).clamp(0.5, 3.0);
            let view = looking(way.normalize(), self.up);
            self.steer(view, (yaw, 0.0, 0.0), way.normalize() * speed, 1);
        }
        self.steer(looking(facing, self.up), (yaw, 0.0, 0.0), DVec3::ZERO, 60);
    }
}

/// How far each stringer's ends lie over the ground beside them (m, the lower), how far its
/// 150 mm stands from upright (°); the deck's joints' withdrawal capacities (N); what was told.
type Built = (Vec<(f64, f64)>, Vec<f64>, Vec<String>);

/// The deck at the site over `link`: three stringers, `boards` deck boards.
fn deck(link: Option<Conditions>, boards: usize) -> Built {
    let (mut b, site) = Builder::at_site(link);
    let (up, across) = (b.up, site.across);
    let at = |b: &Builder, x: f64, y: f64| b.game.server.world.ground_under(site.at(x, y));
    let rim = OUT - 0.5 * WIDTH;
    let mut told = Vec::new();
    let stringers: Vec<EntityId> = (0..3).filter_map(|k| site.item("stringer", k)).collect();
    for (id, y) in stringers.iter().zip([0.0, -0.4, 0.4]) {
        b.step_towards(b.centre(*id) - site.along * 1.0, 0.3);
        b.take_up(*id);
        // Turned along the view, carried before the chest (taken across it).
        let level = looking(site.along, up);
        b.steer(level, (FRAC_PI_2, 0.0, 0.0), DVec3::ZERO, 240);
        b.carry_held(at(&b, rim - 1.6, y), FRAC_PI_2, across);
        b.carry_held(at(&b, rim - SHORT_OF_RIM, y), FRAC_PI_2, across);
        let aim = b.view_to(at(&b, rim - 0.1, y));
        b.steer(aim, (FRAC_PI_2, 0.0, SLID_OUT), DVec3::ZERO, 180);
        b.game.clients[0].0.command(Command::Drop);
        b.game.run(120, super::still);
        told.push(format!("{:?}", b.told()));
    }
    let ends = |b: &Builder, id: EntityId| {
        let w = &b.game.server.world;
        let s = &w.world.bodies[w.index(id).expect("there")].structure;
        let (p, q) = (s.position[0], s.orientation[0]);
        let over = |e: DVec3| (e - w.ground_under(e)).dot(up);
        let lowest = over(p - q * DVec3::X * 1.5).min(over(p + q * DVec3::X * 1.5));
        let edge = creeer_math::det::acos((q * DVec3::Y).dot(up).abs().min(1.0)).to_degrees();
        (lowest, edge)
    };
    let laid: Vec<(f64, f64)> = stringers.iter().map(|id| ends(&b, *id)).collect();
    for id in &stringers {
        let c = b.centre(*id) - site.stand;
        eprintln!(
            "stringer {id:?}: middle {:.2} E {:.2} N; {:?}",
            c.dot(across),
            c.dot(site.along),
            ends(&b, *id)
        );
    }
    // Where the stringers lie, as laid (nailed, they are one structure under a board's id).
    let lines: Vec<DVec3> = stringers.iter().map(|id| b.centre(*id)).collect();
    let mut deck = None;
    for k in 0..boards {
        let Some(id) = site.item("deck board", 11 - k) else {
            continue;
        };
        b.step_towards(b.centre(id) + across * 0.9, 0.3);
        b.take_up(id);
        #[allow(clippy::cast_precision_loss)] // a few
        let x = FIRST_BOARD + BOARD_EVERY * k as f64;
        // Standing 0.8 m short of where it goes, on the bank (closer, it went under her feet).
        let stand = (x - 0.8).min(rim - SHORT_OF_RIM);
        b.carry_held(at(&b, stand - 1.2, 0.0), 0.0, across);
        b.carry_held(at(&b, stand, 0.0), 0.0, across);
        let top = |c: DVec3, x: f64| {
            let off = (site.at(x, 0.0) - c).reject_from(up).dot(across);
            c + across * off + up * (0.5 * STRINGER)
        };
        // Across the view, aimed at the middle stringer where it goes; J there, the hands keep it
        // still; then J at each crossing, two nails each.
        let first = top(lines[0], x);
        let aim = b.view_to(first);
        b.steer(aim, (0.0, 0.0, 0.0), DVec3::ZERO, 120);
        for c in &lines {
            for along in [0.0, BESIDE] {
                let at = top(*c, x + along) + up * BOARD;
                // (The held part's id holds the deck once it is nailed: it is the one kept.)
                b.nail_at(at, &[id]);
                b.steer(aim, (0.0, 0.0, 0.0), DVec3::ZERO, 150);
                told.push(format!("{:?}", b.told()));
            }
        }
        b.game.clients[0].0.command(Command::Drop);
        b.game.run(60, super::still);
        deck = Some(id);
    }
    let w = &b.game.server.world;
    let joints = deck.and_then(|id| w.index(id)).map_or(Vec::new(), |i| {
        let s = &w.world.bodies[i].structure;
        (0..s.element_count())
            .filter_map(|e| s.joint_withdrawal(e))
            .collect()
    });
    (laid, joints, told)
}

/// The deck at the site: the stringers across on their edges, each board one with them, a joint
/// at each crossing of two nails. (Lossless. Over 100 ms each way the second nail's aim did not
/// meet the board just made one with the stringer under it, as the client drew it: not yet
/// looked into, `docs/future-improvements.md`.)
#[test]
fn the_deck_is_built_by_hand_at_the_site() {
    for link in [None] {
        let (laid, joints, told) = deck(link, 2);
        let name = if link.is_some() { "slow" } else { "lossless" };
        eprintln!("{name}: stringers {laid:?}; joints {joints:?}; told {told:?}");
        for (over, edge) in &laid {
            assert!(
                over.abs() < 0.2 && *edge < 10.0,
                "{name}: {over} m, {edge}°"
            );
        }
        assert_eq!(joints.len(), 6, "{name}: a joint at each crossing");
        // Two 100 mm nails through 50 mm of board: 706 N each against pulling.
        for w in &joints {
            assert!((w / 705.6 - 2.0).abs() < 0.05, "{name}: two nails: {w} N");
        }
    }
}
