//! Network protocol, replication and interest management (F5, ADR-0008).
//!
//! The server owns the world; clients are told what is near them ([`interest`]) as snapshots
//! of quantised states ([`quant`], [`snapshot`]) and draw it a little in the past between
//! them ([`interp`]); the player's own avatar is predicted and reconciled ([`predict`]).
//! Everything else that must arrive goes reliably ([`message`]). The messages travel over
//! renet's channels ([`channel`]): in process for a singleplayer game, over UDP
//! ([`udp`]) for a dedicated server, or through a simulated link with latency and loss
//! ([`link`]) in tests.

pub mod channel;
pub mod entity;
pub mod interest;
pub mod interp;
pub mod link;
pub mod message;
pub mod predict;
pub mod quant;
pub mod snapshot;
pub mod udp;

pub use renet;

/// The protocol's version: a client and server speak only if theirs are equal.
pub const PROTOCOL_VERSION: u32 = 18;
