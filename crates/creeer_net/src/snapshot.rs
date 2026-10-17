//! Snapshots: the state of everything in a client's view, written against what the client
//! is known to have (delta compression), within a budget of bytes.
//!
//! The server keeps, per client, the snapshots it sent (what the client has after each:
//! [`Sender`]). A new one is written against the newest the client acknowledged: a body
//! whose quantised state has not changed since costs nothing, one that moved costs the
//! difference (a few bytes for a body that moved little), one new to the client its whole
//! state, one that left view a removal. Bodies are taken in order of priority (grown by
//! how long they waited, `importance` a tick) until the budget is spent; the others keep the
//! state the client has, which the server records as such, and wait with a higher priority.
//! The client keeps the snapshots it received ([`Receiver`]) and rebuilds each from its
//! baseline.

use crate::entity::EntityId;
use crate::quant::{BodyState, Quantised, Rotation};
use creeer_math::codec::{DecodeError, Reader, Writer};
use std::collections::{BTreeMap, HashMap, VecDeque};

/// Snapshots kept on either side (at 30 a second, a second's worth: a client acknowledging
/// none in that long gets whole states).
pub const HISTORY: usize = 32;

/// Every state in a snapshot, by entity.
pub type States = BTreeMap<EntityId, BodyState>;

/// A snapshot as the client has it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    /// The server's tick it was taken at.
    pub tick: u32,
    /// The newest of the client's inputs it includes.
    pub last_input: u32,
    /// The states of everything in view.
    pub states: States,
}

// Entry flags: which parts follow.
const POSITION: u8 = 1;
const ROTATION: u8 = 2;
const VELOCITY: u8 = 4;
const SPIN: u8 = 8;
const ASLEEP: u8 = 16;
const REMOVED: u8 = 32;

/// The server's side for one client.
#[derive(Debug, Default)]
pub struct Sender {
    sent: VecDeque<Snapshot>,
    acked: u32,
    priority: HashMap<EntityId, f64>,
}

impl Sender {
    /// The client has the snapshot of `tick` (and so every older one it acknowledged).
    pub fn ack(&mut self, tick: u32) {
        self.acked = self.acked.max(tick);
    }

    /// The snapshot of `tick` for the entities in view, `(id, state, importance)`, the
    /// client's newest input `last_input` included, in at most `budget` bytes (removals and
    /// the header always fit).
    pub fn write(
        &mut self,
        tick: u32,
        last_input: u32,
        view: &[(EntityId, BodyState, f64)],
        budget: usize,
    ) -> Vec<u8> {
        let base = self.sent.iter().find(|s| s.tick == self.acked).cloned();
        let empty = States::new();
        let base_states = base.as_ref().map_or(&empty, |b| &b.states);
        let mut has = base_states.clone();
        let mut entries: Vec<(EntityId, Option<BodyState>)> = Vec::new();
        // What left view: removed.
        let seen: std::collections::BTreeSet<EntityId> = view.iter().map(|v| v.0).collect();
        for id in base_states.keys().filter(|id| !seen.contains(id)) {
            entries.push((*id, None));
            has.remove(id);
        }
        // What changed, most waited for first.
        let mut changed: Vec<&(EntityId, BodyState, f64)> = view
            .iter()
            .filter(|(id, state, _)| base_states.get(id) != Some(state))
            .collect();
        for (id, _, importance) in &changed {
            *self.priority.entry(*id).or_default() += importance.max(1e-9);
        }
        changed.sort_by(|a, b| {
            let (pa, pb) = (self.priority[&a.0], self.priority[&b.0]);
            pb.total_cmp(&pa).then(a.0.cmp(&b.0))
        });
        let mut size = 16 + entries.len() * 6;
        for &&(id, state, _) in &changed {
            let cost = entry_size(base_states.get(&id), &state);
            if size + cost > budget {
                continue;
            }
            size += cost;
            entries.push((id, Some(state)));
            has.insert(id, state);
            self.priority.remove(&id);
        }
        self.priority.retain(|id, _| seen.contains(id));
        entries.sort_by_key(|e| e.0);
        let bytes = encode(
            tick,
            base.as_ref().map(|b| b.tick),
            last_input,
            base_states,
            &entries,
        );
        self.sent.push_back(Snapshot {
            tick,
            last_input,
            states: has,
        });
        while self.sent.len() > HISTORY {
            self.sent.pop_front();
        }
        bytes
    }
}

/// About how many bytes an entry takes (for the budget).
fn entry_size(base: Option<&BodyState>, state: &BodyState) -> usize {
    let mut w = Writer::new();
    write_entry(&mut w, base, state);
    w.bytes().len() + 2
}

