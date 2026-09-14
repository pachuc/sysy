use std::fs;
use std::process::{Command, Output};

use serde_json::{Value, json};
use sysy_core::{Design, load, save};
use tempfile::{TempDir, tempdir};

struct Harness {
    directory: TempDir,
}

impl Harness {
    fn empty() -> Self {
        Self {
            directory: tempdir().unwrap(),
        }
    }

    fn new() -> Self {
        let harness = Self::empty();
        harness.ok(
            &["new"],
            &[
                "--title",
                "Checkout",
                "--description",
                "Order placement path.",
            ],
        );
        harness
    }

    fn path(&self) -> std::path::PathBuf {
        self.directory.path().join("design.json")
    }

    fn run(&self, command: &[&str], args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_sysy"))
            .arg("--json")
            .args(command)
            .arg(self.path())
            .args(args)
            .output()
            .unwrap()
    }

    fn ok(&self, command: &[&str], args: &[&str]) -> Value {
        let result = self.run(command, args);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stderr.is_empty());
        serde_json::from_slice(&result.stdout).unwrap()
    }

    fn error(&self, command: &[&str], args: &[&str]) -> String {
        let before = fs::read(self.path()).unwrap();
        let result = self.run(command, args);
        assert_eq!(result.status.code(), Some(1));
        assert!(result.stdout.is_empty());
        let error: Value = serde_json::from_slice(&result.stderr).unwrap();
        let message = error["error"]["message"].as_str().unwrap().to_owned();
        assert_eq!(error, json!({"error": {"message": message}}));
        assert_eq!(fs::read(self.path()).unwrap(), before);
        message
    }

    fn design(&self) -> Design {
        load(self.path()).unwrap()
    }
}

#[test]
fn new_creates_valid_design_and_never_overwrites() {
    let harness = Harness::new();
    assert_eq!(
        harness.design(),
        Design {
            title: "Checkout".into(),
            description: Some("Order placement path.".into()),
            ..Design::default()
        }
    );
    harness.error(&["new"], &["--title", "Replacement"]);
    assert_eq!(harness.ok(&["validate"], &[]), json!([]));
    let empty = Harness::empty();
    let output = empty.ok(&["new"], &["--title", "Minimal"]);
    assert_eq!(output, serde_json::to_value(empty.design()).unwrap());
    assert_eq!(empty.design().description, None);
}

#[test]
fn commands_build_checkout_fixture_and_show_its_file_representation() {
    let harness = Harness::new();
    harness.ok(&["container", "add"], &["vpc", "--label", "Production VPC"]);
    for (id, kind, label, container) in [
        ("shopper", "client", "Shopper", false),
        ("orders", "database", "Orders DB", true),
        ("api", "service", "Checkout API", true),
    ] {
        let mut args = vec![id, "--kind", kind, "--label", label];
        if container {
            args.extend(["--in", "vpc"]);
        }
        harness.ok(&["node", "add"], &args);
    }
    harness.ok(
        &["edge", "add"],
        &[
            "place-order",
            "--from",
            "shopper",
            "--to",
            "api",
            "--label",
            "POST /orders",
        ],
    );
    harness.ok(
        &["edge", "add"],
        &[
            "persist", "--from", "api", "--to", "orders", "--kind", "data",
        ],
    );
    harness.ok(
        &["note", "add"],
        &[
            "temporary",
            "--text",
            "Check persistence",
            "--on",
            "persist",
        ],
    );
    assert_eq!(
        harness.ok(&["note", "remove"], &["temporary"])["id"],
        "temporary"
    );
    let fixture = load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../sysy-core/tests/fixtures/checkout.json"
    ))
    .unwrap();
    // Position editing belongs to a later command; supply only the fixture's
    // layout directly so every architecture record is built through this CLI.
    let mut design = harness.design();
    design.layout.clone_from(&fixture.layout);
    save(harness.path(), &design).unwrap();
    assert_eq!(harness.design(), fixture);
    assert_eq!(
        harness.ok(&["show"], &[]),
        serde_json::to_value(fixture).unwrap()
    );
}

