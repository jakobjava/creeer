//! The authoritative server (F5, ADR-0008): the world on a planet ([`world`]), the sessions
//! of the players connected to it ([`session`]), what they are told of it ([`replicate`]),
//! and the loop that ties them together ([`server`]). A singleplayer game runs one in
//! process ([`host`]); `creeer_server` (the binary) runs one on its own, over UDP.

pub mod client;
pub mod host;
pub mod predicted;
mod profiling;
pub mod replicate;
pub mod server;
pub mod session;
mod things;
pub mod world;

pub use client::Connection;
pub use server::{Event, Server};
pub use world::ServerWorld;
