//! Courses to walk over (G2.11, stream A4: `play/walking.rs`): parts nailed together as a
//! builder nails them, without the builder (a deck to walk across is the course, not the
//! building of it).

use super::ServerWorld;
use creeer_net::entity::EntityId;

impl ServerWorld {
    /// Nails part `x` to part `y` where they touch (within 5 mm), with one 100 mm nail: one
    /// structure under `x`'s id.
    ///
    /// # Errors
    /// If they do not touch, or the joint would not hold.
    pub(crate) fn nail_for_course(&mut self, x: EntityId, y: EntityId) -> Result<(), String> {
        let (i, j) = (
            self.index(x).ok_or("no such part")?,
            self.index(y).ok_or("no such part")?,
        );
        let point = (self.world.touching(i, 0.005).into_iter())
            .find(|t| t.other.is_some_and(|(k, _)| k == j))
            .map(|t| t.point)
            .ok_or("they do not touch")?;
        let nail = (self.building.fasteners.iter())
            .find(|f| f.name == "nail_100")
            .cloned()
            .ok_or("no nails in the catalogue")?;
        self.fasten(x, y, point, &nail)
    }
}
