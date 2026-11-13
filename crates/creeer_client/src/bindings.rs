//! The game's controls in one table (G2.11 A3, ADR-0037): every action the player can take, its
//! name in plain words, and the key or button that does it. The input handling (`app/input.rs`),
//! the prompts at the crosshair (`prompts.rs`) and the controls list in the menu all read this
//! table, so a key is named in one place and the screen can never disagree with the controls.
//!
//! HOW TO ADD AN ACTION: add it to [`Action`], give it a row in [`TABLE`] (its id for the
//! settings file, its name, group, context and default keys), and do it in `app/input.rs`
//! (`Running::act`). The controls list, rebinding and the settings file follow by themselves;
//! a prompt for it is one line in `prompts.rs`. To change a default key, change its row.

pub use crate::bindings_keys::{Input, bindable, input_id, input_name, parse_input};
use std::collections::BTreeMap;
use winit::event::MouseButton;
use winit::keyboard::KeyCode as K;

/// When an action applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Context {
    /// Everywhere, even with a menu open.
    Always,
    /// On the planet, walking or flying.
    Planet,
    /// On the planet while the hands hold a part: these come before the planet's on a key both
    /// use (Ctrl: finer while holding, else the jetpack).
    Holding,
    /// In the solar-system sandbox.
    System,
}

/// Everything the player can do with a key or a button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(missing_docs)] // named in the table
pub enum Action {
    Menu,
    Debug,
    Profiler,
    Forward,
    Back,
    Left,
    Right,
    Jump,
    Run,
    Jetpack,
    Look,
    FlySpeed,
    WalkOrFly,
    Camera,
    Use,
    Take,
    Pocket,
    PutIn,
    Throw,
    Inventory,
    Kit,
    Strip,
    Bind,
    Tool1,
    Tool2,
    Tool3,
    Tool4,
    ToolAway,
    Fasten,
    NextFastener,
    Unfasten,
    SetPost,
    StandOrLay,
    Turn,
    Fine,
    Slide,
    Map,
    WarpUp,
    WarpDown,
    Engine,
}

/// An action's row in the table.
#[allow(dead_code)] // Until the menu (G2.11 A3, item 6).
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub action: Action,
    /// Its name in the settings file (never changed once given: saved files use it).
    pub id: &'static str,
    /// What it does, in plain words, as the controls list shows it.
    pub name: &'static str,
    /// Its heading in the controls list.
    pub group: &'static str,
    pub context: Context,
    /// Whether it lasts while the key is held (else it happens when the key goes down).
    pub held: bool,
    /// Whether the player may not move it to another key (the menu's Esc; the wheel's).
    pub fixed: bool,
    /// Its keys or buttons unless the player chose others; the first is the one named.
    pub default: &'static [Input],
}

const fn row(
    action: Action,
    id: &'static str,
    name: &'static str,
    group: &'static str,
    context: Context,
    default: &'static [Input],
) -> Entry {
    Entry {
        action,
        id,
        name,
        group,
        context,
        held: false,
        fixed: false,
        default,
    }
}

/// The same, lasting while held.
const fn hold(entry: Entry) -> Entry {
    Entry {
        held: true,
        ..entry
    }
}

/// The same, not to be moved.
const fn fixed(entry: Entry) -> Entry {
    Entry {
        fixed: true,
        ..entry
    }
}

const MENUS: &str = "Menus and views";
const MOVING: &str = "Moving";
const HANDS: &str = "Hands";
const TOOLS: &str = "Tools";
const BUILDING: &str = "Building";
const HOLDING: &str = "Holding a part";
const SYSTEM: &str = "Solar system sandbox";

use Action as A;
use Context::{Always, Holding, Planet, System};
use Input::{Key, Mouse, Wheel};

