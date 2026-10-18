use super::profiler_ui::ProfilerWindow;
use creeer_core::profile;
use creeer_ui::egui;

/// The profiler window draws this process's ticks (spans on two threads, nested) without a
/// window.
#[test]
fn the_profiler_window_draws_this_processs_ticks() {
    let ctx = egui::Context::default();
    let mut window = ProfilerWindow::default();
    window.toggle();
    let draw = |window: &mut ProfilerWindow| {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            window.window(ui.ctx(), None)
        });
        output.textures_delta.clear();
    };
    draw(&mut window);
    assert!(profile::enabled(), "open, it wants a profile");
    // Frames cut here unless a clock is held elsewhere in the process (then theirs).
    let mut clock = profile::clock(1);
    for tick in 1..=5 {
        {
            let _tick = profile::scope("simulation tick");
            let _inner = profile::scope("physics: groups");
            std::thread::scope(|s| {
                s.spawn(|| drop(profile::scope("physics: group")));
            });
        }
        if let Some(c) = &mut clock {
            c.end_tick(tick);
        }
        draw(&mut window);
    }
    window.toggle();
    draw(&mut window);
    drop(clock);
}
