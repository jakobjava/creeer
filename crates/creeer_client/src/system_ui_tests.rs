use super::*;
use crate::system_frame;

/// The windows and the map labels run on a real sandbox without a window: flight view,
/// a ship in orbit, the map, a selected body, the co-rotating map.
#[test]
fn the_sandbox_windows_draw_without_panicking() {
    let universe = crate::app::system::load_universe().expect("system");
    let mut view = SystemView::new(universe, crate::TICK_RATE_HZ);
    let ctx = egui::Context::default();
    let camera = Camera::new(DVec3::new(0.0, 5.1e5, 0.0));
    let frame = |view: &mut SystemView| {
        std::thread::sleep(std::time::Duration::from_millis(200));
        let s = view.snapshot().current;
        let labels = if view.map {
            let bodies = s.bodies.to_vec();
            let plot = system_frame::plot_frame(view, &s);
            let mut scene = creeer_render::Scene::default();
            let mut lines = creeer_render::DebugLines::default();
            let meshes = crate::DemoMeshes::register(&mut creeer_render::MeshArena::default());
            crate::system_map::build(
                &mut scene,
                &mut lines,
                &meshes,
                &s,
                &bodies,
                (plot, camera.position),
            )
        } else {
            Vec::new()
        };
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let request = window(ui.ctx(), view, &s, &camera);
            assert_eq!(request, Request::None);
            paint_labels(ui.ctx(), &camera, &labels);
        });
        // No GPU here to upload the font atlas to.
        output.textures_delta.clear();
    };
    frame(&mut view);
    view.send(Command::ShipInOrbit(120_000.0));
    frame(&mut view);
    view.map = true;
    frame(&mut view);
    view.selected = Some(BodyRef::Major(0));
    frame(&mut view);
    view.corotating = true;
    view.selected = None;
    frame(&mut view);
    let s = view.snapshot().current;
    assert!(s.focus.is_some(), "the ship is focused");
    assert!(s.prediction.1.points.len() > 10, "a path was predicted");
}
