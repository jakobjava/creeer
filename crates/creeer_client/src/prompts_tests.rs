use super::*;

fn walking() -> Seen {
    Seen {
        walking: true,
        fastener: Some("bast lashing".into()),
        lifts: (58.6, 117.2),
        ..Seen::default()
    }
}

fn actions(prompts: &[Prompt]) -> Vec<Action> {
    prompts.iter().map(|p| p.action).collect()
}

fn pole(in_reach: bool, mass: f64) -> Aimed {
    Aimed {
        what: What::Thing,
        in_reach,
        mass: Some(mass),
        part: true,
        ..Aimed::default()
    }
}

#[test]
fn a_part_in_reach_with_empty_hands_is_taken_or_fastened() {
    let seen = Seen {
        aimed: Some(pole(true, 27.0)),
        ..walking()
    };
    let p = prompts(&seen);
    assert!(p.state.is_empty() && p.hands.is_empty());
    assert_eq!(
        actions(&p.aimed),
        [Action::Take, Action::Fasten, Action::NextFastener]
    );
    assert_eq!(p.aimed[0].verb, "Take");
    assert_eq!(p.aimed[1].verb, "Fasten (bast lashing)");
}

#[test]
fn out_of_reach_it_is_named_too_far_and_nothing_is_offered() {
    let seen = Seen {
        aimed: Some(pole(false, 27.0)),
        ..walking()
    };
    let p = prompts(&seen);
    assert_eq!(p.state, ["too far"]);
    assert!(p.aimed.is_empty() && p.hands.is_empty());
}

#[test]
fn a_thing_too_heavy_is_not_offered_to_be_taken_and_its_weight_is_told() {
    let heavy = Seen {
        aimed: Some(pole(true, 200.0)),
        ..walking()
    };
    let p = prompts(&heavy);
    assert_eq!(p.state, ["too heavy (200 kg)"]);
    assert!(!actions(&p.aimed).contains(&Action::Take));
    let one_end = Seen {
        aimed: Some(pole(true, 70.0)),
        ..walking()
    };
    let p = prompts(&one_end);
    assert_eq!(p.state, ["heavy (70 kg)"]);
    assert_eq!(p.aimed[0].verb, "Lift one end");
}

#[test]
fn holding_a_part_offers_the_holding_keys_by_the_hands_and_fastening_at_the_crosshair() {
    let seen = Seen {
        hands: Some("stick".into()),
        holding: true,
        hands_part: true,
        aimed: Some(Aimed {
            in_hands: true,
            ..pole(true, 10.0)
        }),
        ..walking()
    };
    let p = prompts(&seen);
    assert_eq!(
        actions(&p.aimed),
        [Action::Fasten, Action::NextFastener],
        "where it touches"
    );
    assert_eq!(
        actions(&p.hands),
        [
            Action::Take,
            Action::StandOrLay,
            Action::Turn,
            Action::Slide,
            Action::SetPost,
            Action::Throw,
            Action::Fine
        ]
    );
    assert_eq!(p.hands[0].verb, "Put down");
}

#[test]
fn a_stick_in_hand_strikes_strips_bark_and_binds_a_stone_aimed_at() {
    let stone = Aimed {
        what: What::Thing,
        in_reach: true,
        mass: Some(0.7),
        stone: true,
        pocketable: true,
        ..Aimed::default()
    };
    let seen = Seen {
        hands: Some("stick".into()),
        aimed: Some(stone),
        ..walking()
    };
    let p = prompts(&seen);
    assert_eq!(actions(&p.aimed), [Action::Use, Action::Bind]);
    let hands = actions(&p.hands);
    assert!(hands.contains(&Action::Strip) && hands.contains(&Action::Pocket));
}

#[test]
fn the_ground_is_dug_with_bare_hands_or_a_tool() {
    let ground = Aimed {
        in_reach: true,
        ..Aimed::default()
    };
    let bare = Seen {
        aimed: Some(ground),
        ..walking()
    };
    assert_eq!(prompts(&bare).aimed[0].verb, "Dig with your hands");
    let armed = Seen { tool: true, ..bare };
    assert_eq!(prompts(&armed).aimed[0].verb, "Dig");
}

#[test]
fn a_standing_post_is_set_and_a_built_part_taken_apart() {
    let post = Aimed {
        standing: true,
        built: true,
        ..pole(true, 10.0)
    };
    let seen = Seen {
        aimed: Some(post),
        ..walking()
    };
    let a = actions(&prompts(&seen).aimed);
    assert!(
        a.contains(&Action::SetPost) && a.contains(&Action::Unfasten),
        "{a:?}"
    );
}

#[test]
fn flying_a_click_is_the_sandbox_tools() {
    let seen = Seen {
        fly_tool: Some("dig".into()),
        ..Seen::default()
    };
    let p = prompts(&seen);
    assert_eq!(p.aimed[0].verb, "dig at the crosshair");
    assert_eq!(p.hands[0].action, Action::WalkOrFly);
}