#[test]
fn invalid_edits_report_all_problems_without_writing() {
    let harness = Harness::new();
    let error = harness.error(
        &["edge", "add"],
        &[
            "broken",
            "--from",
            "missing-source",
            "--to",
            "missing-target",
        ],
    );
    assert!(error.contains("missing-source"));
    assert!(error.contains("missing-target"));
    harness.ok(&["container", "add"], &["parent", "--label", "Parent"]);
    harness.ok(
        &["container", "add"],
        &["child", "--label", "Child", "--in", "parent"],
    );
    assert!(
        harness
            .error(&["container", "set"], &["parent", "--in", "child"])
            .contains("cycle")
    );
    assert!(
        harness
            .error(&["note", "add"], &["parent", "--text", "Duplicate"])
            .contains("duplicate")
    );
    for kind in ["node", "container", "edge", "note"] {
        assert!(
            harness
                .error(&[kind, "remove"], &["absent"])
                .contains("absent")
        );
        let field = if kind == "note" { "--text" } else { "--label" };
        assert!(
            harness
                .error(&[kind, "set"], &["absent", field, "Value"])
                .contains("absent")
        );
    }
}

#[test]
fn add_set_and_list_return_saved_records_in_id_order() {
    let harness = populated_design();
    for (kind, args, expected) in [
        (
            "container",
            vec![
                "z-container",
                "--label",
                "Updated",
                "--in",
                "a-container",
                "--clear-description",
            ],
            json!({"id":"z-container", "label":"Updated", "parent":"a-container"}),
        ),
        (
            "node",
            vec![
                "z-node",
                "--kind",
                "database",
                "--label",
                "Updated",
                "--tag",
                "replacement",
                "--clear-description",
                "--clear-in",
            ],
            json!({"id":"z-node", "kind":"database", "label":"Updated", "tags":["replacement"]}),
        ),
        (
            "edge",
            vec![
                "z-edge",
                "--from",
                "z-node",
                "--to",
                "a-container",
                "--kind",
                "dependency",
                "--label",
                "Updated",
                "--bidirectional=false",
            ],
            json!({"id":"z-edge", "from":"z-node", "to":"a-container", "kind":"dependency", "label":"Updated"}),
        ),
        (
            "note",
            vec!["z-note", "--text", "Updated", "--clear-on"],
            json!({"id":"z-note", "text":"Updated"}),
        ),
    ] {
        assert_eq!(harness.ok(&[kind, "set"], &args), expected);
        let section = format!("{kind}s");
        assert_eq!(harness.ok(&["show"], &[])[&section][1], expected);
        // Hand-edited arrays need not already be sorted on disk.
        let mut file: Value = serde_json::from_slice(&fs::read(harness.path()).unwrap()).unwrap();
        file[&section].as_array_mut().unwrap().reverse();
        fs::write(harness.path(), serde_json::to_vec(&file).unwrap()).unwrap();
        let listed = harness.ok(&[kind, "list"], &[]);
        assert_eq!(listed.as_array().unwrap().len(), 2);
        assert_eq!(listed[0]["id"], format!("a-{kind}"));
        assert_eq!(listed[1], expected);
    }
    assert!(
        harness
            .ok(&["node", "set"], &["z-node", "--clear-tags"])
            .get("tags")
            .is_none()
    );
    assert!(
        harness
            .ok(&["container", "set"], &["z-container", "--clear-in"])
            .get("parent")
            .is_none()
    );
    assert!(
        harness
            .ok(&["edge", "set"], &["z-edge", "--clear-label"])
            .get("label")
            .is_none()
    );
    assert_eq!(
        harness.ok(&["edge", "set"], &["z-edge", "--bidirectional"])["bidirectional"],
        true
    );
    assert_eq!(
        harness.ok(&["note", "set"], &["z-note", "--on", "z-node"])["on"],
        "z-node"
    );
}

