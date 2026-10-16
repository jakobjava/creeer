//! The tech demo's Sandbox window.

use super::*;

impl Running {
    pub(super) fn sandbox_window(&mut self, ctx: &egui::Context, snapshot: &SandboxSnapshot) {
        egui::Window::new("Sandbox")
            .default_pos([12.0, 330.0])
            .resizable(false)
            .show(ctx, |ui| {
                if ui.button("Planet sandbox (F3) ▸").clicked() {
                    self.enter_planet();
                }
                if ui.checkbox(&mut self.settings.vsync, "VSync").changed() {
                    self.renderer.set_vsync(self.settings.vsync);
                }
                ui.checkbox(&mut self.settings.factory, "Factory (10,000 parts)");
                ui.horizontal(|ui| {
                    ui.label("Overlay:");
                    for (overlay, name) in [
                        (Overlay::Off, "off"),
                        (Overlay::Temperature, "temperature"),
                        (Overlay::AxialStress, "axial stress"),
                    ] {
                        ui.radio_value(&mut self.settings.overlay, overlay, name);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Shadows:");
                    let before = self.settings.shadows;
                    for (quality, name) in [
                        (ShadowQuality::Low, "low"),
                        (ShadowQuality::Medium, "medium"),
                        (ShadowQuality::High, "high"),
                    ] {
                        ui.radio_value(&mut self.settings.shadows, quality, name);
                    }
                    if self.settings.shadows != before {
                        self.renderer.set_shadow_quality(self.settings.shadows);
                    }
                });
                ui.add(
                    egui::Slider::new(&mut self.settings.sun_elevation, -12.0..=90.0)
                        .text("sun elevation (°)"),
                );
                let stats = self.renderer.stats();
                let build_ms = self.scene_build_times.percentile_ms(50.0).unwrap_or(0.0);
                ui.label(format!(
                    "drawn {} · culled {} · {} draw calls · scene build {build_ms:.2} ms",
                    stats.instances_drawn, stats.instances_culled, stats.draw_calls
                ));
                ui.separator();
                let delay = ui.add(
                    egui::Slider::new(&mut self.settings.tick_delay_ms, 0.0..=40.0)
                        .text("extra tick time (ms)"),
                );
                if delay.changed() {
                    self.sim.set_extra_tick_delay(Duration::from_secs_f32(
                        self.settings.tick_delay_ms / 1000.0,
                    ));
                }
                let far = ui.checkbox(
                    &mut self.settings.far_origin,
                    "Move scene 500 km from the origin",
                );
                if far.changed() {
                    // Shift the camera with the scene so the view stays identical.
                    let shift = if self.settings.far_origin {
                        FAR_ORIGIN
                    } else {
                        -FAR_ORIGIN
                    };
                    self.camera.position += shift;
                }
                ui.separator();
                ui.label(format!("simulated time: {:.1} s", snapshot.time));
                let speed = self
                    .sim_speed
                    .rate()
                    .map_or_else(|| "-".to_owned(), |r| format!("{r:.2}"));
                ui.label(format!("simulation speed: {speed}x real time"));
                ui.label(format!(
                    "orbital energy drift: {:+.2e} (leapfrog, should stay tiny)",
                    snapshot.energy_drift
                ));
                let p = self.camera.position;
                ui.label(format!("camera: ({:.3}, {:.3}, {:.3}) m", p.x, p.y, p.z));
                ui.label(format!("fly speed: {:.1} m/s", self.fly.speed));
                ui.separator();
                ui.label("Right mouse: look · WASD: move · Space/Ctrl: up/down");
                ui.label("Shift: ×10 speed · Mouse wheel: change speed");
            });
    }
}
