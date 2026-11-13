//! Keys, mouse buttons and the wheel, through the controls' table (`bindings.rs`, G2.11 A3):
//! an input names actions, and the first that applies now is done. Nothing here names a key.

use super::*;
use crate::bindings::{Action, Context, Input, entry};

impl Running {
    /// A key or button went down (`pressed`) or up: done by the first action bound to it that
    /// applies now (those of every context first, then holding a part's, then the rest).
    pub(super) fn input(&mut self, input: Input, pressed: bool) {
        let mut actions: Vec<Action> = self.game.bindings.actions(input).collect();
        actions.sort_by_key(|a| match entry(*a).context {
            Context::Always => 0,
            Context::Holding => 1,
            Context::Planet | Context::System => 2,
        });
        for action in actions {
            if self.act(action, pressed) {
                return;
            }
        }
    }

    /// Does `action` (`pressed`: its key went down, else up); whether it applied.
    fn act(&mut self, action: Action, pressed: bool) -> bool {
        let e = entry(action);
        if e.context == Context::Holding {
            return self.holding().is_some_and(|h| h.key(action, pressed));
        }
        if action == Action::Look {
            self.set_looking(pressed);
            return true;
        }
        if e.held {
            // Moving: the free camera's, or the avatar's walk.
            return self.fly.key(action, pressed);
        }
        if !pressed {
            return false;
        }
        match action {
            Action::Profiler => {
                self.profiler.toggle();
                true
            }
            Action::Use => self.use_at_crosshair(),
            _ => self.system_act(action) || self.planet_act(action),
        }
    }

    /// A click where the crosshair is: the planet's tool or hands, or digging into a body of the
    /// solar system.
    fn use_at_crosshair(&mut self) -> bool {
        if let Some(planet) = &mut self.planet {
            planet.use_tool(&self.camera);
            return true;
        }
        if let Some(system) = &self.system
            && !system.map
        {
            system.dig(&self.camera, system.snapshot().current.anchor);
            return true;
        }
        false
    }

    /// The wheel turned `notches`: slides what the hands hold, zooms the map, or sets the
    /// flying speed.
    pub(super) fn wheel(&mut self, notches: f64) {
        let held = self.holding().is_some_and(|h| h.wheel(notches));
        if !held && !self.system_scroll(notches) {
            self.fly.scroll(notches);
        }
    }

    /// The mouse moved by `delta` pixels: turns what the hands hold (Q held), else the view, at
    /// the player's sensitivity (G2.11: the settings).
    pub(super) fn mouse_moved(&mut self, delta: (f64, f64)) {
        let (turned, gain) = self.holding().map_or((false, 1.0), |h| h.mouse(delta));
        if !turned && self.looking && !self.system_mouse(delta.0, delta.1) {
            let gain = gain * self.game.mouse_sensitivity;
            let y = if self.game.invert_y { -1.0 } else { 1.0 };
            self.fly.mouse_motion(delta.0 * gain, delta.1 * gain * y);
        }
    }
}
