//! Interest management: which entities a client is told about, by where they are.
//!
//! Entities are binned once a tick into cubes of [`CELL`] metres; a client's view is what
//! lies within its radius of its centre (its avatar, or its camera), found from the cells
//! that sphere touches. Each entity in view gets an importance for the snapshot budget: near
//! and fast first (`importance`).

use crate::entity::EntityId;
use creeer_math::glam::DVec3;
use std::collections::HashMap;

/// The grid's cell (m).
pub const CELL: f64 = 32.0;

/// Entities binned by where they are.
#[derive(Debug, Default)]
pub struct Grid {
    cells: HashMap<[i64; 3], Vec<(EntityId, DVec3)>>,
}

fn cell(p: DVec3) -> [i64; 3] {
    #[allow(clippy::cast_possible_truncation)] // positions within a planet's frame
    p.to_array().map(|x| (x / CELL).floor() as i64)
}

impl Grid {
    /// The entities `at` their positions.
    pub fn new(at: impl IntoIterator<Item = (EntityId, DVec3)>) -> Self {
        let mut cells: HashMap<[i64; 3], Vec<(EntityId, DVec3)>> = HashMap::new();
        for (id, p) in at {
            cells.entry(cell(p)).or_default().push((id, p));
        }
        Self { cells }
    }

    /// The entities within `radius` of `centre`, by id.
    pub fn around(&self, centre: DVec3, radius: f64) -> Vec<(EntityId, DVec3)> {
        let (lo, hi) = (
            cell(centre - DVec3::splat(radius)),
            cell(centre + DVec3::splat(radius)),
        );
        let mut out = Vec::new();
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                for z in lo[2]..=hi[2] {
                    let Some(found) = self.cells.get(&[x, y, z]) else {
                        continue;
                    };
                    out.extend(
                        found
                            .iter()
                            .filter(|(_, p)| (*p - centre).length_squared() <= radius * radius),
                    );
                }
            }
        }
        out.sort_by_key(|e: &(EntityId, DVec3)| e.0);
        out
    }
}

/// How much an entity `distance` metres away moving at `speed` matters to a client a tick:
/// near ones and fast ones most (an entity at rest still gets its turn as it waits).
pub fn importance(distance: f64, speed: f64) -> f64 {
    (1.0 + speed) / (1.0 + distance / 10.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_view_is_what_lies_within_the_radius() {
        let at: Vec<(EntityId, DVec3)> = (0..2000u32)
            .map(|i| {
                let f = f64::from(i);
                (
                    EntityId(i),
                    DVec3::new(
                        f * 1.7 % 400.0 - 200.0,
                        f * 0.37 % 50.0,
                        f * 2.3 % 300.0 - 150.0,
                    ),
                )
            })
            .collect();
        let grid = Grid::new(at.iter().copied());
        for (centre, radius) in [
            (DVec3::ZERO, 60.0),
            (DVec3::new(130.0, 10.0, -70.0), 25.0),
            (DVec3::splat(1e4), 5.0),
        ] {
            let mut want: Vec<EntityId> = at
                .iter()
                .filter(|(_, p)| (*p - centre).length() <= radius)
                .map(|e| e.0)
                .collect();
            want.sort();
            let got: Vec<EntityId> = grid.around(centre, radius).iter().map(|e| e.0).collect();
            assert_eq!(got, want);
        }
    }
}
