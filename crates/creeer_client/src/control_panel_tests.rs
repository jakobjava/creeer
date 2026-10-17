//! The control panel's rigs hold their setpoints with the scripts they start with; a runaway
//! script uploaded stops, and the next upload runs again with the memory the last one kept.

use super::*;

#[test]
fn the_rigs_hold_their_setpoints() {
    let mut panel = ControlPanel::new();
    for _ in 0..120 * 60 {
        panel.tick();
    }
    let (level, pump) = panel.last();
    assert!((level - 2.5).abs() < 0.01, "level {level} m, pump {pump}");
    panel.switch(true);
    for _ in 0..4 * 60 {
        panel.tick();
    }
    let (speed, supply) = panel.last();
    assert!(
        (speed / 150.0 - 1.0).abs() < 5e-3,
        "{speed} rad/s at {supply} V"
    );
}

#[test]
fn an_upload_stops_or_runs_with_the_memory_kept() {
    let mut panel = ControlPanel::new();
    for _ in 0..600 {
        panel.tick();
    }
    panel.edit_and_upload("function tick(dt) while true do end end");
    panel.tick();
    assert!(panel.status().contains("stopped"), "{}", panel.status());
    panel.edit_and_upload("function tick(dt) print(memory.integral > 0) output[1] = 1 end");
    panel.tick();
    let status = panel.status();
    assert!(
        status.contains("running") && status.ends_with("true"),
        "{status}"
    );
}