/// Every action, in the order the controls list shows them. (Kept one row a line, as a table.)
#[rustfmt::skip]
pub const TABLE: &[Entry] = &[
    fixed(row(A::Menu, "menu", "Menu, controls and settings", MENUS, Always, &[Key(K::Escape)])),
    row(A::Debug, "debug", "Debug view: performance and numbers", MENUS, Always, &[Key(K::F3)]),
    row(A::Profiler, "profiler", "Profiler", MENUS, Always, &[Key(K::F9)]),
    row(A::Inventory, "inventory", "What you carry", MENUS, Planet, &[Key(K::KeyI)]),
    row(A::Kit, "kit", "Creative kit: parts to build with", MENUS, Planet, &[Key(K::KeyC)]),
    row(A::Camera, "camera", "First person or from behind", MENUS, Planet, &[Key(K::F5)]),
    row(A::WalkOrFly, "walk_or_fly", "Walk, or fly the camera free", MENUS, Planet, &[Key(K::KeyV)]),
    hold(row(A::Forward, "forward", "Forward", MOVING, Planet, &[Key(K::KeyW)])),
    hold(row(A::Back, "back", "Back", MOVING, Planet, &[Key(K::KeyS)])),
    hold(row(A::Left, "left", "Left", MOVING, Planet, &[Key(K::KeyA)])),
    hold(row(A::Right, "right", "Right", MOVING, Planet, &[Key(K::KeyD)])),
    hold(row(A::Jump, "jump", "Jump (flying: up)", MOVING, Planet, &[Key(K::Space)])),
    hold(row(A::Run, "run", "Run (flying: faster)", MOVING, Planet, &[Key(K::ShiftLeft)])),
    hold(row(A::Jetpack, "jetpack", "Jetpack (flying: down)", MOVING, Planet, &[Key(K::ControlLeft)])),
    hold(row(A::Look, "look", "Look about while the cursor is free", MOVING, Planet, &[Mouse(MouseButton::Right)])),
    fixed(row(A::FlySpeed, "fly_speed", "Flying speed", MOVING, Planet, &[Wheel])),
    row(A::Use, "use", "Strike or dig with what is in hand", HANDS, Planet, &[Mouse(MouseButton::Left)]),
    row(A::Take, "take", "Take in both hands, or put down", HANDS, Planet, &[Key(K::KeyG)]),
    row(A::Pocket, "pocket", "Put into a pocket", HANDS, Planet, &[Key(K::KeyE)]),
    row(A::PutIn, "put_in", "Put into a container, or look into it", HANDS, Planet, &[Key(K::KeyF)]),
    row(A::Throw, "throw", "Throw what the hands hold", HANDS, Planet, &[Key(K::KeyT)]),
    row(A::Strip, "strip", "Strip bark from the stick held", HANDS, Planet, &[Key(K::KeyX)]),
    row(A::Bind, "bind", "Bind the stone aimed at onto the stick held", HANDS, Planet, &[Key(K::KeyB)]),
    row(A::Tool1, "tool_1", "Tool 1 in hand (creative; again: away)", TOOLS, Planet, &[Key(K::Digit1)]),
    row(A::Tool2, "tool_2", "Tool 2 in hand", TOOLS, Planet, &[Key(K::Digit2)]),
    row(A::Tool3, "tool_3", "Tool 3 in hand", TOOLS, Planet, &[Key(K::Digit3)]),
    row(A::Tool4, "tool_4", "Tool 4 in hand", TOOLS, Planet, &[Key(K::Digit4)]),
    row(A::ToolAway, "tool_away", "Put the tool away", TOOLS, Planet, &[Key(K::Digit0)]),
    row(A::Fasten, "fasten", "Fasten the parts touching at the crosshair", BUILDING, Planet, &[Key(K::KeyJ)]),
    row(A::NextFastener, "next_fastener", "Choose another fastener", BUILDING, Planet, &[Key(K::KeyK)]),
    row(A::Unfasten, "unfasten", "Take apart the joint at the crosshair", BUILDING, Planet, &[Key(K::KeyU)]),
    row(A::SetPost, "set_post", "Set the post at the crosshair in its hole", BUILDING, Planet, &[Key(K::KeyP)]),
    row(A::StandOrLay, "stand_or_lay", "Stand it up, or lay it down", HOLDING, Holding, &[Key(K::KeyR)]),
    hold(row(A::Turn, "turn", "Turn it with the mouse", HOLDING, Holding, &[Key(K::KeyQ)])),
    hold(row(A::Fine, "fine", "Finer: turn, slide and look slower", HOLDING, Holding, &[Key(K::ControlLeft), Key(K::ControlRight)])),
    fixed(row(A::Slide, "slide", "Slide it along itself", HOLDING, Holding, &[Wheel])),
    row(A::Map, "map", "Map", SYSTEM, System, &[Key(K::KeyM)]),
    row(A::WarpUp, "warp_up", "Time faster", SYSTEM, System, &[Key(K::Period)]),
    row(A::WarpDown, "warp_down", "Time slower", SYSTEM, System, &[Key(K::Comma)]),
    row(A::Engine, "engine", "Ship's engine on or off", SYSTEM, System, &[Key(K::KeyB)]),
];

/// Keys two actions share on purpose by default: Ctrl is the jetpack, or finer while holding;
/// the wheel sets the flying speed, or slides what is held.
#[allow(dead_code)] // Until the menu (G2.11 A3, item 6).
pub const SHARED: &[(Action, Action)] = &[(A::Jetpack, A::Fine), (A::FlySpeed, A::Slide)];

/// `action`'s row.
///
/// # Panics
/// If an action has no row (a test checks every one has).
pub fn entry(action: Action) -> &'static Entry {
    TABLE
        .iter()
        .find(|e| e.action == action)
        .expect("every action has a row in the table")
}

