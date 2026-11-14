//! The player's settings (G2.11 A3, ADR-0038): the mouse, the view, the interface's size, the
//! sound's volume and the controls, kept in a file of their own and loaded at start. The menu
//! (`menu.rs`) changes them and saves the file at once.
//!
//! The file: `settings.ron` in the player's settings folder (Windows `%APPDATA%\Creeer`, macOS
//! `~/Library/Application Support/Creeer`, Linux `$XDG_CONFIG_HOME/creeer` or `~/.config/creeer`),
//! or where `CREEER_SETTINGS` points. A value missing from it takes its default, one out of range
//! is brought into range, and a file that does not read is set aside (the defaults are used).

use crate::bindings::Bindings;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameSettings {
    /// How fast the view turns with the mouse, as a share of the base speed (0.1–5).
    pub mouse_sensitivity: f64,
    /// Mouse up looks down.
    pub invert_y: bool,
    /// The view's field of view, top to bottom (degrees, 50–110).
    pub field_of_view: f32,
    /// The interface's size, as a share of the screen's own scale (0.5–3).
    pub ui_scale: f32,
    /// The sound's volume, 0 to 1 (G2.11 A7 plays at it).
    pub volume: f32,
    /// What the crosshair is on, and what can be done with it, told beside the crosshair (else
    /// at the bottom of the screen, above what the hands can do: G2.11 Q).
    pub prompts_beside: bool,
    /// Whether the player has opened the controls once: until then the screen says how.
    pub controls_seen: bool,
    /// The controls the player moved, by action id (`bindings.rs`); the rest are the defaults.
    pub controls: BTreeMap<String, Vec<String>>,
    /// The controls as played (from `controls`).
    #[serde(skip)]
    pub bindings: Bindings,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            mouse_sensitivity: 1.0,
            invert_y: false,
            field_of_view: 70.0,
            ui_scale: 1.0,
            volume: 0.8,
            prompts_beside: false,
            controls_seen: false,
            controls: BTreeMap::new(),
            bindings: Bindings::default(),
        }
    }
}

/// The ranges the menu's sliders give, and a loaded file is brought into.
pub const SENSITIVITY: (f64, f64) = (0.1, 5.0);
pub const FIELD_OF_VIEW: (f32, f32) = (50.0, 110.0);
pub const UI_SCALE: (f32, f32) = (0.5, 3.0);

impl GameSettings {
    /// From the text of a settings file. Unknown controls are left out (and printed).
    ///
    /// # Errors
    /// If it does not read as settings.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut s: Self = ron::from_str(text).map_err(|e| e.to_string())?;
        let (bindings, unknown) = Bindings::with_changes(&s.controls);
        if !unknown.is_empty() {
            eprintln!("settings: controls not known to this version, left out: {unknown:?}");
        }
        s.bindings = bindings;
        s.controls = s.bindings.changes();
        s.mouse_sensitivity = clamp(s.mouse_sensitivity, SENSITIVITY);
        s.field_of_view = clamp(s.field_of_view, FIELD_OF_VIEW);
        s.ui_scale = clamp(s.ui_scale, UI_SCALE);
        s.volume = clamp(s.volume, (0.0, 1.0));
        Ok(s)
    }

    /// As the file keeps them.
    pub fn text(&self) -> String {
        let mut kept = self.clone();
        kept.controls = self.bindings.changes();
        let pretty = ron::ser::PrettyConfig::new();
        let body = ron::ser::to_string_pretty(&kept, pretty).unwrap_or_default();
        format!(
            "// Creëer's settings: changed in the game's menu (Esc). Delete this file to go back \
             to the defaults.\n{body}\n"
        )
    }

    /// From the file at `path`: the defaults if there is none, or if it does not read (then it
    /// is renamed `.bad`, so the player's file is not lost when this one is saved).
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        Self::parse(&text).unwrap_or_else(|e| {
            eprintln!(
                "settings: {} does not read ({e}); the defaults are used",
                path.display()
            );
            let _ = std::fs::rename(path, path.with_extension("ron.bad"));
            Self::default()
        })
    }

    /// Writes the file at `path` (its folder made if need be).
    ///
    /// # Errors
    /// If it cannot be written.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, self.text()).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// `x` within `low..=high`; not a number, the low end.
fn clamp<T: PartialOrd + Copy>(x: T, (low, high): (T, T)) -> T {
    use std::cmp::Ordering::{Greater, Less};
    match (x.partial_cmp(&low), x.partial_cmp(&high)) {
        (Some(Less) | None, _) => low,
        (_, Some(Greater)) => high,
        _ => x,
    }
}

/// Where the settings file is: `CREEER_SETTINGS`, or the platform's settings folder (none if
/// the environment names none: then settings are not kept).
pub fn path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("CREEER_SETTINGS") {
        return Some(PathBuf::from(p));
    }
    let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let folder = if cfg!(target_os = "windows") {
        var("APPDATA")?.join("Creeer")
    } else if cfg!(target_os = "macos") {
        var("HOME")?.join("Library/Application Support/Creeer")
    } else {
        (var("XDG_CONFIG_HOME").or_else(|| var("HOME").map(|h| h.join(".config"))))?.join("creeer")
    };
    Some(folder.join("settings.ron"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bindings::{Action, Input};
    use winit::keyboard::KeyCode;

    #[test]
    fn settings_are_saved_and_loaded_the_same() {
        let mut s = GameSettings {
            mouse_sensitivity: 1.7,
            invert_y: true,
            field_of_view: 85.0,
            ui_scale: 1.5,
            volume: 0.25,
            controls_seen: true,
            ..GameSettings::default()
        };
        s.bindings.bind(Action::Fasten, Input::Key(KeyCode::KeyN));
        let back = GameSettings::parse(&s.text()).expect("reads");
        assert_eq!(back.bindings, s.bindings);
        assert_eq!(back.controls.len(), 1);
        assert_eq!((back.mouse_sensitivity, back.invert_y), (1.7, true));
        assert_eq!(
            (back.field_of_view, back.ui_scale, back.volume),
            (85.0, 1.5, 0.25)
        );
        assert!(back.controls_seen);
    }

    #[test]
    fn missing_values_take_their_defaults_and_wild_ones_are_brought_into_range() {
        let s = GameSettings::parse("(field_of_view: 400.0, ui_scale: 0.01)").expect("reads");
        assert_eq!(s.field_of_view, FIELD_OF_VIEW.1);
        assert_eq!(s.ui_scale, UI_SCALE.0);
        assert_eq!(s.mouse_sensitivity, 1.0);
        assert_eq!(s.bindings, Bindings::default());
        assert!(GameSettings::parse("not settings").is_err());
    }

    #[test]
    fn a_file_that_does_not_read_is_set_aside_and_the_defaults_used() {
        let folder = std::env::temp_dir().join(format!("creeer-settings-{}", std::process::id()));
        let path = folder.join("settings.ron");
        assert_eq!(
            GameSettings::load(&path),
            GameSettings::default(),
            "no file yet"
        );
        std::fs::create_dir_all(&folder).expect("folder");
        std::fs::write(&path, "(volume: oops)").expect("written");
        assert_eq!(GameSettings::load(&path), GameSettings::default());
        assert!(path.with_extension("ron.bad").exists(), "kept aside");
        let s = GameSettings {
            volume: 0.5,
            ..GameSettings::default()
        };
        s.save(&path).expect("saved");
        assert_eq!(GameSettings::load(&path).volume, 0.5);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