#[test]
fn removals_cascade_and_reparent_without_dangling_layout() {
    let harness = design_with_dependents();
    assert_eq!(harness.ok(&["edge", "remove"], &["ab"])["id"], "ab");
    assert!(
        !harness
            .design()
            .notes
            .iter()
            .any(|note| note.id == "note-ab")
    );
    assert!(!harness.design().layout.contains_key("note-ab"));
    harness.ok(&["container", "remove"], &["middle"]);
    let design = harness.design();
    assert!(
        design
            .nodes
            .iter()
            .all(|node| node.container.as_deref() == Some("root"))
    );
    assert_eq!(
        design
            .containers
            .iter()
            .find(|c| c.id == "child")
            .unwrap()
            .parent
            .as_deref(),
        Some("root")
    );
    assert!(design.edges.is_empty());
    assert!(!design.layout.contains_key("middle"));
    assert!(!design.layout.contains_key("am"));
    harness.ok(&["edge", "add"], &["ab", "--from", "a", "--to", "b"]);
    harness.ok(
        &["note", "add"],
        &["note-ab", "--text", "Attached", "--on", "ab"],
    );
    harness.ok(&["node", "remove"], &["a"]);
    let design = harness.design();
    assert_eq!(design.nodes.len(), 1);
    assert!(design.edges.is_empty());
    assert_eq!(
        design
            .notes
            .iter()
            .map(|note| note.id.as_str())
            .collect::<Vec<_>>(),
        ["note-b", "note-child"]
    );
    assert_eq!(
        design.layout.keys().map(String::as_str).collect::<Vec<_>>(),
        ["child"]
    );
    harness.ok(&["container", "remove"], &["root"]);
    assert!(harness.design().nodes[0].container.is_none());
    assert!(harness.design().containers[0].parent.is_none());
    harness.ok(&["validate"], &[]);
}

#[test]
fn validate_reports_multiple_hand_edited_problems() {
    let harness = Harness::new();
    let mut file = harness.ok(&["show"], &[]);
    file["notes"] = json!([
        {"id":"first", "text":"One", "on":"missing-one"},
        {"id":"second", "text":"Two", "on":"missing-two"}
    ]);
    fs::write(harness.path(), serde_json::to_vec(&file).unwrap()).unwrap();
    let message = harness.error(&["validate"], &[]);
    assert!(message.contains("missing-one"));
    assert!(message.contains("missing-two"));
    let message = harness.error(&["note", "add"], &["third", "--text", "Three"]);
    assert!(message.contains("missing-one"));
    assert!(message.contains("missing-two"));
}

#[test]
fn clap_errors_exit_two_and_human_output_is_short() {
    let harness = Harness::new();
    let before = fs::read(harness.path()).unwrap();
    for (command, args) in [
        (vec!["node", "add"], vec!["missing-flags"]),
        (
            vec!["node", "add"],
            vec!["bad-kind", "--kind", "invalid", "--label", "Bad"],
        ),
        (vec!["node", "set"], vec!["a", "--in", "b", "--clear-in"]),
    ] {
        assert_eq!(harness.run(&command, &args).status.code(), Some(2));
    }
    assert_eq!(fs::read(harness.path()).unwrap(), before);
    let human = Command::new(env!("CARGO_BIN_EXE_sysy"))
        .arg("show")
        .arg(harness.path())
        .output()
        .unwrap();
    assert!(human.status.success());
    let text = String::from_utf8(human.stdout).unwrap();
    assert!(text.starts_with("Checkout:"));
    assert_eq!(text.lines().count(), 1);
    let global = Command::new(env!("CARGO_BIN_EXE_sysy"))
        .args(["note", "list"])
        .arg(harness.path())
        .arg("--json")
        .output()
        .unwrap();
    assert!(global.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&global.stdout).unwrap(),
        json!([])
    );
}

fn populated_design() -> Harness {
    let harness = Harness::new();
    for id in ["z-container", "a-container"] {
        let added = harness.ok(
            &["container", "add"],
            &[id, "--label", "Container", "--description", "Details"],
        );
        assert_eq!(
            added,
            serde_json::to_value(
                harness
                    .design()
                    .containers
                    .iter()
                    .find(|x| x.id == id)
                    .unwrap()
            )
            .unwrap()
        );
    }
    for id in ["z-node", "a-node"] {
        let added = harness.ok(
            &["node", "add"],
            &[
                id,
                "--kind",
                "service",
                "--label",
                "Node",
                "--description",
                "Details",
                "--tag",
                "first",
                "--tag",
                "second",
                "--in",
                "z-container",
            ],
        );
        assert_eq!(
            added,
            serde_json::to_value(harness.design().nodes.iter().find(|x| x.id == id).unwrap())
                .unwrap()
        );
    }
    for id in ["z-edge", "a-edge"] {
        let added = harness.ok(
            &["edge", "add"],
            &[id, "--from", "a-node", "--to", "z-node", "--bidirectional"],
        );
        assert_eq!(
            added,
            serde_json::to_value(harness.design().edges.iter().find(|x| x.id == id).unwrap())
                .unwrap()
        );
    }
    for id in ["z-note", "a-note"] {
        let added = harness.ok(&["note", "add"], &[id, "--text", "Note", "--on", "a-edge"]);
        assert_eq!(
            added,
            serde_json::to_value(harness.design().notes.iter().find(|x| x.id == id).unwrap())
                .unwrap()
        );
    }
    harness
}

