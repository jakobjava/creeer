//! The scatter's rules as `geology/loose.ron` has them, and resolved against a planet's rocks.

use super::{LOOSE, Scatter};
use creeer_terrain::Terrain;
use creeer_terrain::geology::rocks::RockTable;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StoneDef {
    ground: String,
    per_m2: f64,
    largest: f64,
    #[serde(default)]
    from_below: bool,
    #[serde(default)]
    nodules: Vec<(String, f64)>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BranchDef {
    ground: String,
    per_m2: f64,
    wood: String,
    length: (f64, f64),
    radius: (f64, f64),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LooseDef {
    cell: f64,
    smallest: f64,
    stones: Vec<StoneDef>,
    branches: Vec<BranchDef>,
}

#[derive(Clone, Debug)]
pub(super) struct StoneRule {
    pub(super) per_m2: f64,
    pub(super) largest: f64,
    pub(super) from_below: bool,
    pub(super) nodules: Vec<(u16, f64)>,
}

#[derive(Clone, Debug)]
pub(super) struct BranchRule {
    pub(super) per_m2: f64,
    pub(super) wood: usize,
    pub(super) length: (f64, f64),
    pub(super) radius: (f64, f64),
}

impl Scatter {
    pub(super) fn new(
        def: &LooseDef,
        rocks: &RockTable,
        terrain: &Terrain,
    ) -> Result<Self, String> {
        let rock = |name: &str| {
            rocks
                .rock(name)
                .ok_or_else(|| format!("{LOOSE}: no rock {name}"))
        };
        let positive = |x: f64| x.is_finite() && x > 0.0;
        if !positive(def.cell) || !positive(def.smallest) {
            return Err(format!("{LOOSE}: a cell or size that is not positive"));
        }
        let mut stones = vec![None; rocks.rocks.len()];
        for s in &def.stones {
            if !positive(s.per_m2) || s.largest <= def.smallest {
                return Err(format!(
                    "{LOOSE}: {}: a density or size out of range",
                    s.ground
                ));
            }
            let nodules = (s.nodules.iter())
                .map(|(n, f)| Ok((rock(n)?, *f)))
                .collect::<Result<_, String>>()?;
            stones[usize::from(rock(&s.ground)?)] = Some(StoneRule {
                per_m2: s.per_m2,
                largest: s.largest,
                from_below: s.from_below,
                nodules,
            });
        }
        let (mut branches, mut woods) = (vec![None; rocks.rocks.len()], Vec::new());
        for b in &def.branches {
            let ok = positive(b.per_m2)
                && positive(b.length.0)
                && b.length.1 >= b.length.0
                && positive(b.radius.0)
                && b.radius.1 >= b.radius.0;
            if !ok {
                return Err(format!("{LOOSE}: {}'s branches out of range", b.ground));
            }
            woods.push(b.wood.clone());
            branches[usize::from(rock(&b.ground)?)] = Some(BranchRule {
                per_m2: b.per_m2,
                wood: woods.len() - 1,
                length: b.length,
                radius: b.radius,
            });
        }
        let planet = terrain.planet();
        Ok(Self {
            step: def.cell / planet.radius,
            smallest: def.smallest,
            stones,
            branches,
            woods,
            seed: planet.seed,
        })
    }
}
