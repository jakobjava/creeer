//! Machines in the world (F7.4, ADR-0020): run in full near players, abstract far from
//! all of them (`creeer_sim::abstraction`), caught up beside the ticks when a player comes
//! near.
//!
//! - **Active** within [`lod::PARK`] of a player: ticked with the world.
//! - **Leaving** beyond [`lod::PARK`] of every player once steady: from its own last windows
//!   (no probe; a machine never steady stays active).
//! - **Abstract**: nothing per tick; advanced only when needed.
//! - **Catching up** when a player comes within [`lod::PARK`]: on a thread of its own, to the
//!   time [`CATCH`] ticks on, and taken in on that tick (waited for if not done: what it
//!   finds never depends on how fast the thread ran). Then active again if a player is within
//!   [`lod::ACTIVE`].

use super::{DT, ServerWorld};
use creeer_math::glam::DVec3;
use creeer_net::message::MachineView;
use creeer_sim::abstraction::{Abstract, Mode};
use creeer_sim::lod;
use creeer_sim::powerhouse::Powerhouse;
use rayon::prelude::*;
use std::thread::JoinHandle;

/// Ticks a catching up takes (it runs beside them).
pub const CATCH: u64 = 60;
/// Active machines from which their ticks run side by side.
const PARALLEL: usize = 4;

type House = Box<Abstract<Powerhouse>>;

/// How a placed machine is run now.
#[derive(Debug)]
enum State {
    Active(House),
    Abstract(House),
    /// Caught up on a thread, taken in on tick `until`.
    Catching {
        job: JoinHandle<House>,
        until: u64,
    },
    /// Taken for a moment (never seen outside a method).
    Moved,
}

/// A machine in the world.
#[derive(Debug)]
pub struct Placed {
    pub id: u32,
    pub at: DVec3,
    /// The world's tick it was built on (its time is the world's since).
    pub born: u64,
    state: State,
}

impl Placed {
    /// A saved machine, `built` anew and restored.
    pub fn restored(saved: &creeer_sim::save::SavedMachine, built: Powerhouse) -> Self {
        let house = Box::new(Abstract::restore(built, &saved.snapshot));
        let state = if saved.snapshot.abstract_mode {
            State::Abstract(house)
        } else {
            State::Active(house)
        };
        Self {
            id: saved.id,
            at: saved.at,
            born: saved.born,
            state,
        }
    }

    /// How it is run now.
    pub fn mode(&self) -> Option<Mode> {
        match &self.state {
            State::Active(_) => Some(Mode::Active),
            State::Abstract(_) => Some(Mode::Abstract),
            State::Catching { .. } | State::Moved => None,
        }
    }

    /// The machine, unless it is catching up.
    pub fn house(&self) -> Option<&Abstract<Powerhouse>> {
        match &self.state {
            State::Active(h) | State::Abstract(h) => Some(h),
            _ => None,
        }
    }

    /// Takes in its catching up (waiting for it if it is not done).
    fn join(&mut self) {
        if let State::Catching { .. } = self.state {
            let State::Catching { job, .. } = std::mem::replace(&mut self.state, State::Moved)
            else {
                unreachable!("catching")
            };
            self.state = State::Abstract(job.join().expect("a machine's catching up"));
        }
    }
}

impl ServerWorld {
    /// Builds a powerhouse at `at`, active; returns its id.
    pub fn place_powerhouse(&mut self, at: DVec3) -> u32 {
        let id = self.next_machine;
        self.next_machine += 1;
        self.machines.push(Placed {
            id,
            at,
            born: self.tick,
            state: State::Active(Box::new(Abstract::new(Powerhouse::new()))),
        });
        id
    }

    /// The machines.
    pub fn machines(&self) -> &[Placed] {
        &self.machines
    }

    /// One tick of the active machines (side by side when there are several: each is its own;
    /// abstract ones cost nothing, not even a parallel loop's waking of the workers).
    pub(super) fn tick_machines(&mut self) {
        let mut active: Vec<&mut House> = (self.machines.iter_mut())
            .filter_map(|m| match &mut m.state {
                State::Active(house) => Some(house),
                _ => None,
            })
            .collect();
        if active.len() >= PARALLEL {
            active.par_iter_mut().for_each(|house| house.tick());
        } else {
            active.iter_mut().for_each(|house| house.tick());
        }
    }

    /// The machines' part of the LOD pass (every [`lod::PERIOD`] ticks).
    pub(super) fn machine_regions(&mut self, centres: &[DVec3]) {
        let tick = self.tick;
        for m in &mut self.machines {
            let near = centres
                .iter()
                .map(|c| (*c - m.at).length())
                .fold(f64::INFINITY, f64::min);
            #[allow(clippy::cast_precision_loss)]
            let at_tick = |t: u64| (t - m.born) as f64 * DT;
            m.state = match std::mem::replace(&mut m.state, State::Moved) {
                State::Active(mut house) if near > lod::PARK => {
                    if house.try_leave() {
                        State::Abstract(house)
                    } else {
                        State::Active(house)
                    }
                }
                State::Abstract(mut house) if near <= lod::PARK => {
                    let until = tick + CATCH;
                    let target = at_tick(until);
                    let job = std::thread::spawn(move || {
                        house.advance(target);
                        house
                    });
                    State::Catching { job, until }
                }
                State::Catching { job, until } if tick >= until => {
                    let mut house = job.join().expect("a machine's catching up");
                    if near <= lod::ACTIVE {
                        house.come_back();
                        State::Active(house)
                    } else {
                        State::Abstract(house)
                    }
                }
                other => other,
            };
        }
    }

    /// Takes in every catching up (before the world is saved or replaced).
    pub fn join_machines(&mut self) {
        for m in &mut self.machines {
            m.join();
        }
    }

    /// What a panel shows of each machine.
    pub fn machine_views(&self) -> Vec<MachineView> {
        (self.machines.iter())
            .map(|m| {
                let (mode, amounts, time) = match &m.state {
                    State::Active(h) => (0, h.amounts(), h.time()),
                    State::Abstract(h) => (1, h.amounts(), h.time()),
                    _ => (2, vec![f64::NAN; 3], f64::NAN),
                };
                MachineView {
                    id: m.id,
                    at: m.at,
                    mode,
                    water: amounts[0],
                    hydrogen: amounts[2],
                    time,
                }
            })
            .collect()
    }
}
