//! The keys and buttons a control can be bound to, with their names on screen and in the
//! settings file (`bindings.rs`). A key missing here cannot be bound; to allow one, add a row
//! to [`KEYS`].

use winit::event::MouseButton;
use winit::keyboard::KeyCode as K;

/// A key, a mouse button or the mouse wheel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Input {
    Key(K),
    Mouse(MouseButton),
    Wheel,
}

/// The keys that may be bound: each with its name in the settings file and on screen.
#[rustfmt::skip]
const KEYS: &[(K, &str, &str)] = &[
    (K::KeyA, "KeyA", "A"), (K::KeyB, "KeyB", "B"), (K::KeyC, "KeyC", "C"), (K::KeyD, "KeyD", "D"),
    (K::KeyE, "KeyE", "E"), (K::KeyF, "KeyF", "F"), (K::KeyG, "KeyG", "G"), (K::KeyH, "KeyH", "H"),
    (K::KeyI, "KeyI", "I"), (K::KeyJ, "KeyJ", "J"), (K::KeyK, "KeyK", "K"), (K::KeyL, "KeyL", "L"),
    (K::KeyM, "KeyM", "M"), (K::KeyN, "KeyN", "N"), (K::KeyO, "KeyO", "O"), (K::KeyP, "KeyP", "P"),
    (K::KeyQ, "KeyQ", "Q"), (K::KeyR, "KeyR", "R"), (K::KeyS, "KeyS", "S"), (K::KeyT, "KeyT", "T"),
    (K::KeyU, "KeyU", "U"), (K::KeyV, "KeyV", "V"), (K::KeyW, "KeyW", "W"), (K::KeyX, "KeyX", "X"),
    (K::KeyY, "KeyY", "Y"), (K::KeyZ, "KeyZ", "Z"),
    (K::Digit0, "Digit0", "0"), (K::Digit1, "Digit1", "1"), (K::Digit2, "Digit2", "2"),
    (K::Digit3, "Digit3", "3"), (K::Digit4, "Digit4", "4"), (K::Digit5, "Digit5", "5"),
    (K::Digit6, "Digit6", "6"), (K::Digit7, "Digit7", "7"), (K::Digit8, "Digit8", "8"),
    (K::Digit9, "Digit9", "9"),
    (K::F1, "F1", "F1"), (K::F2, "F2", "F2"), (K::F3, "F3", "F3"), (K::F4, "F4", "F4"),
    (K::F5, "F5", "F5"), (K::F6, "F6", "F6"), (K::F7, "F7", "F7"), (K::F8, "F8", "F8"),
    (K::F9, "F9", "F9"), (K::F10, "F10", "F10"), (K::F11, "F11", "F11"), (K::F12, "F12", "F12"),
    (K::Space, "Space", "Space"), (K::Tab, "Tab", "Tab"), (K::Enter, "Enter", "Enter"),
    (K::Backspace, "Backspace", "Backspace"), (K::CapsLock, "CapsLock", "Caps Lock"),
    (K::Escape, "Escape", "Esc"),
    (K::ShiftLeft, "ShiftLeft", "Shift"), (K::ShiftRight, "ShiftRight", "Right Shift"),
    (K::ControlLeft, "ControlLeft", "Ctrl"), (K::ControlRight, "ControlRight", "Right Ctrl"),
    (K::AltLeft, "AltLeft", "Alt"), (K::AltRight, "AltRight", "Right Alt"),
    (K::ArrowUp, "ArrowUp", "Up"), (K::ArrowDown, "ArrowDown", "Down"),
    (K::ArrowLeft, "ArrowLeft", "Left"), (K::ArrowRight, "ArrowRight", "Right"),
    (K::Insert, "Insert", "Insert"), (K::Delete, "Delete", "Delete"), (K::Home, "Home", "Home"),
    (K::End, "End", "End"), (K::PageUp, "PageUp", "Page Up"), (K::PageDown, "PageDown", "Page Down"),
    (K::Backquote, "Backquote", "`"), (K::Minus, "Minus", "-"), (K::Equal, "Equal", "="),
    (K::BracketLeft, "BracketLeft", "["), (K::BracketRight, "BracketRight", "]"),
    (K::Backslash, "Backslash", "\\"), (K::Semicolon, "Semicolon", ";"), (K::Quote, "Quote", "'"),
    (K::Comma, "Comma", ","), (K::Period, "Period", "."), (K::Slash, "Slash", "/"),
    (K::Numpad0, "Numpad0", "Num 0"), (K::Numpad1, "Numpad1", "Num 1"),
    (K::Numpad2, "Numpad2", "Num 2"), (K::Numpad3, "Numpad3", "Num 3"),
    (K::Numpad4, "Numpad4", "Num 4"), (K::Numpad5, "Numpad5", "Num 5"),
    (K::Numpad6, "Numpad6", "Num 6"), (K::Numpad7, "Numpad7", "Num 7"),
    (K::Numpad8, "Numpad8", "Num 8"), (K::Numpad9, "Numpad9", "Num 9"),
];

/// The mouse's buttons that may be bound, likewise.
const BUTTONS: &[(MouseButton, &str, &str)] = &[
    (MouseButton::Left, "MouseLeft", "Left click"),
    (MouseButton::Right, "MouseRight", "Right mouse"),
    (MouseButton::Middle, "MouseMiddle", "Middle mouse"),
    (MouseButton::Back, "MouseBack", "Mouse 4"),
    (MouseButton::Forward, "MouseForward", "Mouse 5"),
];

/// Whether `input` may be bound at all (a key or button in the lists above).
pub fn bindable(input: Input) -> bool {
    match input {
        Input::Key(k) => KEYS.iter().any(|(x, ..)| *x == k),
        Input::Mouse(b) => BUTTONS.iter().any(|(x, ..)| *x == b),
        Input::Wheel => true,
    }
}

/// `input`'s name on screen ("G", "Ctrl", "Left click", "Wheel").
pub fn input_name(input: Input) -> String {
    let found = match input {
        Input::Key(k) => KEYS.iter().find(|(x, ..)| *x == k).map(|(_, _, n)| *n),
        Input::Mouse(b) => BUTTONS.iter().find(|(x, ..)| *x == b).map(|(_, _, n)| *n),
        Input::Wheel => Some("Wheel"),
    };
    found.map_or_else(|| format!("{input:?}"), str::to_owned)
}

/// `input`'s name in the settings file ("KeyG", "MouseLeft", "Wheel").
pub fn input_id(input: Input) -> String {
    let found = match input {
        Input::Key(k) => KEYS.iter().find(|(x, ..)| *x == k).map(|(_, id, _)| *id),
        Input::Mouse(b) => BUTTONS.iter().find(|(x, ..)| *x == b).map(|(_, id, _)| *id),
        Input::Wheel => Some("Wheel"),
    };
    found.map_or_else(|| format!("{input:?}"), str::to_owned)
}

/// The input named `id` in the settings file, if it is one that may be bound.
pub fn parse_input(id: &str) -> Option<Input> {
    if id == "Wheel" {
        return Some(Input::Wheel);
    }
    (KEYS
        .iter()
        .find(|(_, x, _)| *x == id)
        .map(|(k, ..)| Input::Key(*k)))
    .or_else(|| (BUTTONS.iter().find(|(_, x, _)| *x == id)).map(|(b, ..)| Input::Mouse(*b)))
}
