//! renet's channels as the game uses them, the server's rates, and the byte budget each
//! client is held to.
//!
//! The server steps at [`TICK_RATE`] and sends every [`SEND_EVERY`]-th tick. What it sends a
//! client is paced ([`Pacer`]): a bucket filling at [`BYTES_PER_SECOND`] (256 kbit/s, on the
//! wire, headers counted) pays for every message as it is handed to renet; reliable messages
//! wait in the server's outbox until the bucket can pay for them, and the snapshot takes what
//! is left (never less than [`SNAPSHOT_FLOOR`], so the world keeps moving while a joining
//! client is sent a large structure). renet itself is let send whatever it is given.

use renet::{ChannelConfig, ConnectionConfig, SendType};
use std::time::Duration;

/// Reliable, in order: the handshake, commands, spawns, edits, deformation.
pub const RELIABLE: u8 = 0;
/// Unreliable: snapshots, inputs.
pub const UNRELIABLE: u8 = 1;

/// Server ticks a second.
pub const TICK_RATE: u32 = 60;
/// The server sends every this many ticks (30 times a second).
pub const SEND_EVERY: u32 = 2;
/// The budget a client is held to, on the wire (bytes a second: 256 kbit/s).
pub const BYTES_PER_SECOND: f64 = 32_000.0;
/// What a packet costs beyond its payload: IPv4 (20) and UDP (8) headers, netcode's prefix,
/// sequence and tag (up to 25), renet's packet header (up to 11).
pub const PACKET_OVERHEAD: usize = 64;
/// What a message costs within a packet beyond its bytes (its id and length in renet).
pub const MESSAGE_OVERHEAD: usize = 6;
/// A snapshot's bytes at least, however full the bucket.
pub const SNAPSHOT_FLOOR: usize = 192;
/// A snapshot's bytes at most (one packet: renet slices beyond 1200).
pub const SNAPSHOT_MOST: usize = 1100;

/// The channels and limits for both ends.
pub fn config() -> ConnectionConfig {
    let channels = || {
        vec![
            // Snapshots first: renet fills a packet in this order.
            ChannelConfig {
                channel_id: UNRELIABLE,
                max_memory_usage_bytes: 1 << 20,
                send_type: SendType::Unreliable,
            },
            ChannelConfig {
                channel_id: RELIABLE,
                max_memory_usage_bytes: 32 << 20,
                send_type: SendType::ReliableOrdered {
                    resend_time: Duration::from_millis(200),
                },
            },
        ]
    };
    ConnectionConfig {
        // The pacer limits; renet sends what it is given.
        available_bytes_per_tick: 1 << 20,
        server_channels_config: channels(),
        client_channels_config: channels(),
    }
}

/// The bytes a client may still be sent: a bucket filling at [`BYTES_PER_SECOND`], holding
/// at most a fifth of a second's worth, in debt after a large message until it has refilled.
///
/// A send round (one every [`SEND_EVERY`] ticks) pays for its reliable messages as they are
/// handed over ([`Self::send`]), for the packets they fill when it ends ([`Self::flush`]),
/// and for the snapshot, a packet of its own ([`Self::snapshot`]).
#[derive(Clone, Copy, Debug)]
pub struct Pacer {
    tokens: f64,
    round: usize,
}

impl Default for Pacer {
    fn default() -> Self {
        Self {
            tokens: Self::BURST,
            round: 0,
        }
    }
}

/// The packets `bytes` of messages fill.
fn packets(bytes: usize) -> usize {
    bytes.div_ceil(PACKET_PAYLOAD)
}

/// The bytes of messages renet puts in one packet.
const PACKET_PAYLOAD: usize = 1200;

impl Pacer {
    /// The most the bucket holds (bytes).
    pub const BURST: f64 = BYTES_PER_SECOND / 5.0;

    /// `seconds` passed.
    pub fn refill(&mut self, seconds: f64) {
        self.tokens = (self.tokens + BYTES_PER_SECOND * seconds).min(Self::BURST);
    }

    /// What this round costs so far and would with `more` bytes of messages and a snapshot
    /// of `snapshot` bytes.
    fn cost(&self, more: usize, snapshot: usize) -> f64 {
        let reliable = self.round + more;
        (packets(reliable) * PACKET_OVERHEAD + more + snapshot + MESSAGE_OVERHEAD + PACKET_OVERHEAD)
            as f64
    }

    /// Whether a reliable message of `len` bytes may go this round: if the bucket pays for
    /// it and still for a snapshot's floor (one larger than the bucket goes into debt).
    pub fn may_send(&self, len: usize) -> bool {
        let len = len.min(SNAPSHOT_MOST) + MESSAGE_OVERHEAD;
        self.tokens >= self.cost(len, SNAPSHOT_FLOOR)
    }

    /// Pays for a reliable message of `len` bytes.
    pub fn send(&mut self, len: usize) {
        self.tokens -= (len + MESSAGE_OVERHEAD) as f64;
        self.round += len + MESSAGE_OVERHEAD;
    }

    /// The bytes a snapshot may take this round.
    pub fn snapshot_budget(&self) -> usize {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped
        let left = (self.tokens - self.cost(0, 0)).max(0.0) as usize;
        left.clamp(SNAPSHOT_FLOOR, SNAPSHOT_MOST)
    }

    /// Pays for a snapshot of `len` bytes.
    pub fn snapshot(&mut self, len: usize) {
        self.tokens -= (len + MESSAGE_OVERHEAD + PACKET_OVERHEAD) as f64;
    }

    /// Ends the round: pays for the packets its reliable messages filled.
    pub fn flush(&mut self) {
        self.tokens -= (packets(self.round) * PACKET_OVERHEAD) as f64;
        self.round = 0;
    }

    /// The bytes left (negative: in debt).
    pub fn tokens(&self) -> f64 {
        self.tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pacer_spends_no_more_than_the_budget_and_lets_everything_through() {
        let mut pacer = Pacer::default();
        let mut spent = 0usize;
        let dt = f64::from(SEND_EVERY) / f64::from(TICK_RATE);
        // Ten seconds of a client joining: a 100 kB structure, then many small messages,
        // and a snapshot each round.
        let mut outbox: Vec<usize> = std::iter::once(100_000)
            .chain(std::iter::repeat_n(40, 2000))
            .collect();
        outbox.reverse();
        let mut full = 0;
        for _ in 0..300 {
            pacer.refill(dt);
            let mut round = 0;
            while outbox.last().is_some_and(|&len| pacer.may_send(len)) {
                let len = outbox.pop().unwrap();
                pacer.send(len);
                round += len + MESSAGE_OVERHEAD;
            }
            let budget = pacer.snapshot_budget();
            pacer.snapshot(budget);
            pacer.flush();
            // On the wire: the messages in their packets, and the snapshot in its own.
            spent += round + round.div_ceil(1200) * PACKET_OVERHEAD;
            spent += budget + MESSAGE_OVERHEAD + PACKET_OVERHEAD;
            full += usize::from(budget > SNAPSHOT_FLOOR);
        }
        #[allow(clippy::cast_precision_loss)]
        let rate = spent as f64 / 10.0;
        // Only the bucket's first fill comes on top.
        assert!(rate <= BYTES_PER_SECOND + Pacer::BURST / 10.0, "{rate} B/s");
        assert!(outbox.is_empty(), "{} left", outbox.len());
        assert!(full > 100, "{full} full snapshots");
    }
}
