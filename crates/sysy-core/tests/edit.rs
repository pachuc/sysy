use sysy_core::{
    Container, ContainerUpdate, Design, Edge, EdgeKind, EdgeUpdate, Error, LayoutEntry, NodeUpdate,
    Note, NoteUpdate, OptionalUpdate, load,
};

fn fixture() -> Design {
    load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/checkout.json"
    ))
    .unwrap()
}

#[test]
fn rejected_additions_preserve_the_callers_design() {
    let mut design = fixture();
    let before = design.clone();
    let mut node = design.nodes[0].clone();
    node.id = "vpc".into();
    assert!(design.add_node(node).is_err());
    assert_eq!(design, before);
    assert!(
        design
            .add_container(Container {
                id: "api".into(),
                label: "Duplicate".into(),
                description: None,
                parent: None,
            })
            .is_err()
    );
    assert_eq!(design, before);
    assert!(
        design
            .add_note(Note {
                id: "api".into(),
                text: "Duplicate".into(),
                on: None,
            })
            .is_err()
    );
    assert_eq!(design, before);
    let error = design
        .add_edge(Edge {
            id: "broken".into(),
            from: "missing-source".into(),
            to: "missing-target".into(),
            kind: EdgeKind::Sync,
            label: None,
            bidirectional: false,
        })
        .unwrap_err();
    let Error::Validation(problems) = error else {
        panic!("expected validation problems")
    };
    assert_eq!(problems.len(), 2);
    assert_eq!(design, before);
}

#[test]
fn rejected_updates_preserve_every_field_and_layout() {
    let mut design = fixture();
    design
        .add_note(Note {
            id: "note".into(),
            text: "Original".into(),
            on: Some("api".into()),
        })
        .unwrap();
    let before = design.clone();
    assert!(
        design
            .set_node(
                "api",
                NodeUpdate {
                    label: Some("Changed".into()),
                    container: OptionalUpdate::Set("missing".into()),
                    ..NodeUpdate::default()
                }
            )
            .is_err()
    );
    assert_eq!(design, before);
    assert!(
        design
            .set_container(
                "vpc",
                ContainerUpdate {
                    parent: OptionalUpdate::Set("vpc".into()),
                    ..ContainerUpdate::default()
                }
            )
            .is_err()
    );
    assert_eq!(design, before);
    assert!(
        design
            .set_edge(
                "persist",
                EdgeUpdate {
                    from: Some("missing".into()),
                    ..EdgeUpdate::default()
                }
            )
            .is_err()
    );
    assert_eq!(design, before);
    assert!(
        design
            .set_note(
                "note",
                NoteUpdate {
                    text: Some("Changed".into()),
                    on: OptionalUpdate::Set("missing".into()),
                }
            )
            .is_err()
    );
    assert_eq!(design, before);
}

#[test]
fn removals_validate_the_whole_candidate_before_committing() {
    let mut design = fixture();
    design
        .add_note(Note {
            id: "note".into(),
            text: "Original".into(),
            on: Some("api".into()),
        })
        .unwrap();
    // A viewer can have unsaved invalid state, unlike a CLI that just loaded.
    design.layout.insert(
        "missing".into(),
        LayoutEntry {
            x: 1.0,
            y: 2.0,
            size: None,
        },
    );
    let before = design.clone();
    assert!(design.remove_node("api").is_err());
    assert_eq!(design, before);
    assert!(design.remove_container("vpc").is_err());
    assert_eq!(design, before);
    assert!(design.remove_edge("persist").is_err());
    assert_eq!(design, before);
    assert!(design.remove_note("note").is_err());
    assert_eq!(design, before);
}

#[test]
fn concurrent_creation_has_one_winner_and_a_complete_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("design.json");
    let barrier = std::sync::Barrier::new(2);
    let outcomes = std::thread::scope(|scope| {
        let threads: Vec<_> = ["First", "Second"]
            .into_iter()
            .map(|title| {
                let path = &path;
                let barrier = &barrier;
                scope.spawn(move || {
                    let design = Design {
                        title: title.into(),
                        ..Design::default()
                    };
                    barrier.wait();
                    sysy_core::create(path, &design).map(|()| title)
                })
            })
            .collect();
        threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    let winner = outcomes.into_iter().find_map(Result::ok).unwrap();
    assert_eq!(load(path).unwrap().title, winner);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn design_metadata_updates_preserve_architecture_and_layout() {
    let mut design = fixture();
    let original = design.clone();
    let returned = design
        .set(sysy_core::DesignUpdate {
            title: Some("Revised checkout".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(returned, design);
    assert_eq!(design.title, "Revised checkout");
    assert_eq!(design.description, original.description);
    design
        .set(sysy_core::DesignUpdate {
            description: OptionalUpdate::Set("New scope".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(design.description.as_deref(), Some("New scope"));
    design
        .set(sysy_core::DesignUpdate {
            description: OptionalUpdate::Clear,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(design.description, None);
    assert_eq!(design.nodes, original.nodes);
    assert_eq!(design.containers, original.containers);
    assert_eq!(design.edges, original.edges);
    assert_eq!(design.notes, original.notes);
    assert_eq!(design.layout, original.layout);
    let before = design.clone();
    assert_eq!(
        design.set(sysy_core::DesignUpdate::default()).unwrap(),
        before
    );
    design.nodes[0].container = Some("missing".into());
    let before = design.clone();
    assert!(
        design
            .set(sysy_core::DesignUpdate {
                title: Some("Rejected".into()),
                ..Default::default()
            })
            .is_err()
    );
    assert_eq!(design, before);
}
