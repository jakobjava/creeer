//! Models (G1.3b, ADR-0026): binary glTF files that the scripts in `art/` make
//! (`docs/blender-mcp.md`), read into one mesh whose vertices name palette tokens. The
//! manifest (`data/models/models.ron`) says what each model is, and [`crate::model_check`]
//! holds it to the style (`docs/art-style.md` §7, §11.3). A skinned model (the engineer,
//! G1.3f) also has its bones, each vertex's weights on them, and its parts
//! ([`crate::model_skin`]).

use crate::model_skin::{self, Bone, Part};
use crate::palette::Palette;
use creeer_data::Data;
use creeer_data::merge::ron_list;
use creeer_math::glam::{Mat3, Mat4, Vec3};
use gltf::buffer::Source;
use gltf::mesh::Mode;
use serde::Deserialize;

/// One vertex: where, which way it faces, its token and its baked masks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    /// Position (m), +Y up, the model's origin at its rest point.
    pub position: [f32; 3],
    /// Unit normal.
    pub normal: [f32; 3],
    /// Index of its palette token.
    pub token: u16,
    /// Baked ambient occlusion: 1 open, 0 fully occluded.
    pub occlusion: f32,
    /// Baked edge wear: 0 none, 1 full.
    pub wear: f32,
    /// Its part's tone: 0.5 the token's colour, 0 and 1 the darkest and lightest.
    pub tone: f32,
    /// Its height above the model's base over `art/bake.py`'s HEIGHT_RANGE (2 m), clamped.
    pub height: f32,
    /// Grain coordinates (m): along its part's grain, and across it.
    pub grain: [f32; 2],
    /// A skinned model's: the bones (indices into [`Model::bones`]) that move it, and how much
    /// each (summing to one); all nought for a model without a skin.
    pub bones: [u8; 4],
    pub weights: [f32; 4],
}

/// The range heights above a model's base are stored over (m; `art/bake.py`).
pub const HEIGHT_RANGE: f32 = 2.0;

/// A model as one triangle mesh.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    pub vertices: Vec<Vertex>,
    /// Triangles, counter-clockwise from the front.
    pub indices: Vec<u32>,
    /// A skinned model's bones, in its skin's order (empty without one).
    pub bones: Vec<Bone>,
    /// A skinned model's parts: each piece's region at each level, a run of the indices.
    pub parts: Vec<Part>,
}

/// What kind of thing a model is: its triangle budget (`art-style.md` §7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Class {
    /// The character as drawn, with everything worn.
    Character,
    /// One wearable.
    Wearable,
    /// A tool.
    Tool,
    /// An item, bag or crate.
    Item,
    /// A decorative.
    Decorative,
}

impl Class {
    /// Its most triangles at LOD 0 (the close-up level).
    pub fn budget(self) -> usize {
        self.level_budget(0)
    }

    /// Its most triangles at level of detail `level` (0 close-up, 1 mid, 2 far): the
    /// character's measured before modelling (ADR-0026, Cost), a wearable's 4,000 at the mid
    /// level and the others in the character's proportion; a model without levels has only 0.
    pub fn level_budget(self, level: u8) -> usize {
        const CHARACTER: [usize; 3] = [50_000, 15_000, 3_000];
        let k = usize::from(level.min(2));
        match self {
            Self::Character => CHARACTER[k],
            Self::Wearable => 4_000 * CHARACTER[k] / CHARACTER[1],
            Self::Tool => 2_000,
            Self::Item => 1_000,
            Self::Decorative => 200,
        }
    }
}

/// A model's entry in the manifest.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub name: String,
    /// Its file in the packs.
    pub file: String,
    pub class: Class,
    /// Its bounding box (m), which says its size and where its origin is.
    pub bounds: ([f64; 3], [f64; 3]),
    /// A model in levels of detail: each level's error against the close-up one (m), as
    /// `model_levels::level_errors` measures it (too slow to measure on every load).
    #[serde(default)]
    pub levels: Option<[f32; 3]>,
}

/// Where the manifest lives in a pack.
pub const MANIFEST: &str = "models/models.ron";

impl Entry {
    /// Every model's entry, as the packs merge them by name.
    ///
    /// # Errors
    /// If a manifest does not parse, or a name repeats within a pack.
    pub fn all(data: &Data) -> Result<Vec<Self>, String> {
        (data.table("models", MANIFEST))
            .merge(ron_list, |e: &Self| &e.name)
            .map_err(|e| e.to_string())
    }
}