fn design_with_dependents() -> Harness {
    let harness = Harness::new();
    for (id, parent) in [
        ("root", None),
        ("middle", Some("root")),
        ("child", Some("middle")),
    ] {
        let mut args = vec![id, "--label", id];
        if let Some(parent) = parent {
            args.extend(["--in", parent]);
        }
        harness.ok(&["container", "add"], &args);
    }
    for id in ["a", "b"] {
        harness.ok(
            &["node", "add"],
            &[id, "--kind", "generic", "--label", id, "--in", "middle"],
        );
    }
    for (id, target) in [("ab", "b"), ("am", "middle")] {
        harness.ok(&["edge", "add"], &[id, "--from", "a", "--to", target]);
    }
    for target in ["a", "ab", "middle", "am", "child", "b"] {
        harness.ok(
            &["note", "add"],
            &[
                &format!("note-{target}"),
                "--text",
                "Attached",
                "--on",
                target,
            ],
        );
    }
    let mut design = harness.design();
    for id in [
        "a",
        "ab",
        "middle",
        "am",
        "child",
        "note-a",
        "note-ab",
        "note-middle",
        "note-am",
    ] {
        design.layout.insert(
            id.into(),
            sysy_core::LayoutEntry {
                x: 1.0,
                y: 2.0,
                size: None,
            },
        );
    }
    save(harness.path(), &design).unwrap();
    harness
}

#[test]
fn layout_writes_positions_is_idempotent_and_reset_recomputes_pins() {
    let harness = Harness::empty();
    let mut fixture: Value =
        serde_json::from_str(include_str!("../../sysy-core/tests/fixtures/checkout.json")).unwrap();
    fixture.as_object_mut().unwrap().remove("layout");
    fixture["notes"] = json!([{ "id": "reminder", "text": "Keep orders durable", "on": "orders" }]);
    fs::write(harness.path(), serde_json::to_vec(&fixture).unwrap()).unwrap();
    let result = harness.ok(&["layout"], &[]);
    let first = harness.design();
    assert_eq!(result, serde_json::to_value(&first.layout).unwrap());
    // Edges are derived from their endpoints; the layout returns boxes for nodes,
    // containers, and notes, and preserves any existing edge entries.
    for id in ["api", "orders", "shopper", "vpc", "reminder"] {
        assert!(first.layout.contains_key(id));
    }
    let bytes = fs::read(harness.path()).unwrap();
    assert_eq!(harness.ok(&["layout"], &[]), result);
    assert_eq!(fs::read(harness.path()).unwrap(), bytes);
    let mut pinned = first.clone();
    pinned.layout.get_mut("shopper").unwrap().x = -9000.0;
    pinned.layout.insert(
        "persist".into(),
        sysy_core::LayoutEntry {
            x: 123.0,
            y: 456.0,
            size: None,
        },
    );
    save(harness.path(), &pinned).unwrap();
    harness.ok(&["layout"], &[]);
    assert_eq!(harness.design().layout["shopper"], pinned.layout["shopper"]);
    assert_eq!(harness.design().layout["persist"], pinned.layout["persist"]);
    harness.ok(&["layout"], &["--reset"]);
    assert_eq!(harness.design(), first);
}

#[test]
fn layout_grows_pinned_containers_and_reports_invalid_files_without_writing() {
    let harness = Harness::empty();
    fs::write(
        harness.path(),
        include_str!("../../sysy-layout/tests/fixtures/pinned-container.json"),
    )
    .unwrap();
    let original = harness.design();
    harness.ok(&["layout"], &[]);
    let placed = harness.design();
    for (id, pin) in &original.layout {
        let actual = placed.layout[id];
        assert_eq!((actual.x, actual.y), (pin.x, pin.y));
        if let Some(size) = pin.size {
            let grown = actual.size.unwrap();
            assert!(grown.width >= size.width && grown.height >= size.height);
        }
    }
    let grown = placed.layout["box"].size.unwrap();
    let saved = original.layout["box"].size.unwrap();
    assert!(grown.width > saved.width || grown.height > saved.height);
    fs::write(harness.path(), "{invalid").unwrap();
    harness.error(&["layout"], &[]);
    harness.error(&["layout"], &["--reset"]);
    harness.error(&["ui"], &[]);
}
