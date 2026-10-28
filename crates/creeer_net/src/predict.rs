//! Client-side prediction of the player's own avatar, and the server's queue of its inputs.
//!
//! The client applies each input to its own copy of the avatar at once (so it moves as the
//! key is pressed, not a round trip later), keeps it in an [`InputLog`] and sends the last
//! [`RESEND`] of them every tick (a lost packet loses none). The server applies one input a
//! tick from its [`InputQueue`] and says, in each snapshot, which was the newest applied.
//! When a snapshot comes, the client takes the avatar's state from it, drops the inputs it
//! includes and applies the rest again ([`reconcile`]): what the server did, plus what it has
//! not seen yet. Where the two disagree (the avatar was hit by something the client did not
//! know of), the drawn avatar slides to the corrected one ([`Smoothing`]) rather than jumps.
//!
//! Inputs are quantised as they are made ([`AvatarInput::quantised`]): the client predicts
//! with exactly what the server will be told.

use crate::message::AvatarInput;
use crate::quant::Quantised;
use creeer_math::glam::DVec3;
use std::collections::VecDeque;

/// The client sends its inputs every this many ticks (30 times a second).
pub const INPUT_EVERY: u32 = 2;
/// Inputs sent with each packet (the newest last): 130 ms of them.
pub const RESEND: usize = 8;
/// Inputs the client keeps unacknowledged at most (two seconds): past it, the oldest go.
pub const KEEP: usize = 120;
/// Inputs the server's jitter buffer waits for at least ([`InputQueue`]).
pub const BUFFER: usize = 2;
/// And at most (100 ms).
pub const MOST_BUFFER: usize = 6;
/// Ticks of a buffer never running low before it shrinks (ten seconds).
pub const SETTLE: u32 = 600;
/// The bodies a client predicts with its avatar lie within this of it (m; ADR-0025): what it
/// can walk into within a round trip, and a reach.
pub const NEARBY: f64 = 6.0;
/// And at most this many of them, the nearest (each costs the client its re-simulation).
pub const MOST_NEARBY: usize = 12;
/// Inputs the server holds for a client at most: a client running ahead of the server's
/// clock has the excess dropped (and its prediction corrected), so its inputs never wait
/// longer than this many ticks.
pub const QUEUED: usize = 10;

impl AvatarInput {
    /// The input as it arrives at the server: its walk to [`crate::message::WALK_SCALE`], its
    /// thrust to [`crate::message::THRUST_SCALE`], its hold as written (`inputs.rs`).
    #[must_use]
    pub fn quantised(self) -> Self {
        Self {
            walk: Quantised::new(self.walk, crate::message::WALK_SCALE)
                .value(crate::message::WALK_SCALE),
            thrust: Quantised::new(self.thrust, crate::message::THRUST_SCALE)
                .value(crate::message::THRUST_SCALE),
            hold: crate::message::as_read(self.hold),
            ..self
        }
    }
}

/// For this many inputs after the last that asked to walk, the avatar is stopping (a second).
const STOPPING: u32 = 60;

/// The client's inputs not yet known to be applied, oldest first.
#[derive(Debug, Default)]
pub struct InputLog {
    inputs: VecDeque<AvatarInput>,
    next: u32,
    /// The number of the last input that asked to walk.
    walked: Option<u32>,
    /// What the hands are steered to, for the inputs from now on.
    hold: Option<crate::message::Hold>,
}

impl InputLog {
    /// Steers the hands to `hold` in the inputs from now on (G2.10; none: as they hold it).
    pub fn set_hold(&mut self, hold: Option<crate::message::Hold>) {
        self.hold = hold;
    }

    /// A new input: walk at `walk` (m/s), jump or not. Numbered, quantised and kept.
    pub fn push(&mut self, walk: DVec3, jump: bool, thrust: DVec3) -> AvatarInput {
        self.next += 1;
        if walk != DVec3::ZERO {
            self.walked = Some(self.next);
        }
        let input = AvatarInput {
            seq: self.next,
            walk,
            jump,
            thrust,
            stopping: walk == DVec3::ZERO && self.walked.is_some_and(|w| self.next - w <= STOPPING),
            hold: self.hold,
        }
        .quantised();
        self.inputs.push_back(input);
        while self.inputs.len() > KEEP {
            self.inputs.pop_front();
        }
        input
    }

    /// The server applied every input up to `last`.
    pub fn applied(&mut self, last: u32) {
        while self.inputs.front().is_some_and(|i| i.seq <= last) {
            self.inputs.pop_front();
        }
    }

    /// The inputs after the newest applied, oldest first.
    pub fn pending(&self) -> impl Iterator<Item = &AvatarInput> {
        self.inputs.iter()
    }

    /// The inputs to send this tick: the newest [`RESEND`], oldest first.
    pub fn to_send(&self) -> Vec<AvatarInput> {
        self.inputs
            .iter()
            .skip(self.inputs.len().saturating_sub(RESEND))
            .copied()
            .collect()
    }
}

