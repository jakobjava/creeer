//! What the players reach for, judged as they saw it (G1.4d, `world/reach.rs`): a client counts
//! the server's ticks as its snapshots are numbered, the world its own (which go back when a
//! save is loaded).

use super::Server;

impl Server {
    /// The world's tick at server tick `net` (as snapshots are numbered; fractional).
    pub fn world_tick(&self, net: f64) -> f64 {
        #[allow(clippy::cast_precision_loss)] // ticks within 2⁵³
        let world = self.world.tick as f64;
        net - f64::from(self.net_tick) + world
    }
}
