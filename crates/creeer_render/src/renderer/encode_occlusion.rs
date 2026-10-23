//! The frame's depth prepass and ambient occlusion (G1.3d), recorded before the scene pass.

use super::*;

impl Renderer {
    /// Pass 1a: the meshes', characters' and terrain's depth, for ambient occlusion and so the
    /// scene pass shades each pixel once (without occlusion it would not pay for itself); pass 1b:
    /// the occlusion from it. Returns whether the prepass was drawn.
    pub(super) fn encode_occlusion(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        camera: &Camera,
        aspect: f32,
    ) -> bool {
        let prepassed = self.ao.on();
        if prepassed {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("prepass"),
                timestamp_writes: self.timer.as_ref().and_then(|t| t.render("prepass")),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.targets.depth,
                    depth_ops: Some(wgpu::Operations {
                        // Reversed-Z: 0 is infinitely far away.
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_bind_group(0, &self.globals_bind_group, &[]);
            self.mesh_pass
                .draw_prepass(&mut pass, &self.meshes, &self.skin);
            self.terrain.draw_prepass(&mut pass);
        }
        // Pass 1b: ambient occlusion from that depth.
        self.ao.prepare(&self.queue, camera, aspect);
        self.ao.encode(encoder, self.timer.as_ref());
        prepassed
    }
}
