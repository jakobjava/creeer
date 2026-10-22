use crate::model::Model;
use crate::model_skin::{Part, inside, parse_part};
use crate::wearable::{Wearable, drawn, problems, visible};

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

/// What an outfit draws (G1.3e): exactly the regions left visible, of the body and every worn
/// piece, what is hidden or inside a worn wearable left out, and nothing of what is not worn.
#[test]
fn an_outfit_draws_exactly_its_visible_regions() {
    let model = model(vec![
        part("body:head@0", 1),
        part("body:torso@0", 1),
        part("body:hand.L@0", 1),
        part("body:torso@1", 1),
        part("vest:torso@0", 1),
        part("vest:in.coat@0", 1),
        part("coat:torso@0", 1),
        part("coat:forearm.L@0", 1),
        part("gloves:hand.L@0", 1),
    ]);
    let wearables = [
        wearable("vest", &[("body", &["torso"])]),
        wearable("coat", &[("vest", &["torso"])]),
        wearable("gloves", &[("body", &["hand.L"])]),
    ];
    let names = |worn: &[&str]| -> Vec<String> {
        (visible(&model, &wearables, worn).iter())
            .map(|p| format!("{}:{}@{}", p.piece, p.region, p.level))
            .collect()
    };
    assert_eq!(
        names(&[]),
        [
            "body:head@0",
            "body:torso@0",
            "body:hand.L@0",
            "body:torso@1"
        ]
    );
    assert_eq!(
        names(&["vest"]),
        [
            "body:head@0",
            "body:hand.L@0",
            "vest:torso@0",
            "vest:in.coat@0"
        ]
    );
    assert_eq!(
        names(&["vest", "coat", "gloves"]),
        [
            "body:head@0",
            "coat:torso@0",
            "coat:forearm.L@0",
            "gloves:hand.L@0"
        ]
    );
}
