use std::path::PathBuf;

use serde_json::{Value, json};
use sysy_core::{
    Container, Design, Edge, EdgeKind, Error, LayoutEntry, Node, NodeKind, Note, Problem, Size,
    load, save, validate,
};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn checkout() -> Design {
    load(fixture("checkout.json")).unwrap()
}

fn problems(result: Result<Design, Error>) -> Vec<Problem> {
    match result {
        Err(Error::Validation(problems)) => problems,
        other => panic!("expected validation problems, got {other:?}"),
    }
}

fn load_json(value: &Value) -> Result<Design, Error> {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("design.json");
    std::fs::write(&path, serde_json::to_vec(value).unwrap()).unwrap();
    load(path)
}

fn full_design() -> Design {
    let mut design = checkout();
    design.containers.push(Container {
        id: "zone".into(),
        label: "Zone".into(),
        description: Some("A nested boundary".into()),
        parent: Some("vpc".into()),
    });
    let kinds = [
        NodeKind::Service,
        NodeKind::Database,
        NodeKind::Queue,
        NodeKind::Cache,
        NodeKind::Storage,
        NodeKind::Client,
        NodeKind::External,
        NodeKind::Function,
        NodeKind::Generic,
    ];
    for (index, kind) in kinds.into_iter().enumerate() {
        design.nodes.push(Node {
            id: format!("node-{index}"),
            kind,
            label: format!("Node {index}"),
            description: Some("Component".into()),
            tags: vec!["critical".into(), "checkout".into()],
            container: Some("zone".into()),
        });
    }
    for (index, kind) in [
        EdgeKind::Sync,
        EdgeKind::Async,
        EdgeKind::Data,
        EdgeKind::Dependency,
    ]
    .into_iter()
    .enumerate()
    {
        design.edges.push(Edge {
            id: format!("edge-{index}"),
            from: "api".into(),
            to: "zone".into(),
            kind,
            label: Some("Connection".into()),
            bidirectional: true,
        });
    }
    for (index, target) in [None, Some("api"), Some("zone"), Some("edge-0")]
        .into_iter()
        .enumerate()
    {
        design.notes.push(Note {
            id: format!("note-{index}"),
            text: "A callout with Unicode: café ☕".into(),
            on: target.map(str::to_owned),
        });
    }
    for (id, size) in [
        (
            "zone",
            Some(Size {
                width: 640.5,
                height: 480.25,
            }),
        ),
        ("vpc", None),
        ("note-0", None),
        ("edge-0", None),
    ] {
        design.layout.insert(
            id.into(),
            LayoutEntry {
                x: -10.5,
                y: 0.25,
                size,
            },
        );
    }
    design.containers.sort_by(|a, b| a.id.cmp(&b.id));
    design.nodes.sort_by(|a, b| a.id.cmp(&b.id));
    design.edges.sort_by(|a, b| a.id.cmp(&b.id));
    design.notes.sort_by(|a, b| a.id.cmp(&b.id));
    design
}

#[test]
fn example_fixture_round_trips_byte_for_byte() {
    let design = checkout();
    assert_eq!(design.title, "Checkout");
    assert!(design.validate().is_empty());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("checkout.json");
    save(&path, &design).unwrap();
    assert_eq!(
        std::fs::read(path).unwrap(),
        std::fs::read(fixture("checkout.json")).unwrap()
    );
}

#[test]
fn fixture_matches_the_documented_example() {
    let doc = include_str!("../../../docs/DESIGN.md");
    let example = doc
        .split_once("```json\n")
        .unwrap()
        .1
        .split_once("```")
        .unwrap()
        .0;
    let mut design = load_json(&serde_json::from_str::<Value>(example).unwrap()).unwrap();
    design.edges.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(design, checkout());
}

#[test]
fn every_element_kind_and_layout_round_trip() {
    let design = full_design();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("design.json");
    save(&path, &design).unwrap();
    assert_eq!(load(path).unwrap(), design);
}

#[test]
fn empty_design_round_trips() {
    let design = Design {
        title: "Empty".into(),
        ..Design::default()
    };
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("design.json");
    save(&path, &design).unwrap();
    assert_eq!(load(path).unwrap(), design);
}

#[test]
fn output_is_deterministic_across_insertion_orders_and_does_not_mutate_input() {
    let mut design = full_design();
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first.json");
    let second = directory.path().join("second.json");
    save(&first, &design).unwrap();
    design.containers.reverse();
    design.nodes.reverse();
    design.edges.reverse();
    design.notes.reverse();
    design.layout = design.layout.into_iter().rev().collect();
    let original = design.clone();
    save(&second, &design).unwrap();
    let bytes = std::fs::read(&first).unwrap();
    assert_eq!(bytes, std::fs::read(&second).unwrap());
    assert_eq!(design, original);
    save(&second, &design).unwrap();
    assert_eq!(bytes, std::fs::read(&second).unwrap());
}