fn write_entry(w: &mut Writer, base: Option<&BodyState>, state: &BodyState) {
    let none = BodyState::default();
    let b = base.unwrap_or(&none);
    let mut flags = 0;
    // Whole states for what the client does not have; else only the parts that changed.
    if base.is_none() || state.position != b.position {
        flags |= POSITION;
    }
    if base.is_none() || state.rotation != b.rotation {
        flags |= ROTATION;
    }
    if base.is_none() || state.velocity != b.velocity {
        flags |= VELOCITY;
    }
    if base.is_none() || state.spin != b.spin {
        flags |= SPIN;
    }
    if state.asleep {
        flags |= ASLEEP;
    }
    w.u8(flags);
    if flags & POSITION != 0 {
        state.position.write(w, b.position);
    }
    if flags & ROTATION != 0 {
        state.rotation.write(w);
    }
    if flags & VELOCITY != 0 {
        state.velocity.write(w, b.velocity);
    }
    if flags & SPIN != 0 {
        state.spin.write(w, b.spin);
    }
}

fn encode(
    tick: u32,
    base: Option<u32>,
    last_input: u32,
    base_states: &States,
    entries: &[(EntityId, Option<BodyState>)],
) -> Vec<u8> {
    let mut w = Writer::new();
    w.varint(u64::from(tick));
    // The baseline as how many ticks back (0: none).
    w.varint(base.map_or(0, |b| u64::from(tick - b)));
    w.varint(u64::from(last_input));
    w.varint(entries.len() as u64);
    let mut previous = 0u32;
    for (id, state) in entries {
        w.varint(u64::from(id.0 - previous));
        previous = id.0;
        match state {
            None => w.u8(REMOVED),
            Some(state) => write_entry(&mut w, base_states.get(id), state),
        }
    }
    w.finish()
}

/// The client's side.
#[derive(Debug, Default)]
pub struct Receiver {
    received: VecDeque<Snapshot>,
}

impl Receiver {
    /// Reads a snapshot and keeps it: the newest received if it is newer than all before.
    /// One older than the newest is kept too (for interpolation), unless its baseline is
    /// gone.
    ///
    /// # Errors
    /// If the bytes are not a snapshot, or its baseline is not here (it then is dropped:
    /// the server writes the next against what the client acknowledged).
    pub fn read(&mut self, bytes: &[u8]) -> Result<&Snapshot, DecodeError> {
        let mut r = Reader::new(bytes);
        let tick = small(&mut r)?;
        let back = small(&mut r)?;
        let last_input = small(&mut r)?;
        let base = if back == 0 {
            None
        } else {
            let at = tick
                .checked_sub(back)
                .ok_or_else(|| r.error("baseline before tick 0"))?;
            Some(
                self.received
                    .iter()
                    .find(|s| s.tick == at)
                    .ok_or_else(|| r.error("baseline not here"))?
                    .states
                    .clone(),
            )
        };
        let mut states = base.unwrap_or_default();
        let count = r.count(2)?;
        let mut id = 0u32;
        for _ in 0..count {
            id = id
                .checked_add(small(&mut r)?)
                .ok_or_else(|| r.error("entity id"))?;
            let id = EntityId(id);
            let flags = r.u8()?;
            if flags & REMOVED != 0 {
                states.remove(&id);
                continue;
            }
            let b = states.get(&id).copied().unwrap_or_default();
            let state = BodyState {
                position: part(&mut r, flags, POSITION, b.position, |r| {
                    Quantised::read(r, b.position)
                })?,
                rotation: part(&mut r, flags, ROTATION, b.rotation, Rotation::read)?,
                velocity: part(&mut r, flags, VELOCITY, b.velocity, |r| {
                    Quantised::read(r, b.velocity)
                })?,
                spin: part(&mut r, flags, SPIN, b.spin, |r| Quantised::read(r, b.spin))?,
                asleep: flags & ASLEEP != 0,
            };
            states.insert(id, state);
        }
        r.end()?;
        let snapshot = Snapshot {
            tick,
            last_input,
            states,
        };
        let at = self.received.partition_point(|s| s.tick < tick);
        if self.received.get(at).is_some_and(|s| s.tick == tick) {
            self.received[at] = snapshot;
        } else {
            self.received.insert(at, snapshot);
        }
        while self.received.len() > HISTORY {
            self.received.pop_front();
        }
        self.received
            .iter()
            .find(|s| s.tick == tick)
            .ok_or_else(|| r.error("older than every snapshot kept"))
    }

    /// The newest snapshot received.
    pub fn newest(&self) -> Option<&Snapshot> {
        self.received.back()
    }

    /// The snapshots kept, oldest first.
    pub fn snapshots(&self) -> impl Iterator<Item = &Snapshot> {
        self.received.iter()
    }
}

fn small(r: &mut Reader<'_>) -> Result<u32, DecodeError> {
    u32::try_from(r.varint()?).map_err(|_| r.error("beyond 32 bits"))
}

fn part<T>(
    r: &mut Reader<'_>,
    flags: u8,
    bit: u8,
    unchanged: T,
    read: impl FnOnce(&mut Reader<'_>) -> Result<T, DecodeError>,
) -> Result<T, DecodeError> {
    if flags & bit != 0 {
        read(r)
    } else {
        Ok(unchanged)
    }
}

#[cfg(test)]
mod tests;
