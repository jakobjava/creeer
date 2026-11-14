//! Parts' meshes (G2.11, ADR-0037): the unit primitives' levels of detail (`primitives.rs`) with
//! their faces naming palette tokens, so a pole, a sawn board or a log is drawn with the full
//! surface of `art-style.md` §6 (its wood's grain, edge wear, age and grime), the same mesh at
//! every size. A part's place, tone and taper are its instance's ([`crate::Material::part`]).

use crate::mesh::MeshData;
use crate::primitives::{box_levels, cylinder_levels, sphere_levels};
use crate::vertex::{Grain, MeshVertex};

/// What a round part's faces are: its side's token and grain, its ends' (the caps across its
/// Y axis), and a token drawn over its side in patches, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Faces {
    pub side: (u16, Grain),
    pub ends: (u16, Grain),
    pub marks: Option<u16>,
}

impl Faces {
    /// A part of one wood all over, worked round: a pole, a peg, a haft.
    pub fn round(token: u16) -> Self {
        Self {
            side: (token, Grain::Round),
            ends: (token, Grain::Round),
            marks: None,
        }
    }

    /// A log or a branch: bark of `bark` (with `marks`) on its side, `cut` wood on its ends.
    pub fn log(bark: u16, cut: u16, marks: Option<u16>) -> Self {
        Self {
            side: (bark, Grain::Bark),
            ends: (cut, Grain::Round),
            marks,
        }
    }
}

/// A vertex whose normal leans this far along Y (its cosine) is an end's (a cap's, or the
/// half of a rim nearer it).
const END: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// The unit cylinder's levels (along Y, 1 m across and long) as a round part with `faces`.
pub fn cylinder(faces: Faces) -> Vec<(MeshData, f32)> {
    retoken(cylinder_levels(), |v| {
        let (token, grain) = if v.normal[1].abs() >= END {
            faces.ends
        } else {
            faces.side
        };
        let marks = (grain == Grain::Bark).then_some(faces.marks).flatten();
        v.as_part(token, grain, marks)
    })
}

/// The unit box's levels as a sawn part of `token`, its grain along its longest side.
pub fn board(token: u16) -> Vec<(MeshData, f32)> {
    retoken(box_levels(), |v| v.as_part(token, Grain::Sawn, None))
}

/// The unit sphere's levels as a lump of `token` with no grain (foliage).
pub fn lump(token: u16) -> Vec<(MeshData, f32)> {
    retoken(sphere_levels(), |v| v.as_part(token, Grain::Solid, None))
}

/// The unit sphere's levels drawn in their instance's colour with the token pattern `pattern`
/// over them (`surface.wgsl`'s `PATTERN_*`): a stone in its rock's colour (rocks are the
/// palette's by data, `art-style.md` §3.1, not tokens).
pub fn patterned_sphere(pattern: u8) -> Vec<(MeshData, f32)> {
    retoken(sphere_levels(), |v| v.with_pattern(pattern))
}

fn retoken(
    levels: Vec<(MeshData, f32)>,
    each: impl Fn(MeshVertex) -> MeshVertex,
) -> Vec<(MeshData, f32)> {
    (levels.into_iter())
        .map(|(mut mesh, error)| {
            for v in &mut mesh.vertices {
                *v = each(*v);
            }
            (mesh, error)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A log's caps are its cut wood's, its side its bark's with the marks, every level alike;
    /// a board is sawn all over; each keeps its primitive's levels and bevels.
    #[test]
    fn a_part_s_faces_name_their_tokens() {
        let levels = cylinder(Faces::log(3, 9, Some(4)));
        assert_eq!(levels.len(), cylinder_levels().len());
        for (mesh, _) in &levels {
            for v in &mesh.vertices {
                assert!(v.is_part());
                let token = v.material & 0x7fff;
                let grain = (v.detail >> 8) & 0xff;
                if v.normal[1].abs() > 0.99 {
                    assert_eq!((token, grain), (9, Grain::Round as u32), "a cap");
                    assert!(v.grain[0] < 0.0, "no marks on the cut");
                } else if v.normal[1].abs() < 0.01 {
                    assert_eq!((token, grain), (3, Grain::Bark as u32), "the side");
                    assert!((v.grain[0] - 4.0).abs() < 1e-6, "marked");
                }
            }
        }
        let boards = board(2);
        assert!(
            !boards[0].0.vertices[0].is_bevelled(),
            "the sharp level stays sharp"
        );
        assert!(boards[1].0.vertices.iter().all(MeshVertex::is_bevelled));
        assert!(
            (boards.iter().flat_map(|(m, _)| &m.vertices))
                .all(|v| v.material & 0x7fff == 2 && (v.detail >> 8) & 0xff == Grain::Sawn as u32)
        );
        let stone = patterned_sphere(4);
        assert!(
            stone[0]
                .0
                .vertices
                .iter()
                .all(|v| !v.is_part() && (v.detail >> 8) & 0xff == 4)
        );
    }
}