#[test]
fn unknown_version_names_found_and_supported_versions() {
    let error = load(fixture("unknown-version.json")).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unsupported version 7; supported version is 1")
    );
    assert_eq!(
        problems(Err(error)),
        vec![Problem::UnknownVersion {
            found: json!(7),
            supported: 1
        }]
    );
}

#[test]
fn duplicate_missing_endpoint_and_cycle_are_reported_together() {
    let found = problems(load(fixture("multiple-problems.json")));
    assert_eq!(
        found,
        vec![
            Problem::DuplicateId { id: "api".into() },
            Problem::MissingReference {
                id: "persist".into(),
                field: "to",
                target: "missing-node".into()
            },
            Problem::ContainerCycle {
                ids: vec!["region".into(), "vpc".into()]
            },
        ]
    );
}

#[test]
fn unknown_keys_and_version_do_not_hide_model_problems() {
    let mut value: Value =
        serde_json::from_str(include_str!("fixtures/multiple-problems.json")).unwrap();
    value["version"] = json!(99);
    value["surprise"] = json!(true);
    value["another"] = json!({});
    let found = problems(load_json(&value));
    assert_eq!(found.len(), 6);
    assert!(found.contains(&Problem::UnknownTopLevelKey {
        key: "surprise".into()
    }));
    assert!(found.contains(&Problem::UnknownTopLevelKey {
        key: "another".into()
    }));
}

#[test]
fn malformed_ids_are_rejected_in_memory_and_on_load() {
    for id in [
        "",
        "API",
        "two words",
        "-api",
        "api-",
        "has_underscore",
        "café",
        "a/b",
        "a.b",
    ] {
        let design = Design {
            notes: vec![Note {
                id: id.into(),
                text: "note".into(),
                on: None,
            }],
            ..Design::default()
        };
        let expected = vec![Problem::InvalidId { id: id.into() }];
        assert_eq!(validate(&design), expected);
        let value = json!({"version": 1, "design": {"title": "Ids"}, "containers": [], "nodes": [], "edges": [], "notes": design.notes, "layout": {}});
        assert_eq!(problems(load_json(&value)), expected);
    }
    for id in ["a", "0", "api-2", "a--b", "123"] {
        let design = Design {
            notes: vec![Note {
                id: id.into(),
                text: "note".into(),
                on: None,
            }],
            ..Design::default()
        };
        assert!(validate(&design).is_empty(), "{id}");
    }
}

#[test]
fn duplicate_ids_are_global_across_all_element_types() {
    let mut design = full_design();
    design.containers[0].id = "shared".into();
    design.nodes[0].id = "shared".into();
    design.edges[0].id = "shared".into();
    design.notes[0].id = "shared".into();
    let found = design.validate();
    assert_eq!(
        found
            .iter()
            .filter(|problem| matches!(problem, Problem::DuplicateId { id } if id == "shared"))
            .count(),
        3
    );
}

#[test]
fn missing_references_include_membership_parents_endpoints_notes_and_layout() {
    let mut design = full_design();
    design.nodes[0].container = Some("gone".into());
    design.containers[1].parent = Some("gone".into());
    design.edges[0].from = "gone".into();
    design.edges[0].to = "gone".into();
    design.notes[0].on = Some("gone".into());
    design.layout.insert(
        "gone".into(),
        LayoutEntry {
            x: 0.0,
            y: 0.0,
            size: None,
        },
    );
    let found = design.validate();
    assert_eq!(found.len(), 6);
    for field in ["container", "parent", "from", "to", "on", "layout"] {
        assert!(found.iter().any(|problem| matches!(problem, Problem::MissingReference { field: actual, target, .. } if *actual == field && target == "gone")));
    }
}

#[test]
fn existing_ids_must_have_the_correct_reference_type() {
    let mut design = full_design();
    design.nodes[0].container = Some("api".into());
    design.containers[1].parent = Some("api".into());
    design.edges[0].from = "note-0".into();
    design.edges[0].to = "edge-1".into();
    design.notes[0].on = Some("note-1".into());
    let found = design.validate();
    assert_eq!(found.len(), 5);
    assert!(
        found
            .iter()
            .all(|problem| matches!(problem, Problem::InvalidReference { .. }))
    );
}

#[test]
fn edges_allow_node_container_in_either_direction_but_not_two_containers() {
    let mut design = full_design();
    design.edges[0].from = "vpc".into();
    design.edges[0].to = "api".into();
    assert!(design.validate().is_empty());
    design.edges[0].to = "zone".into();
    assert_eq!(
        design.validate(),
        vec![Problem::InvalidEdge {
            id: "edge-0".into()
        }]
    );
}