impl Model {
    /// The model in the binary glTF `bytes`, its materials resolved against `palette`.
    ///
    /// # Errors
    /// If it does not parse; if it has images or textures, more than one skin, a primitive that
    /// is not triangles, or one without positions, normals, indices or a material; if a
    /// material is not a palette token; or, skinned, if a mesh is not named as a part, or a
    /// vertex lacks its bones or names one the skin lacks.
    pub fn read(bytes: &[u8], palette: &Palette) -> Result<Self, String> {
        let gltf = gltf::Gltf::from_slice(bytes).map_err(|e| format!("not a glTF: {e}"))?;
        if gltf.images().len() > 0 || gltf.textures().len() > 0 {
            return Err("it has images: the style uses none (art-style.md §6)".into());
        }
        let scene = gltf
            .default_scene()
            .or_else(|| gltf.scenes().next())
            .ok_or("it has no scene")?;
        let worlds = model_skin::worlds(&scene);
        let blob = gltf.blob.as_deref();
        let mut model = Self {
            bones: model_skin::bones(&gltf, &worlds, blob)?,
            ..Self::default()
        };
        let skinned = !model.bones.is_empty();
        // In node order, so a file reads the same every time.
        let mut nodes: Vec<gltf::Node> = gltf
            .nodes()
            .filter(|n| worlds.contains_key(&n.index()))
            .collect();
        nodes.sort_by_key(gltf::Node::index);
        for node in nodes {
            let Some(mesh) = node.mesh() else { continue };
            let (at, part) = if skinned {
                let name = node.name().ok_or("a skinned mesh without a name")?;
                if node.skin().is_none() {
                    return Err(format!("{name}: a mesh not on the skin"));
                }
                // Moved by its bones alone, which stand at rest.
                (Mat4::IDENTITY, Some(model_skin::parse_part(name)?))
            } else {
                (worlds[&node.index()], None)
            };
            let first = model.indices.len();
            for primitive in mesh.primitives() {
                model.add(&primitive, at, blob, palette, skinned)?;
            }
            if let Some((piece, region, level)) = part {
                let count = model.indices.len() - first;
                model.parts.push(Part {
                    piece,
                    region,
                    level,
                    first,
                    count,
                });
            }
        }
        if model.indices.is_empty() {
            return Err("it has no triangles".into());
        }
        Ok(model)
    }

