use super::*;

fn walking() -> Seen {
    Seen {
        walking: true,
        fastener: Some("bast lashing".into()),
        lifts: (58.6, 117.2),
        ..Seen::default()
    }
}

fn actions(seen: &Seen) -> Vec<Action> {
    prompts(seen).1.into_iter().map(|p| p.action).collect()
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
    let (state, prompts) = prompts(&seen);
    assert!(state.is_empty());
    let actions: Vec<Action> = prompts.iter().map(|p| p.action).collect();
    assert_eq!(
        actions,
        [Action::Take, Action::Fasten, Action::NextFastener]
    );
    assert_eq!(prompts[0].verb, "Take");
    assert_eq!(prompts[1].verb, "Fasten (bast lashing)");
}

#[test]
fn out_of_reach_it_is_named_too_far_and_nothing_is_offered() {
    let seen = Seen {
        aimed: Some(pole(false, 27.0)),
        ..walking()
    };
    let (state, prompts) = prompts(&seen);
    assert_eq!(state, ["too far"]);
    assert!(prompts.is_empty());
}

#[test]
fn a_thing_too_heavy_is_not_offered_to_be_taken_and_its_weight_is_told() {
    let heavy = Seen {
        aimed: Some(pole(true, 200.0)),
        ..walking()
    };
    let (state, _) = prompts(&heavy);
    assert_eq!(state, ["too heavy (200 kg)"]);
    assert!(!actions(&heavy).contains(&Action::Take));
    let one_end = Seen {
        aimed: Some(pole(true, 70.0)),
        ..walking()
    };
    let (state, prompts) = prompts(&one_end);
    assert_eq!(state, ["heavy (70 kg)"]);
    assert_eq!(prompts[0].verb, "Lift one end");
}

#[test]
fn holding_a_part_offers_the_holding_keys_and_fastening_where_it_touches() {
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
    let a = actions(&seen);
    assert_eq!(a[0], Action::Take, "put down first");
    for wanted in [
        Action::Fasten,
        Action::StandOrLay,
        Action::Turn,
        Action::Slide,
        Action::SetPost,
        Action::Throw,
        Action::Fine,
    ] {
        assert!(a.contains(&wanted), "{wanted:?} in {a:?}");
    }
    assert!(
        !a.contains(&Action::Use),
        "what is held is not struck with itself"
    );
    assert!(
        !a.contains(&Action::Pocket),
        "a part does not go in a pocket"
    );
}

#[test]
fn a_stick_in_hand_strips_bark_and_binds_a_stone_aimed_at() {
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
    let a = actions(&seen);
    assert_eq!(a[0], Action::Use, "strike it");
    assert!(a.contains(&Action::Strip) && a.contains(&Action::Bind) && a.contains(&Action::Pocket));
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
    assert_eq!(prompts(&bare).1[0].verb, "Dig with your hands");
    let armed = Seen { tool: true, ..bare };
    assert_eq!(prompts(&armed).1[0].verb, "Dig");
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
    let a = actions(&seen);
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
    let (_, prompts) = prompts(&seen);
    assert_eq!(prompts[0].verb, "dig at the crosshair");
    assert_eq!(prompts[1].action, Action::WalkOrFly);
}
