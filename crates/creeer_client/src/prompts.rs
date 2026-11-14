//! What the player can do right now, with what the hands hold and what the crosshair is on
//! (G2.11 A3, ADR-0038): the prompts beside the crosshair, each an action of the controls' table
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
    /// Whether it is kept still where it is (a nail was driven into it while held: G lets go).
    pub pinned: bool,
    pub hands_part: bool,
    /// Whether what the hands hold is small enough for a pocket (a 25 kg block is not).
    pub hands_pocketable: bool,
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

/// The prompts: what is told of the aim (too far, too heavy), what can be done with what the
/// crosshair is on (shown beside it), and with what the hands hold (shown at the bottom, near
/// the hands), each in order of how likely it is wanted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Prompts {
    pub state: Vec<String>,
    pub aimed: Vec<Prompt>,
    pub hands: Vec<Prompt>,
}

fn prompt(action: Action, verb: &str) -> Prompt {
    Prompt {
        action,
        verb: verb.to_owned(),
    }
}

/// What the player can do now.
pub fn prompts(seen: &Seen) -> Prompts {
    let mut out = Prompts::default();
    if !seen.walking {
        if let Some(tool) = &seen.fly_tool {
            out.aimed
                .push(prompt(Action::Use, &format!("{tool} at the crosshair")));
        }
        out.hands.push(prompt(Action::WalkOrFly, "Walk"));
        return out;
    }
    if seen
        .aimed
        .is_some_and(|a| !a.in_reach && a.what != What::Ground)
    {
        out.state.push("too far".to_owned());
    }
    if let Some(a) = seen.aimed.filter(|a| a.in_reach) {
        aimed(seen, a, &mut out);
    }
    if let Some(word) = &seen.hands {
        let mut add = |action, verb| out.hands.push(prompt(action, verb));
        if seen.pinned {
            add(Action::Take, "Let go: the nails hold it");
        } else {
            add(Action::Take, "Put down");
        }
        if seen.holding && !seen.pinned {
            add(Action::StandOrLay, "Stand up or lay down");
            add(Action::Turn, "Turn (hold, move the mouse)");
            add(Action::Slide, "Slide along itself");
        }
        if seen.hands_part {
            add(Action::SetPost, "Set it in the ground");
        } else if seen.hands_pocketable {
            add(Action::Pocket, "Put in a pocket");
        }
        if word == "stick" && !seen.hands_part {
            add(Action::Strip, "Strip bark");
        }
        add(Action::Throw, "Throw");
        if seen.holding {
            add(Action::Fine, "Finer (hold)");
        }
    }
    out
}

/// What can be done with `a`, within reach.
fn aimed(seen: &Seen, a: Aimed, out: &mut Prompts) {
    // (An empty name is the hand's own choice: the shortest nail that holds, ADR-0036.)
    let fastener = match seen.fastener.as_deref() {
        Some("") => "a nail at hand",
        Some(f) => f,
        None => "nothing chosen",
    };
    let fasten = format!("Fasten ({fastener})");
    // (A part to build with is laid and fastened, not swung: G2.11 Q, a stringer carried to the
    // gully offered to dig the ground with it.)
    let armed = seen.tool || (seen.hands.is_some() && !seen.hands_part);
    let empty = seen.hands.is_none();
    let mut add = |action, verb: &str| out.aimed.push(prompt(action, verb));
    // A click: strike with what is in hand, or dig.
    match a.what {
        What::Thing | What::Tree if armed && !a.in_hands => add(Action::Use, "Strike"),
        What::Ground if armed => add(Action::Use, "Dig"),
        What::Ground if empty => add(Action::Use, "Dig with your hands"),
        _ => {}
    }
    if a.what == What::Thing && empty {
        match a.mass {
            Some(m) if m > seen.lifts.1 => out.state.push(format!("too heavy ({m:.0} kg)")),
            Some(m) if m > seen.lifts.0 => {
                out.state.push(format!("heavy ({m:.0} kg)"));
                add(Action::Take, "Lift one end");
            }
            _ => add(Action::Take, "Take"),
        }
        if a.pocketable {
            add(Action::Pocket, "Put in a pocket");
        }
    }
    if a.container {
        add(
            Action::PutIn,
            if empty { "Look inside" } else { "Put inside" },
        );
    }
    if a.stone && seen.hands.as_deref() == Some("stick") {
        add(Action::Bind, "Bind on as a head");
    }
    // Fastening where parts touch (a part held against another, or to a tree's trunk).
    let fastens = a.part || (a.what == What::Tree && seen.holding);
    if fastens {
        add(Action::Fasten, &fasten);
    }
    if a.part && a.standing && empty {
        add(Action::SetPost, "Set it in the ground");
    }
    if a.built && empty {
        add(Action::Unfasten, "Take apart");
    }
    if fastens {
        add(Action::NextFastener, "Another fastener");
    }
}

#[cfg(test)]
#[path = "prompts_tests.rs"]
mod tests;