/// The server's side: a client's inputs as they come (repeated, out of order, some lost),
/// one a tick, from a jitter buffer that fits itself to the link.
///
/// It waits to have `target` inputs before it applies them (when it starts, and whenever it
/// ran out), so a late packet finds the next input already there. Each time it runs out
/// anyway the target grows by one (up to [`MOST_BUFFER`]); when for [`SETTLE`] ticks it never
/// held fewer than two spare, the link is better than it was ready for: one input is dropped
/// (the client's prediction corrected for it) and the target shrinks, back to [`BUFFER`] at
/// least. Every input of the target costs a tick (17 ms) of the avatar's latency.
#[derive(Debug)]
pub struct InputQueue {
    queue: VecDeque<AvatarInput>,
    last: u32,
    held: AvatarInput,
    running: bool,
    target: usize,
    /// Ticks since it last ran out or shrank, and the fewest spare inputs held in them.
    quiet: u32,
    least: usize,
}

impl Default for InputQueue {
    fn default() -> Self {
        Self {
            queue: VecDeque::new(),
            last: 0,
            held: AvatarInput::default(),
            running: false,
            target: BUFFER,
            quiet: 0,
            least: usize::MAX,
        }
    }
}

impl InputQueue {
    /// Inputs received (any order; those already had are ignored).
    pub fn receive(&mut self, inputs: &[AvatarInput]) {
        for input in inputs {
            let input = input.quantised();
            let newest = self.queue.back().map_or(self.last, |i| i.seq);
            if input.seq > newest {
                // Ones between that never came stay lost: the next stands in for them.
                self.queue.push_back(input);
            } else if input.seq > self.last {
                let at = self.queue.partition_point(|i| i.seq < input.seq);
                if self.queue.get(at).is_none_or(|i| i.seq != input.seq) {
                    self.queue.insert(at, input);
                }
            }
        }
        while self.queue.len() > QUEUED {
            self.queue.pop_front();
        }
    }

    /// The input for this tick: the oldest waiting, or (if none came in time, or the buffer
    /// is filling again) the last one again, still walking but not jumping.
    pub fn take(&mut self) -> AvatarInput {
        if self.running && self.queue.is_empty() {
            self.running = false;
            self.target = (self.target + 1).min(MOST_BUFFER);
            (self.quiet, self.least) = (0, usize::MAX);
        }
        self.running = self.running || self.queue.len() >= self.target;
        let next = if self.running {
            self.queue.pop_front()
        } else {
            None
        };
        if self.running {
            self.least = self.least.min(self.queue.len());
            self.quiet += 1;
            if self.quiet >= SETTLE {
                if self.least >= 2 {
                    self.queue.pop_front();
                    self.target = (self.target - 1).max(BUFFER);
                }
                (self.quiet, self.least) = (0, usize::MAX);
            }
        }
        match next {
            Some(input) => {
                self.last = input.seq;
                self.held = input;
                input
            }
            None => AvatarInput {
                jump: false,
                ..self.held
            },
        }
    }

    /// The newest input applied (0: none yet), for the snapshot.
    pub fn last(&self) -> u32 {
        self.last
    }

    /// The inputs it waits to have now.
    pub fn target(&self) -> usize {
        self.target
    }
}

/// The predicted avatar: the server's `state` after the inputs up to `last_input`, then the
/// rest of `log` applied by `step` (one tick each). The log forgets what the server has.
pub fn reconcile<S>(
    state: S,
    last_input: u32,
    log: &mut InputLog,
    mut step: impl FnMut(&mut S, &AvatarInput),
) -> S {
    log.applied(last_input);
    let mut state = state;
    for input in log.pending() {
        step(&mut state, input);
    }
    state
}

/// The drawn avatar's offset from the predicted one, after a correction: it decays to
/// nothing in about [`Smoothing::TIME`], unless the correction was too large to slide.
#[derive(Clone, Copy, Debug, Default)]
pub struct Smoothing {
    offset: DVec3,
}

impl Smoothing {
    /// The time constant of the slide (s).
    pub const TIME: f64 = 0.1;
    /// A correction past this (m) is a jump, drawn at once (a teleport, a respawn).
    pub const SNAP: f64 = 2.0;

    /// The prediction was corrected from `before` to `after`.
    pub fn corrected(&mut self, before: DVec3, after: DVec3) {
        let offset = self.offset + before - after;
        self.offset = if offset.length() > Self::SNAP {
            DVec3::ZERO
        } else {
            offset
        };
    }

    /// `dt` seconds passed: the offset to draw at.
    pub fn advance(&mut self, dt: f64) -> DVec3 {
        self.offset *= creeer_math::det::exp(-dt / Self::TIME);
        self.offset
    }
}

#[cfg(test)]
mod tests;
