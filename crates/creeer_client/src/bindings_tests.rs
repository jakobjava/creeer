use super::*;
use winit::keyboard::KeyCode;

/// Every action, listed by a match that stops compiling when one is added and not listed here.
fn all() -> Vec<Action> {
    fn listed(a: Action) {
        match a {
            A::Menu | A::Debug | A::Profiler | A::Forward | A::Back | A::Left | A::Right => {}
            A::Jump | A::Run | A::Jetpack | A::Look | A::FlySpeed | A::WalkOrFly | A::Camera => {}
            A::Use | A::Take | A::Pocket | A::PutIn | A::Throw | A::Inventory | A::Kit => {}
            A::Strip | A::Bind | A::Tool1 | A::Tool2 | A::Tool3 | A::Tool4 | A::ToolAway => {}
            A::Fasten | A::NextFastener | A::Unfasten | A::SetPost | A::StandOrLay => {}
            A::Turn | A::Fine | A::Slide | A::Map | A::WarpUp | A::WarpDown | A::Engine => {}
        }
    }
    let all = vec![
        A::Menu,
        A::Debug,
        A::Profiler,
        A::Forward,
        A::Back,
        A::Left,
        A::Right,
        A::Jump,
        A::Run,
        A::Jetpack,
        A::Look,
        A::FlySpeed,
        A::WalkOrFly,
        A::Camera,
        A::Use,
        A::Take,
        A::Pocket,
        A::PutIn,
        A::Throw,
        A::Inventory,
        A::Kit,
        A::Strip,
        A::Bind,
        A::Tool1,
        A::Tool2,
        A::Tool3,
        A::Tool4,
        A::ToolAway,
        A::Fasten,
        A::NextFastener,
        A::Unfasten,
        A::SetPost,
        A::StandOrLay,
        A::Turn,
        A::Fine,
        A::Slide,
        A::Map,
        A::WarpUp,
        A::WarpDown,
        A::Engine,
    ];
    all.iter().for_each(|a| listed(*a));
    all
}

#[test]
fn every_action_has_one_row_with_a_unique_id_a_name_and_bindable_keys() {
    let all = all();
    assert_eq!(TABLE.len(), all.len(), "one row per action");
    for a in &all {
        assert_eq!(TABLE.iter().filter(|e| e.action == *a).count(), 1, "{a:?}");
        let e = entry(*a);
        assert!(!e.name.is_empty() && !e.group.is_empty(), "{a:?}");
        assert!(!e.default.is_empty(), "{a:?} has a key by default");
        assert!(e.default.iter().all(|i| bindable(*i)), "{a:?}");
        assert_eq!(TABLE.iter().filter(|o| o.id == e.id).count(), 1, "{}", e.id);
    }
}

#[test]
fn the_defaults_clash_nowhere_but_where_they_share_on_purpose() {
    let b = Bindings::default();
    for e in TABLE {
        assert!(
            b.clashes(e.action).is_empty(),
            "{:?}: {:?}",
            e.action,
            b.clashes(e.action)
        );
    }
    // Ctrl: the jetpack, finer while holding.
    let ctrl = Input::Key(KeyCode::ControlLeft);
    let on_ctrl: Vec<Action> = b.actions(ctrl).collect();
    assert_eq!(on_ctrl, vec![A::Jetpack, A::Fine]);
}

#[test]
fn a_key_bound_to_another_action_leaves_the_one_that_clashed() {
    let mut b = Bindings::default();
    let e = Input::Key(KeyCode::KeyE);
    let lost = b.bind(A::Take, e).expect("bound");
    assert_eq!(lost, vec![A::Pocket], "pocketing loses E");
    assert_eq!(b.of(A::Take), &[e]);
    assert!(b.of(A::Pocket).is_empty());
    assert_eq!(b.name(A::Pocket), "unbound");
    assert_eq!(b.name(A::Take), "E");
    assert!(
        b.actions(Input::Key(KeyCode::KeyG)).next().is_none(),
        "G is free now"
    );
}

#[test]
fn a_key_may_serve_two_actions_that_are_never_wanted_at_once() {
    let mut b = Bindings::default();
    // The solar system's map on G: G still takes on the planet.
    let g = Input::Key(KeyCode::KeyG);
    assert_eq!(b.bind(A::Map, g), Some(vec![]));
    assert_eq!(b.actions(g).collect::<Vec<_>>(), vec![A::Take, A::Map]);
    // But a holding key on G would hide taking and putting down while holding.
    assert_eq!(b.bind(A::StandOrLay, g), Some(vec![A::Take]));
}

#[test]
fn esc_the_wheel_and_fixed_actions_are_not_moved() {
    let mut b = Bindings::default();
    assert_eq!(
        b.bind(A::Take, Input::Key(KeyCode::Escape)),
        None,
        "Esc opens the menu"
    );
    assert_eq!(b.bind(A::Take, Input::Wheel), None);
    assert_eq!(b.bind(A::Menu, Input::Key(KeyCode::KeyM)), None);
    assert_eq!(b.bind(A::Slide, Input::Key(KeyCode::KeyM)), None);
    assert_eq!(
        b.bind(A::Take, Input::Key(KeyCode::NumLock)),
        None,
        "not in the list"
    );
    assert_eq!(b, Bindings::default());
}

#[test]
fn the_players_changes_are_kept_by_id_and_come_back_the_same() {
    let mut b = Bindings::default();
    b.bind(A::Fasten, Input::Key(KeyCode::KeyN));
    b.bind(A::Use, Input::Mouse(MouseButton::Middle));
    let changes = b.changes();
    assert_eq!(changes.len(), 2, "{changes:?}");
    assert_eq!(changes["fasten"], vec!["KeyN".to_owned()]);
    assert_eq!(changes["use"], vec!["MouseMiddle".to_owned()]);
    let (back, unknown) = Bindings::with_changes(&changes);
    assert_eq!(back, b);
    assert!(unknown.is_empty());
    assert!(Bindings::default().changes().is_empty());
}

#[test]
fn a_settings_file_with_unknown_actions_or_keys_still_loads() {
    let mut changes = BTreeMap::new();
    changes.insert("hover".to_owned(), vec!["KeyH".to_owned()]);
    changes.insert(
        "take".to_owned(),
        vec!["KeyH".to_owned(), "Hyperkey".to_owned()],
    );
    changes.insert("menu".to_owned(), vec!["KeyM".to_owned()]);
    let (b, unknown) = Bindings::with_changes(&changes);
    assert_eq!(b.of(A::Take), &[Input::Key(KeyCode::KeyH)]);
    assert_eq!(
        b.of(A::Menu),
        &[Input::Key(KeyCode::Escape)],
        "fixed: not changed"
    );
    assert_eq!(unknown.len(), 3, "{unknown:?}");
}

#[test]
fn names_and_ids_of_keys_and_buttons() {
    assert_eq!(input_name(Input::Key(KeyCode::ControlLeft)), "Ctrl");
    assert_eq!(input_name(Input::Mouse(MouseButton::Left)), "Left click");
    assert_eq!(input_name(Input::Wheel), "Wheel");
    for e in TABLE {
        for i in e.default {
            assert_eq!(parse_input(&input_id(*i)), Some(*i), "{i:?}");
        }
    }
    assert_eq!(parse_input("KeyZ"), Some(Input::Key(KeyCode::KeyZ)));
    assert_eq!(parse_input("Hyperkey"), None);
}
