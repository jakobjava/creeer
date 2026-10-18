//! The server's profile (F7.6): its ticks cut into frames while anyone in the process wants
//! a profile (a hosting client's profiler window, or a client that asked over the network),
//! and a report a second to every client that asked.

use creeer_core::profile::{self, Clock, Want};
use creeer_net::channel::TICK_RATE;
use creeer_net::message::{ProfileReport, ServerMessage};
use creeer_net::renet::ClientId;
use std::collections::BTreeMap;

/// Who watches the server's profile, and the clock its ticks are cut with.
#[derive(Debug, Default)]
pub(crate) struct Profiling {
    /// Each watcher and the last frame it was sent (`profile::frames_since`).
    pub(crate) watchers: BTreeMap<ClientId, u64>,
    want: Option<Want>,
    clock: Option<Clock>,
}

impl Profiling {
    /// Client `client` asks for the profile (`on`) or not any more.
    pub fn watch(&mut self, client: ClientId, on: bool) {
        if on {
            self.watchers.entry(client).or_insert(0);
        } else {
            self.watchers.remove(&client);
        }
        if self.watchers.is_empty() {
            self.want = None;
        } else if self.want.is_none() {
            self.want = Some(profile::want());
        }
    }

    /// Ends tick `tick`'s frame (while profiling), and gives the reports due.
    pub fn end_tick(&mut self, tick: u64) -> Vec<(ClientId, ServerMessage)> {
        if !profile::enabled() {
            self.clock = None;
            return Vec::new();
        }
        self.clock = self
            .clock
            .take()
            .filter(Clock::holds)
            .or_else(|| profile::clock(2));
        if let Some(clock) = &mut self.clock {
            clock.end_tick(tick);
        }
        // (The frames kept are the process's, whoever cut them: another server's in the
        // same process, in tests.)
        if !tick.is_multiple_of(u64::from(TICK_RATE)) {
            return Vec::new();
        }
        self.watchers
            .iter_mut()
            .filter_map(|(client, seen)| {
                let report = ProfileReport::of(&profile::frames_since(seen))?;
                Some((*client, ServerMessage::Profile(report)))
            })
            .collect()
    }
}
