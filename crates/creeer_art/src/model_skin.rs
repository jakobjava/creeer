//! Skinned models (G1.3f, ADR-0026 decisions 6 and 7): the engineer and what he wears. A skinned
//! file has one skin, its joints the ragdoll's segments by name, each bone's head at its joint
//! with its parent in the ragdoll's standing frame. Its meshes are named
//! `<piece>:<region>@<level>`: a piece (the body, a wearable), one of [`REGIONS`] or what lies
//! inside a wearable ([`inside`]), and a level of detail (0 close-up, 1 mid, 2 far), so an outfit
//! can leave out the regions it hides and draw each level by its screen error. The file stands at rest: each joint's place in the scene
//! undoes its inverse bind matrix, so the vertices as written are the model standing (glTF
//! ignores a skinned mesh's own node's place).

use creeer_math::glam::Mat4;
use std::collections::HashMap;

/// The regions a body or a wearable is divided into (ADR-0026 decision 7): what a wearable may
/// hide of the body and of the layers beneath.
pub const REGIONS: [&str; 18] = [
    "head",
    "face",
    "hair",
    "neck",
    "torso",
    "pelvis",
    "upper_arm.L",
    "upper_arm.R",
    "forearm.L",
    "forearm.R",
    "hand.L",
    "hand.R",
    "thigh.L",
    "thigh.R",
    "shank.L",
    "shank.R",
    "foot.L",
    "foot.R",
];

/// The wearable a region `in.<wearable>` lies inside (G1.3f, ADR-0026 decision 7): the part of a
/// piece that wearable encloses, hidden while it is worn (`art/engineer/inside.py`). Skinning
/// cannot part the layers a joint closed fully presses together; hidden, the one beneath is not
/// drawn there to show through.
pub fn inside(region: &str) -> Option<&str> {
    region.strip_prefix("in.").filter(|w| !w.is_empty())
}

/// The levels of detail: close-up, mid and far (`art-style.md` §7).
pub const LEVELS: u8 = 3;

/// A bone of the skin: its name and its head where the model stands at rest (m, +Y up).
#[derive(Clone, Debug, PartialEq)]
pub struct Bone {
    pub name: String,
    pub head: [f32; 3],
}

/// One piece's region at one level: a run of the model's indices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub piece: String,
    pub region: String,
    pub level: u8,
    /// Its first index in the model's indices, and how many (three a triangle).
    pub first: usize,
    pub count: usize,
}

impl Part {
    /// Its triangle count.
    pub fn triangles(&self) -> usize {
        self.count / 3
    }
}

/// `<piece>:<region>@<level>` read as its three.
///
/// # Errors
/// If it is not of that form, its region is neither one of [`REGIONS`] nor [`inside`] a
/// wearable, or its level is past the last.
pub fn parse_part(name: &str) -> Result<(String, String, u8), String> {
    let bad = || format!("a part named {name:?}, not <piece>:<region>@<level>");
    let (piece, rest) = name.split_once(':').ok_or_else(bad)?;
    let (region, level) = rest.rsplit_once('@').ok_or_else(bad)?;
    let level: u8 = level.parse().map_err(|_| bad())?;
    if piece.is_empty() {
        return Err(bad());
    }
    if !REGIONS.contains(&region) && inside(region).is_none() {
        return Err(format!("{name}: no region {region:?}"));
    }
    if level >= LEVELS {
        return Err(format!(
            "{name}: level {level}, past the last ({})",
            LEVELS - 1
        ));
    }
    Ok((piece.to_owned(), region.to_owned(), level))
}

/// Every node's transform in the world of the file's default scene.
pub(crate) fn worlds(scene: &gltf::Scene) -> HashMap<usize, Mat4> {
    let mut out = HashMap::new();
    let mut stack: Vec<(gltf::Node, Mat4)> = scene.nodes().map(|n| (n, Mat4::IDENTITY)).collect();
    while let Some((node, parent)) = stack.pop() {
        let at = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
        out.insert(node.index(), at);
        stack.extend(node.children().map(|c| (c, at)));
    }
    out
}

/// The file's skin as bones, in its joints' order (empty without one).
///
/// # Errors
/// If it has more than one skin, more joints than a vertex can name, a joint without a name or
/// outside the scene, or one not at rest (its place and inverse bind matrix not undoing each
/// other to 10 µm).
pub(crate) fn bones(
    gltf: &gltf::Gltf,
    worlds: &HashMap<usize, Mat4>,
    blob: Option<&[u8]>,
) -> Result<Vec<Bone>, String> {
    let mut skins = gltf.skins();
    let Some(skin) = skins.next() else {
        return Ok(Vec::new());
    };
    if skins.next().is_some() {
        return Err("more than one skin: one skeleton, the ragdoll's".into());
    }
    if skin.joints().len() > usize::from(u8::MAX) {
        return Err(format!("{} joints, over {}", skin.joints().len(), u8::MAX));
    }
    let reader = skin.reader(|buffer| match buffer.source() {
        gltf::buffer::Source::Bin => blob,
        gltf::buffer::Source::Uri(_) => None,
    });
    let unbind: Vec<Mat4> = match reader.read_inverse_bind_matrices() {
        Some(m) => m.map(|m| Mat4::from_cols_array_2d(&m)).collect(),
        None => vec![Mat4::IDENTITY; skin.joints().len()],
    };
    if unbind.len() != skin.joints().len() {
        return Err("inverse bind matrices of a different count than the joints".into());
    }
    skin.joints()
        .zip(unbind)
        .map(|(joint, unbind)| {
            let name = joint.name().ok_or("a joint without a name")?;
            let at = worlds
                .get(&joint.index())
                .ok_or_else(|| format!("joint {name} is not in the scene"))?;
            if !(*at * unbind).abs_diff_eq(Mat4::IDENTITY, 1e-5) {
                return Err(format!(
                    "joint {name} is not at rest: its place and inverse bind matrix differ"
                ));
            }
            Ok(Bone {
                name: name.to_owned(),
                head: at.w_axis.truncate().to_array(),
            })
        })
        .collect()
}
