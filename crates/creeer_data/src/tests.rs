use crate::{Data, DataError, Location, Pack, Watcher};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// A fresh directory under the system's temporary one.
fn scratch(name: &str) -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let k = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("creeer_data_{}_{k}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

fn write(root: &Path, path: &str, text: &str) {
    let file = root.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

fn pack(root: &Path, name: &str, extra: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = root.join(name);
    write(
        &dir,
        "pack.ron",
        &format!("Pack(name: {name:?}, version: \"1\", {extra})"),
    );
    for (path, text) in files {
        write(&dir, path, text);
    }
    dir
}

#[derive(Debug, Deserialize, PartialEq)]
struct Thing {
    name: String,
    value: f64,
}

fn things(data: &Data) -> Result<Vec<(String, f64)>, DataError> {
    let table = data.table("things", "things/*.ron");
    let parsed = table.merge(
        |text| ron::from_str::<Vec<Thing>>(text).map_err(|e| e.to_string()),
        |t| &t.name,
    )?;
    Ok(parsed.into_iter().map(|t| (t.name, t.value)).collect())
}

const BASE_THINGS: &str =
    r#"[(name: "a", value: 1.0), (name: "b", value: 2.0), (name: "c", value: 3.0)]"#;

#[test]
fn mods_replace_add_and_remove_entries_in_load_order() {
    let root = scratch("merge");
    let base = pack(
        &root,
        "base",
        "",
        &[("things/core.ron", BASE_THINGS), ("worlds/w.ron", "1")],
    );
    let mods = root.join("mods");
    // `zeta` loads before `alpha` because alpha requires it: order follows requirements.
    pack(
        &mods,
        "alpha",
        r#"requires: ["zeta"], remove: { "things": ["c"] }"#,
        &[
            (
                "things/more.ron",
                r#"[(name: "b", value: 20.0), (name: "d", value: 4.0)]"#,
            ),
            ("worlds/w.ron", "2"),
        ],
    );
    pack(
        &mods,
        "zeta",
        "",
        &[(
            "things/z.ron",
            r#"[(name: "b", value: 200.0), (name: "e", value: 5.0)]"#,
        )],
    );
    let data = Data::open(&base, &[mods]).unwrap();
    let names: Vec<&str> = data.packs().iter().map(Pack::name).collect();
    assert_eq!(names, ["base", "zeta", "alpha"]);
    // b replaced twice (in place), c removed, e and d added in load order.
    assert_eq!(
        things(&data).unwrap(),
        [
            ("a".into(), 1.0),
            ("b".into(), 20.0),
            ("e".into(), 5.0),
            ("d".into(), 4.0)
        ]
    );
    assert_eq!(data.file("worlds/w.ron").unwrap(), "2");
    assert_eq!(data.origin("worlds/w.ron"), Some("alpha"));
    assert_eq!(
        data.list("things/"),
        ["things/core.ron", "things/more.ron", "things/z.ron"]
    );
}

#[test]
fn broken_packs_are_refused_with_their_origin() {
    let root = scratch("broken");
    let base = pack(&root, "base", "", &[("things/core.ron", BASE_THINGS)]);
    let case = |name: &str, extra: &str, files: &[(&str, &str)]| {
        let mods = root.join(format!("mods_{name}"));
        pack(&mods, name, extra, files);
        Data::open(&base, &[mods]).and_then(|d| things(&d).map(|_| ()))
    };
    let missing = case("lonely", r#"requires: ["nowhere"]"#, &[]).unwrap_err();
    assert!(missing.message.contains("not there"), "{missing}");
    let removes = case("typo", r#"remove: { "things": ["zz"] }"#, &[]).unwrap_err();
    assert_eq!(removes.origin, "typo");
    let twice = case(
        "twice",
        "",
        &[
            ("things/x.ron", r#"[(name: "q", value: 1.0)]"#),
            ("things/y.ron", r#"[(name: "q", value: 2.0)]"#),
        ],
    )
    .unwrap_err();
    assert!(twice.origin.contains("things/y.ron"), "{twice}");
    let bad = case("bad", "", &[("things/x.ron", r#"[(name: "q")]"#)]).unwrap_err();
    assert!(bad.origin == "bad: things/x.ron", "{bad}");
    let name = case("Bad Name", "", &[]).unwrap_err();
    assert!(name.message.contains("lower-case"), "{name}");
    // A circle: each requires the other.
    let mods = root.join("mods_circle");
    pack(&mods, "p", r#"requires: ["q"]"#, &[]);
    pack(&mods, "q", r#"requires: ["p"]"#, &[]);
    let circle = Data::open(&base, &[mods]).unwrap_err();
    assert!(circle.message.contains("circle"), "{circle}");
}

#[test]
fn the_hash_follows_content_not_line_ends_or_listing() {
    let root = scratch("hash");
    let a = pack(&root, "base", "", &[("x.ron", "1\n2\n")]);
    let b = pack(&root.join("other"), "base", "", &[("x.ron", "1\r\n2\r\n")]);
    let c = pack(&root.join("third"), "base", "", &[("x.ron", "1\n3\n")]);
    let hash = |p: &Path| Data::open(p, &[]).unwrap().hash();
    assert_eq!(hash(&a), hash(&b));
    assert_ne!(hash(&a), hash(&c));
    // A removal is content too.
    let mods = root.join("mods");
    pack(&mods, "m", r#"remove: { "things": ["a"] }"#, &[]);
    assert_ne!(hash(&a), Data::open(&a, &[mods]).unwrap().hash());
}

#[test]
fn binary_files_come_from_the_last_pack_and_count_in_the_hash() {
    let root = scratch("binary");
    let base = pack(&root, "base", "", &[("models/m.glb", "base bytes")]);
    std::fs::write(base.join("models/skipped.png"), [1, 2, 3]).unwrap();
    let alone = Data::open(&base, &[]).unwrap();
    assert_eq!(alone.binary("models/m.glb").unwrap(), b"base bytes");
    // Only the listed binary kinds are read: an image is not content.
    assert!(alone.binary("models/skipped.png").is_err());
    assert!(alone.file("models/m.glb").is_err());
    let mods = root.join("mods");
    pack(&mods, "m", "", &[("models/m.glb", "mod bytes")]);
    let both = Data::open(&base, &[mods]).unwrap();
    assert_eq!(both.binary("models/m.glb").unwrap(), b"mod bytes");
    assert_eq!(both.origin("models/m.glb"), Some("m"));
    assert_ne!(alone.hash(), both.hash());
    // Bytes are kept as they are: a CR LF in a binary is not a line end.
    let crlf = pack(
        &root.join("crlf"),
        "base",
        "",
        &[("models/m.glb", "a\r\nb")],
    );
    let data = Data::open(&crlf, &[]).unwrap();
    assert_eq!(data.binary("models/m.glb").unwrap(), b"a\r\nb");
}

#[test]
fn the_watcher_sees_a_changed_file() {
    let root = scratch("watch");
    let base = pack(&root, "base", "", &[("x.ron", "1")]);
    let mut watcher = Watcher::new(vec![base.clone()]);
    assert!(!watcher.check());
    write(&base, "x.ron", "22");
    assert!(watcher.check());
    assert!(!watcher.check());
    write(&base, "new/y.ron", "3");
    assert!(watcher.check());
}

#[test]
fn the_repository_data_is_a_base_pack() {
    let data = Data::base();
    assert_eq!(data.packs().len(), 1);
    assert!(data.file("planets/home.ron").is_ok());
    let location = Location::find(&["--data".into(), "x".into(), "--mods".into(), "m".into()]);
    assert_eq!(location.base, PathBuf::from("x"));
    assert_eq!(location.mods, [PathBuf::from("m")]);
}
