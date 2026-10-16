//! Ready-made structures for tests, benchmarks and the sandbox.

use super::{Material, NodeId, Section, Structure, StructureBuilder, Support};
use creeer_math::glam::DVec3;

/// A horizontal cantilever along +x from the origin, clamped at x = 0, made of `elements`
/// equal beams. The tip is the last node.
pub fn cantilever(length: f64, elements: usize, section: Section, material: Material) -> Structure {
    let mut b = StructureBuilder::new();
    #[allow(clippy::cast_precision_loss)]
    let nodes: Vec<NodeId> = (0..=elements)
        .map(|i| b.node(DVec3::X * (length * i as f64 / elements as f64)))
        .collect();
    b.support(nodes[0], Support::Clamped);
    for pair in nodes.windows(2) {
        b.beam(pair[0], pair[1], section, material);
    }
    b.build()
}

/// How a frame stands on the ground.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base {
    /// Not supported (it can fall or fly).
    Free,
    /// Clamped at its four bottom corners.
    Corners,
    /// Clamped at every `n`-th bottom node along both horizontal axes (piers, columns).
    Every(usize),
}

/// A rigid-jointed 3D frame: a lattice of `cells` boxes with edges `spacing` long, every
/// edge one beam (welded joints carry the moments, so no diagonals are needed).
pub fn space_frame(
    cells: [usize; 3],
    spacing: f64,
    section: Section,
    material: Material,
    base: Base,
) -> Structure {
    let [nx, ny, nz] = cells;
    let id = |i: usize, j: usize, k: usize| (k * (ny + 1) + j) * (nx + 1) + i;
    let mut b = StructureBuilder::new();
    for k in 0..=nz {
        for j in 0..=ny {
            for i in 0..=nx {
                #[allow(clippy::cast_precision_loss)]
                b.node(DVec3::new(i as f64, j as f64, k as f64) * spacing);
            }
        }
    }
    for k in 0..=nz {
        for j in 0..=ny {
            for i in 0..=nx {
                let here = id(i, j, k);
                if i < nx {
                    b.beam(here, id(i + 1, j, k), section, material);
                }
                if j < ny {
                    b.beam(here, id(i, j + 1, k), section, material);
                }
                if k < nz {
                    b.beam(here, id(i, j, k + 1), section, material);
                }
            }
        }
    }
    let every = match base {
        Base::Free => return b.build(),
        Base::Corners => usize::MAX,
        Base::Every(n) => n.max(1),
    };
    for k in 0..=nz {
        for i in 0..=nx {
            let edge_x = i == 0 || i == nx || i % every == 0;
            let edge_z = k == 0 || k == nz || k % every == 0;
            let corner = (i == 0 || i == nx) && (k == 0 || k == nz);
            if corner || (every != usize::MAX && edge_x && edge_z) {
                b.support(id(i, 0, k), Support::Clamped);
            }
        }
    }
    b.build()
}
