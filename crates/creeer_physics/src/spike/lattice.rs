//! Large single-island test structures for the F2 speed comparison.
//!
//! The 5,000-beam budget of ADR-0002 must hold for **one** structure, where island
//! parallelism cannot help. Two shapes stress a direct solver differently:
//! - a long bridge (a chain of bays): narrow, the easiest case for sparse factorisation;
//! - a square space-frame roof (a 2D mesh of cubes): the factor fills in much more.

use super::*;

impl Truss {
    /// A reference-style box bridge with `bays` bays (about 13 beams per bay).
    pub fn long_bridge(bays: usize) -> Self {
        Self::box_bridge(
            DVec3::ZERO,
            BridgeSpec {
                bays,
                ..BridgeSpec::REFERENCE
            },
        )
    }

    /// A double-layer space-frame roof of `cells × cells` cubes of 2 m, every cube face
    /// braced with one diagonal (a triangulated, hence rigid, lattice), steel members of
    /// 10 cm², pinned at the four bottom corners.
    pub fn space_frame(cells: usize) -> Self {
        Self::lattice([cells, 1, cells], true)
    }

    /// A free-floating block of `cells` 2 m cubes (no supports), for rotation tests.
    pub fn free_block(cells: [usize; 3]) -> Self {
        Self::lattice(cells, false)
    }

    fn lattice(cells: [usize; 3], pinned_corners: bool) -> Self {
        const SPACING: f64 = 2.0;
        const AREA: f64 = 1.0e-3;
        let [nx, ny, nz] = cells;
        let id = |i: usize, j: usize, k: usize| (k * (ny + 1) + j) * (nx + 1) + i;
        let mut position = Vec::new();
        for k in 0..=nz {
            for j in 0..=ny {
                for i in 0..=nx {
                    #[allow(clippy::cast_precision_loss)]
                    position.push(DVec3::new(i as f64, j as f64, k as f64) * SPACING);
                }
            }
        }
        let mut pairs = Vec::new();
        for k in 0..=nz {
            for j in 0..=ny {
                for i in 0..=nx {
                    let here = id(i, j, k);
                    if i < nx {
                        pairs.push((here, id(i + 1, j, k)));
                    }
                    if j < ny {
                        pairs.push((here, id(i, j + 1, k)));
                    }
                    if k < nz {
                        pairs.push((here, id(i, j, k + 1)));
                    }
                    if i < nx && j < ny {
                        pairs.push((here, id(i + 1, j + 1, k)));
                    }
                    if j < ny && k < nz {
                        pairs.push((here, id(i, j + 1, k + 1)));
                    }
                    if i < nx && k < nz {
                        pairs.push((here, id(i + 1, j, k + 1)));
                    }
                }
            }
        }
        let corners = [id(0, 0, 0), id(nx, 0, 0), id(0, 0, nz), id(nx, 0, nz)];
        let pinned: &[usize] = if pinned_corners { &corners } else { &[] };
        let (c, d) = (nx / 2, nz / 2);
        let probe = [
            id(c, 0, d),
            id(c + 1, 0, d),
            id(c, 0, d + 1),
            id(c + 1, 0, d + 1),
        ];
        Self::from_members(position, &pairs, AREA, STEEL, pinned, probe)
    }

    /// Builds a truss from node positions and member node pairs; `pinned` nodes are fixed
    /// on all axes and `probe` nodes are where [`Truss::midspan_sag`] measures.
    fn from_members(
        position: Vec<DVec3>,
        pairs: &[(usize, usize)],
        area: f64,
        material: Material,
        pinned: &[usize],
        probe: [usize; 4],
    ) -> Self {
        let ea = material.youngs_modulus * area;
        let mut mass = vec![0.0; position.len()];
        let beams = pairs
            .iter()
            .map(|&(a, b)| {
                let rest_length = (position[a] - position[b]).length();
                let member_mass = material.density * area * rest_length;
                mass[a] += member_mass / 2.0;
                mass[b] += member_mass / 2.0;
                Beam {
                    a,
                    b,
                    rest_length,
                    compliance: rest_length / ea,
                }
            })
            .collect();
        let mut inv_mass: Vec<DVec3> = mass.iter().map(|m| DVec3::splat(1.0 / m)).collect();
        for &node in pinned {
            inv_mass[node] = DVec3::ZERO;
        }
        Self {
            previous: position.clone(),
            velocity: vec![DVec3::ZERO; position.len()],
            rest: position.clone(),
            position,
            inv_mass,
            mass,
            beams,
            midspan_nodes: probe,
            damping: 0.0,
        }
    }
}