/// Whether two contexts are ever active at once, so a key cannot serve both.
#[allow(dead_code)] // Until the menu (G2.11 A3, item 6).
pub fn clash(a: Context, b: Context) -> bool {
    match (a, b) {
        (Always, _) | (_, Always) => true,
        (Holding, Planet) | (Planet, Holding) => true,
        _ => a == b,
    }
}

/// The controls as the player has them: each action's keys, by its row in [`TABLE`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bindings {
    inputs: Vec<Vec<Input>>,
}

impl Default for Bindings {
    fn default() -> Self {
        Self {
            inputs: TABLE.iter().map(|e| e.default.to_vec()).collect(),
        }
    }
}

impl Bindings {
    fn index(action: Action) -> usize {
        TABLE.iter().position(|e| e.action == action).unwrap_or(0)
    }

    /// `action`'s keys (none: unbound).
    pub fn of(&self, action: Action) -> &[Input] {
        &self.inputs[Self::index(action)]
    }

    /// The name of `action`'s first key, as the screen shows it ("unbound" if it has none).
    pub fn name(&self, action: Action) -> String {
        self.of(action)
            .first()
            .map_or_else(|| "unbound".to_owned(), |i| input_name(*i))
    }

    /// The actions `input` does, in the table's order.
    pub fn actions(&self, input: Input) -> impl Iterator<Item = Action> + '_ {
        (TABLE.iter().zip(&self.inputs))
            .filter(move |(_, inputs)| inputs.contains(&input))
            .map(|(e, _)| e.action)
    }

    /// Binds `action` to `input` alone. Other actions that would clash on it lose it; they are
    /// returned. Refused (`None`) for a fixed action, the wheel, a key that may not be bound,
    /// or one a fixed action holds (Esc).
    #[allow(dead_code)] // Until the menu (G2.11 A3, item 6).
    pub fn bind(&mut self, action: Action, input: Input) -> Option<Vec<Action>> {
        let e = entry(action);
        let taken_by_fixed = (TABLE.iter().zip(&self.inputs))
            .any(|(o, i)| o.fixed && o.action != action && i.contains(&input));
        if e.fixed || input == Wheel || !bindable(input) || taken_by_fixed {
            return None;
        }
        let mut lost = Vec::new();
        for (other, inputs) in TABLE.iter().zip(&mut self.inputs) {
            if other.action != action && clash(e.context, other.context) && inputs.contains(&input)
            {
                inputs.retain(|i| *i != input);
                lost.push(other.action);
            }
        }
        self.inputs[Self::index(action)] = vec![input];
        Some(lost)
    }

    /// The actions that clash with `action` on one of its keys (none, unless the player made
    /// them; the defaults' [`SHARED`] keys excepted).
    #[allow(dead_code)] // Until the menu (G2.11 A3, item 6).
    pub fn clashes(&self, action: Action) -> Vec<Action> {
        let e = entry(action);
        let shared = |a, b| SHARED.contains(&(a, b)) || SHARED.contains(&(b, a));
        (TABLE.iter().zip(&self.inputs))
            .filter(|(o, i)| {
                o.action != action
                    && clash(e.context, o.context)
                    && !shared(action, o.action)
                    && i.iter().any(|x| self.of(action).contains(x))
            })
            .map(|(o, _)| o.action)
            .collect()
    }

    /// The player's changes from the defaults, as the settings file keeps them (by id).
    pub fn changes(&self) -> BTreeMap<String, Vec<String>> {
        (TABLE.iter().zip(&self.inputs))
            .filter(|(e, i)| e.default != i.as_slice())
            .map(|(e, i)| (e.id.to_owned(), i.iter().map(|x| input_id(*x)).collect()))
            .collect()
    }

    /// The defaults with `changes` made (as [`Self::changes`] gives them). An unknown action or
    /// key is left out and named in the second value, so a settings file of another version
    /// still loads.
    pub fn with_changes(changes: &BTreeMap<String, Vec<String>>) -> (Self, Vec<String>) {
        let mut bindings = Self::default();
        let mut unknown = Vec::new();
        for (id, inputs) in changes {
            let Some(k) = TABLE.iter().position(|e| e.id == id && !e.fixed) else {
                unknown.push(id.clone());
                continue;
            };
            let mut parsed = Vec::new();
            for s in inputs {
                match parse_input(s) {
                    Some(i) => parsed.push(i),
                    None => unknown.push(s.clone()),
                }
            }
            bindings.inputs[k] = parsed;
        }
        (bindings, unknown)
    }
}

#[cfg(test)]
#[path = "bindings_tests.rs"]
mod tests;
