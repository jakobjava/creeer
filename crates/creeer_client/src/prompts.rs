//! What the player can do right now, with what the hands hold and what the crosshair is on
//! (G2.11 A3, ADR-0037): the prompts beside the crosshair, each an action of the controls' table
//! (`bindings.rs`) with a verb, its key named from the table when drawn. Only what a person on
//! site would know: what a thing is, that it is too far, that it is too heavy (and its weight,
//! once felt). No other numbers (plan.md, principle 13).
//!
//! TO ADD A PROMPT: a line in [`prompts`] under the case it applies to, with its action and verb.

use crate::bindings::Action;

/// What the crosshair is on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum What {
    /// The ground.
    #[default]
    Ground,
    /// A standing tree's trunk.
    Tree,
    /// A thing: a body, a part, a stone or a stick.
    Thing,
}

/// What the crosshair is on, as the prompts need it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Aimed {
    pub what: What,
    /// Within the hands' reach.
    pub in_reach: bool,
    /// Its mass (kg), if it is a thing.
    pub mass: Option<f64>,
    /// What the hands hold.
    pub in_hands: bool,
    /// A part to build with (timber: fastened, set in the ground), and whether it stands.
    pub part: bool,
    pub standing: bool,
    /// Fastened to something: taken apart with U.
    pub built: bool,
    /// Small enough for a pocket with room left.
    pub pocketable: bool,
    /// A container one looks into.
    pub container: bool,
    /// A stone (a tool's head).
    pub stone: bool,
}

/// What the player has and sees.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Seen {
    /// Walking (else the camera flies free, and a click is the sandbox tool's).
    pub walking: bool,
    /// The word for what the hands hold, if anything.
    pub hands: Option<String>,
    /// Whether what the hands hold is steered by the mouse (the holding keys apply), and
    /// whether it is a part to build with (too big for a pocket).
    pub holding: bool,
    pub hands_part: bool,
    /// A tool in hand (1 to 4).
    pub tool: bool,
    /// What the crosshair is on, near enough to be named.
    pub aimed: Option<Aimed>,
    /// The fastener chosen.
    pub fastener: Option<String>,
    /// Flying: the sandbox tool a click uses.
    pub fly_tool: Option<String>,
    /// The most a person lifts (kg), and what they can raise one end of, here.
    pub lifts: (f64, f64),
}

/// A prompt: an action and what it does now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prompt {
    pub action: Action,
    pub verb: String,
}

/// What can be told of the aim (out of reach, too heavy) and what can be done, in order of how
/// likely it is wanted.
pub fn prompts(seen: &Seen) -> (Vec<String>, Vec<Prompt>) {
    let mut state = Vec::new();
    let mut out = Vec::new();
    let mut add = |action: Action, verb: &str| {
        out.push(Prompt {
            action,
            verb: verb.to_owned(),
        });
    };
    if !seen.walking {
        if let Some(tool) = &seen.fly_tool {
            add(Action::Use, &format!("{tool} at the crosshair"));
        }
        add(Action::WalkOrFly, "Walk");
        return (state, out);
    }
    let aimed = seen.aimed.filter(|a| a.in_reach);
    if seen
        .aimed
        .is_some_and(|a| !a.in_reach && a.what != What::Ground)
    {
        state.push("too far".to_owned());
    }
    let fastener = seen.fastener.as_deref().unwrap_or("nothing chosen");
    let fasten = format!("Fasten ({fastener})");
    let armed = seen.tool || seen.hands.is_some();
    // A click: strike with what is in hand, or dig.
    match aimed.map(|a| a.what) {
        Some(What::Thing | What::Tree) if armed && !aimed.is_some_and(|a| a.in_hands) => {
            add(Action::Use, "Strike");
        }
        Some(What::Ground) if armed => add(Action::Use, "Dig"),
        Some(What::Ground) => add(Action::Use, "Dig with your hands"),
        _ => {}
    }
    match &seen.hands {
        Some(word) => {
            add(Action::Take, "Put down");
            let on_part = aimed.is_some_and(|a| a.part || a.what == What::Tree);
            if on_part {
                add(Action::Fasten, &fasten);
            }
            if seen.holding {
                add(Action::StandOrLay, "Stand up or lay down");
                add(Action::Turn, "Turn (hold, move the mouse)");
                add(Action::Slide, "Slide along itself");
            }
            if word == "stick" {
                add(Action::Strip, "Strip bark");
                if aimed.is_some_and(|a| a.stone) {
                    add(Action::Bind, "Bind on as a head");
                }
            }
            if aimed.is_some_and(|a| a.container) {
                add(Action::PutIn, "Put inside");
            }
            if seen.hands_part {
                add(Action::SetPost, "Set it in the ground");
            } else {
                add(Action::Pocket, "Put in a pocket");
            }
            add(Action::Throw, "Throw");
            if seen.holding {
                add(Action::Fine, "Finer (hold)");
            }
            if on_part {
                add(Action::NextFastener, "Another fastener");
            }
        }
        None => {
            if let Some(a) = aimed.filter(|a| a.what == What::Thing) {
                match a.mass {
                    Some(m) if m > seen.lifts.1 => state.push(format!("too heavy ({m:.0} kg)")),
                    Some(m) if m > seen.lifts.0 => {
                        state.push(format!("heavy ({m:.0} kg)"));
                        add(Action::Take, "Lift one end");
                    }
                    _ => add(Action::Take, "Take"),
                }
                if a.pocketable {
                    add(Action::Pocket, "Put in a pocket");
                }
                if a.container {
                    add(Action::PutIn, "Look inside");
                }
                if a.part {
                    add(Action::Fasten, &fasten);
                }
                if a.part && a.standing {
                    add(Action::SetPost, "Set it in the ground");
                }
                if a.built {
                    add(Action::Unfasten, "Take apart");
                }
                if a.part {
                    add(Action::NextFastener, "Another fastener");
                }
            }
        }
    }
    (state, out)
}

#[cfg(test)]
#[path = "prompts_tests.rs"]
mod tests;
