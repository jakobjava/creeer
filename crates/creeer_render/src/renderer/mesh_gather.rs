//! A frame's instances gathered into the mesh pass's lists (`mesh_pass.rs`): culled for the
//! camera and for each shadow cascade, and a mesh with levels of detail drawn at each
//! instance's level, on screen and in each cascade (`levels.rs`).

use super::*;
use crate::mesh::Levels;
use creeer_math::glam::{Quat, Vec3};

/// The most levels of detail a mesh may have.
pub(super) const MAX_LEVELS: usize = 16;

/// What instances are culled against, and their levels chosen for, this frame.
pub(super) struct Cull<'a> {
    pub camera: DVec3,
    pub frustum: Frustum,
    pub shadows: Option<(&'a [Cascade; CASCADES], &'a LightBasis)>,
    /// The viewport's height in pixels over 2·tan(fov/2).
    pub focal: f32,
}

impl MeshPass {
    /// Gathers `batch`, the instances of the mesh registered as `levels`, into the passes'
    /// lists, recording their draws in `pass_draws` (each pass's own). Each instance is
    /// converted once, and lands in each pass it reaches at its level there.
    pub(super) fn gather_mesh(
        &mut self,
        pass_draws: &mut [Vec<Draw>],
        meshes: &MeshArena,
        levels: Levels,
        batch: &[Instance],
        cull: &Cull<'_>,
        stats: &mut RenderStats,
    ) {
        assert!(levels.count <= MAX_LEVELS, "too many levels of detail");
        let ranges = &meshes.ranges[levels.first..levels.first + levels.count];
        // Each pass and level's instances, in `self.buckets[pass * MAX_LEVELS + level]`.
        for instance in batch {
            let raw = InstanceRaw::new(instance, cull.camera);
            let center = Vec3::from(raw.position);
            let bound = ranges[0].bounding_radius * instance.scale.max_element();
            if cull.frustum.intersects_sphere(center, bound) {
                // On screen: the coarsest level under half a pixel.
                let k = levels.round.map_or(0, |round| {
                    let rotation = Quat::from_array(raw.rotation);
                    levels::level(ranges, round, center, rotation, instance.scale, cull.focal)
                });
                self.buckets[k].push(raw);
            } else {
                stats.instances_culled += 1;
            }
            if let Some((cascades, basis)) = cull.shadows {
                let radius = levels
                    .round
                    .map_or(0.0, |round| levels::round_radius(round, instance.scale));
                for (c, cascade) in cascades.iter().enumerate() {
                    if cascade.may_cast_into(basis, center, bound) {
                        // In a cascade: the coarsest level under half a texel of its map.
                        let k = if levels.round.is_some() {
                            levels::shadow_level(ranges, radius, cascade.texel)
                        } else {
                            0
                        };
                        self.buckets[(1 + c) * MAX_LEVELS + k].push(raw);
                    }
                }
            }
        }
        for (p, list) in self.pass_scratch.iter_mut().enumerate() {
            for k in 0..levels.count {
                let bucket = &mut self.buckets[p * MAX_LEVELS + k];
                if bucket.is_empty() {
                    continue;
                }
                pass_draws[p].push(Draw {
                    range: levels.first + k,
                    first_instance: u32::try_from(list.len()).expect("instance count"),
                    instance_count: u32::try_from(bucket.len()).expect("instance count"),
                });
                list.append(bucket);
            }
        }
    }
}