#[test]
fn cycles_are_reported_once_each_including_self_cycles() {
    let mut design = Design::default();
    for (id, parent) in [
        ("a", "b"),
        ("b", "c"),
        ("c", "b"),
        ("d", "d"),
        ("e", "f"),
        ("f", "e"),
    ] {
        design.containers.push(Container {
            id: id.into(),
            label: id.into(),
            description: None,
            parent: Some(parent.into()),
        });
    }
    assert_eq!(
        design.validate(),
        vec![
            Problem::ContainerCycle {
                ids: vec!["b".into(), "c".into()]
            },
            Problem::ContainerCycle {
                ids: vec!["d".into()]
            },
            Problem::ContainerCycle {
                ids: vec!["e".into(), "f".into()]
            },
        ]
    );
    design.containers.reverse();
    assert_eq!(design.validate().len(), 3);
}

#[test]
fn deeply_nested_containers_do_not_need_recursion() {
    let mut design = Design::default();
    for index in 0..10_000 {
        design.containers.push(Container {
            id: format!("c-{index}"),
            label: index.to_string(),
            description: None,
            parent: (index < 9_999).then(|| format!("c-{}", index + 1)),
        });
    }
    assert!(design.validate().is_empty());
}

#[test]
fn invalid_layout_cannot_be_saved_as_null_or_with_size_on_a_node() {
    let mut design = checkout();
    design.layout.get_mut("api").unwrap().x = f64::NAN;
    design.layout.get_mut("api").unwrap().size = Some(Size {
        width: f64::INFINITY,
        height: 10.0,
    });
    let found = design.validate();
    assert_eq!(found.len(), 3);
    assert!(
        found
            .iter()
            .all(|problem| matches!(problem, Problem::InvalidLayout { id, .. } if id == "api"))
    );
}

#[test]
fn invalid_save_leaves_original_untouched() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("design.json");
    let mut design = checkout();
    save(&path, &design).unwrap();
    let original = std::fs::read(&path).unwrap();
    design.edges[0].to = "missing".into();
    assert!(matches!(save(&path, &design), Err(Error::Validation(_))));
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn rename_failure_preserves_target_and_cleans_temporary() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("occupied");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("original"), b"unchanged").unwrap();
    assert!(matches!(save(&target, &checkout()), Err(Error::Io(_))));
    assert_eq!(
        std::fs::read(target.join("original")).unwrap(),
        b"unchanged"
    );
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn malformed_records_do_not_hide_other_records_or_model_problems() {
    let mut value: Value =
        serde_json::from_str(include_str!("fixtures/multiple-problems.json")).unwrap();
    value["nodes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "bad", "kind": "unknown-kind", "label": "Bad"}));
    value["notes"] =
        json!([{"id": "bad-note"}, {"id": "orphan", "text": "Orphan", "on": "missing"}]);
    value["layout"] = json!({
        "api": {"x": "left", "y": 0},
        "orders": {"x": 0},
        "missing": {"x": 0, "y": 0}
    });
    let found = problems(load_json(&value));
    assert_eq!(found.len(), 9);
    assert!(found.iter().any(
        |problem| matches!(problem, Problem::InvalidField { field, .. } if field == "nodes[4]")
    ));
    assert!(found.iter().any(
        |problem| matches!(problem, Problem::InvalidField { field, .. } if field == "notes[0]")
    ));
    for id in ["api", "orders"] {
        assert!(found.iter().any(
            |problem| matches!(problem, Problem::InvalidField { field, .. } if *field == format!("layout.{id}"))
        ));
    }
    assert!(found.contains(&Problem::MissingReference {
        id: "missing".into(),
        field: "layout",
        target: "missing".into(),
    }));
}

#[test]
fn missing_required_sections_wrong_types_invalid_json_and_io_errors_are_rejected() {
    assert_eq!(problems(load_json(&json!({}))).len(), 6);
    assert!(
        matches!(problems(load_json(&json!([]))).as_slice(), [Problem::InvalidField { field, .. }] if field == "document")
    );
    let mut value: Value = serde_json::from_str(include_str!("fixtures/checkout.json")).unwrap();
    value["layout"] = json!({"api": {"x": "left", "y": 0}});
    assert!(
        matches!(problems(load_json(&value)).as_slice(), [Problem::InvalidField { field, .. }] if field == "layout.api")
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("design.json");
    assert!(matches!(load(&path), Err(Error::Io(_))));
    std::fs::write(&path, b"{").unwrap();
    assert!(matches!(load(&path), Err(Error::Json(_))));
    assert!(matches!(
        save(directory.path().join("missing/design.json"), &checkout()),
        Err(Error::Io(_))
    ));
}

#[test]
fn omitted_layout_is_empty_but_explicit_invalid_layout_is_rejected() {
    let mut value: Value = serde_json::from_str(include_str!("fixtures/checkout.json")).unwrap();
    value.as_object_mut().unwrap().remove("layout");
    let design = load_json(&value).unwrap();
    assert!(design.layout.is_empty());
    let mut expected = checkout();
    expected.layout.clear();
    assert_eq!(design, expected);
    for invalid in [Value::Null, json!([]), json!("positions"), json!(12)] {
        value["layout"] = invalid;
        assert!(
            matches!(problems(load_json(&value)).as_slice(), [Problem::InvalidField { field, .. }] if field == "layout")
        );
    }
}