    /// Appends one primitive, placed by `at`.
    fn add(
        &mut self,
        primitive: &gltf::Primitive,
        at: Mat4,
        blob: Option<&[u8]>,
        palette: &Palette,
        skinned: bool,
    ) -> Result<(), String> {
        if primitive.mode() != Mode::Triangles {
            return Err(format!(
                "a primitive of {:?}, not triangles",
                primitive.mode()
            ));
        }
        let name = primitive
            .material()
            .name()
            .ok_or("a primitive without a material")?;
        let token = palette
            .index(name)
            .ok_or_else(|| format!("material {name:?} is not a palette token"))?;
        let token = u16::try_from(token).map_err(|_| "too many tokens")?;
        let reader = primitive.reader(|buffer| match buffer.source() {
            Source::Bin => blob,
            Source::Uri(_) => None,
        });
        let positions: Vec<[f32; 3]> = reader
            .read_positions()
            .ok_or_else(|| format!("{name}: no positions"))?
            .collect();
        let normals: Vec<[f32; 3]> = reader
            .read_normals()
            .ok_or_else(|| format!("{name}: no normals"))?
            .collect();
        // Unbaked: open, unworn, the token's own tone, at the base.
        let masks: Vec<[f32; 4]> = match reader.read_colors(0) {
            Some(colours) => colours.into_rgba_f32().collect(),
            None => vec![[1.0, 0.0, 0.5, 0.0]; positions.len()],
        };
        // glTF's v runs down where Blender's runs up: v' = 1 − v.
        let grain: Vec<[f32; 2]> = match reader.read_tex_coords(0) {
            Some(uv) => uv.into_f32().map(|[u, v]| [u, 1.0 - v]).collect(),
            None => vec![[0.0, 0.0]; positions.len()],
        };
        let (bones, weights) = if skinned {
            let bones: Vec<[u16; 4]> = reader
                .read_joints(0)
                .ok_or_else(|| format!("{name}: skinned, but no joints"))?
                .into_u16()
                .collect();
            let weights: Vec<[f32; 4]> = reader
                .read_weights(0)
                .ok_or_else(|| format!("{name}: skinned, but no weights"))?
                .into_f32()
                .collect();
            (bones, weights)
        } else {
            (
                vec![[0; 4]; positions.len()],
                vec![[0.0; 4]; positions.len()],
            )
        };
        if reader.read_joints(1).is_some() {
            return Err(format!("{name}: more than four bones a vertex"));
        }
        if normals.len() != positions.len()
            || masks.len() != positions.len()
            || grain.len() != positions.len()
            || bones.len() != positions.len()
            || weights.len() != positions.len()
        {
            return Err(format!("{name}: attributes of different lengths"));
        }
        let most = self.bones.len();
        if skinned && bones.iter().flatten().any(|&b| usize::from(b) >= most) {
            return Err(format!("{name}: a vertex on a bone the skin lacks"));
        }
        let indices: Vec<u32> = reader
            .read_indices()
            .ok_or_else(|| format!("{name}: no indices"))?
            .into_u32()
            .collect();
        let normal_matrix = Mat3::from_mat4(at).inverse().transpose();
        let base = u32::try_from(self.vertices.len()).map_err(|_| "too many vertices")?;
        for (k, (((p, n), m), g)) in positions
            .iter()
            .zip(&normals)
            .zip(&masks)
            .zip(&grain)
            .enumerate()
        {
            self.vertices.push(Vertex {
                position: at.transform_point3(Vec3::from(*p)).to_array(),
                normal: (normal_matrix * Vec3::from(*n))
                    .normalize_or_zero()
                    .to_array(),
                token,
                occlusion: m[0],
                wear: m[1],
                tone: m[2],
                height: m[3],
                grain: *g,
                // At most 255 bones (`model_skin::bones`), checked above against the skin's.
                #[allow(clippy::cast_possible_truncation)]
                bones: bones[k].map(|b| b as u8),
                weights: weights[k],
            });
        }
        for &i in &indices {
            if i as usize >= positions.len() {
                return Err(format!("{name}: an index past its vertices"));
            }
            self.indices.push(base + i);
        }
        Ok(())
    }

    /// Its triangle count.
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }

    /// Its bounding box (m).
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        for v in &self.vertices {
            lo = lo.min(Vec3::from(v.position));
            hi = hi.max(Vec3::from(v.position));
        }
        (lo, hi)
    }

    /// Its triangles in a form that ignores how the file happens to order and share vertices:
    /// each corner quantised (positions and normals to 10 µm and 10⁻⁴, masks and weights to
    /// 10⁻³, its token and bones exact), each triangle turned to begin at its least corner
    /// (keeping its winding), the triangles sorted. Two models with the same canonical form are
    /// the same model.
    pub fn canonical(&self) -> Vec<[[i64; 21]; 3]> {
        let corner = |i: u32| {
            let v = &self.vertices[i as usize];
            let q = |x: f32, step: f64| {
                #[allow(clippy::cast_possible_truncation)] // models are metres across
                let k = (f64::from(x) / step).round() as i64;
                k
            };
            [
                q(v.position[0], 1e-5),
                q(v.position[1], 1e-5),
                q(v.position[2], 1e-5),
                q(v.normal[0], 1e-4),
                q(v.normal[1], 1e-4),
                q(v.normal[2], 1e-4),
                i64::from(v.token),
                q(v.occlusion, 1e-3),
                q(v.wear, 1e-3),
                q(v.tone, 1e-3),
                q(v.height, 1e-4),
                q(v.grain[0], 1e-5),
                q(v.grain[1], 1e-5),
                i64::from(v.bones[0]),
                i64::from(v.bones[1]),
                i64::from(v.bones[2]),
                i64::from(v.bones[3]),
                q(v.weights[0], 1e-3),
                q(v.weights[1], 1e-3),
                q(v.weights[2], 1e-3),
                q(v.weights[3], 1e-3),
            ]
        };
        let mut out: Vec<[[i64; 21]; 3]> = self
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let c = [corner(t[0]), corner(t[1]), corner(t[2])];
                let first = (0..3).min_by_key(|&k| c[k]).unwrap_or(0);
                [c[first], c[(first + 1) % 3], c[(first + 2) % 3]]
            })
            .collect();
        out.sort_unstable();
        out
    }
}
