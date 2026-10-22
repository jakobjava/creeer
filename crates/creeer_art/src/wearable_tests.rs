use crate::model::Model;
use crate::model_skin::{Part, inside, parse_part};
use crate::wearable::{Wearable, drawn, problems};

fn part(name: &str, triangles: usize) -> Part {
    let (piece, region, level) = parse_part(name).expect("a part's name");
    Part {
        piece,
        region,
        level,
        first: 0,
        count: 3 * triangles,
    }
}

fn wearable(name: &str, hides: &[(&str, &[&str])]) -> Wearable {
    Wearable {
        name: name.into(),
        hides: (hides.iter())
            .map(|(piece, regions)| {
                (
                    (*piece).into(),
                    regions.iter().map(|r| (*r).into()).collect(),
                )
            })
            .collect(),
    }
}

fn model(parts: Vec<Part>) -> Model {
    Model {
        vertices: Vec::new(),
        indices: Vec::new(),
        bones: Vec::new(),
        parts,
    }
}

#[test]
fn a_region_inside_names_its_wearable() {
    assert_eq!(inside("in.jacket"), Some("jacket"));
    assert_eq!(inside("in."), None);
    assert_eq!(inside("torso"), None);
    assert!(parse_part("coverall:in.jacket@0").is_ok());
    let error = parse_part("coverall:in.@0").expect_err("no wearable");
    assert!(error.contains("no region"), "{error}");
}

#[test]
fn what_lies_inside_a_wearable_is_hidden_while_it_is_worn() {
    // The coverall's sleeves (its forearms) are drawn, but where the jacket's enclose them.
    let model = model(vec![
        part("body:torso@0", 100),
        part("coverall:forearm.L@0", 10),
        part("coverall:in.jacket@0", 20),
        part("jacket:torso@0", 40),
    ]);
    let wearables = [
        wearable("coverall", &[("body", &["torso"])]),
        wearable("jacket", &[]),
    ];
    assert_eq!(drawn(&model, &wearables, &["coverall"])[0], 30);
    assert_eq!(drawn(&model, &wearables, &["coverall", "jacket"])[0], 50);
    assert!(problems(&model, &wearables).is_empty());
}

#[test]
fn a_part_inside_no_wearable_is_a_problem() {
    let model = model(vec![
        part("coverall:torso@0", 10),
        part("coverall:in.cape@0", 10),
    ]);
    let found = problems(&model, &[wearable("coverall", &[])]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("in.cape lies inside no wearable"),
        "{found:?}"
    );
}
