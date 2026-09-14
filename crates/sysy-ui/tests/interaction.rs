use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

use notify::{
    Event, EventKind,
    event::{AccessKind, CreateKind, DataChange, ModifyKind, RemoveKind, RenameMode},
};
use sysy_core::{Container, Design, Layout, LayoutEntry, Node, NodeKind};
use sysy_layout::{Point, layout};
use sysy_ui::interaction::{
    DEBOUNCE, Debounce, ViewerState, dragged_positions, event_matches, merge_positions,
    save_positions,
};

fn example() -> Design {
    let mut design = sysy_core::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../sysy-core/tests/fixtures/checkout.json"
    ))
    .unwrap();
    design.containers.extend([
        Container {
            id: "inner".into(),
            label: "Inner".into(),
            parent: Some("vpc".into()),
            description: None,
        },
        Container {
            id: "deep".into(),
            label: "Deep".into(),
            parent: Some("inner".into()),
            description: None,
        },
        Container {
            id: "outside".into(),
            label: "Outside".into(),
            parent: None,
            description: None,
        },
    ]);
    design
        .nodes
        .iter_mut()
        .find(|node| node.id == "api")
        .unwrap()
        .container = Some("deep".into());
    design.nodes.push(Node {
        id: "nested".into(),
        label: "Nested".into(),
        kind: NodeKind::Service,
        container: Some("inner".into()),
        description: None,
        tags: Vec::new(),
    });
    design
        .containers
        .sort_by(|left, right| left.id.cmp(&right.id));
    design.nodes.sort_by(|left, right| left.id.cmp(&right.id));
    design
}

#[test]
fn container_drag_moves_all_descendants_and_nothing_else() {
    let design = example();
    let positions = layout(&design);
    let changed = dragged_positions(&design, &positions, "vpc", Point { x: 123.0, y: -45.0 });
    assert_eq!(
        changed.keys().map(String::as_str).collect::<Vec<_>>(),
        ["api", "deep", "inner", "nested", "orders", "vpc"]
    );
    for (id, entry) in &changed {
        assert!((entry.x - positions[id].x - 123.0).abs() < 1e-8);
        assert!((entry.y - positions[id].y + 45.0).abs() < 1e-8);
        assert_eq!(entry.size, positions[id].size);
    }
    let mut moved = positions.clone();
    moved.extend(changed);
    for id in ["outside", "shopper"] {
        assert_eq!(moved[id], positions[id]);
    }
    let inner = dragged_positions(&design, &positions, "inner", Point { x: -20.0, y: 60.0 });
    assert_eq!(
        inner.keys().map(String::as_str).collect::<Vec<_>>(),
        ["api", "deep", "inner", "nested"]
    );
    let node = dragged_positions(&design, &positions, "api", Point { x: 10.0, y: 20.0 });
    assert_eq!(node.len(), 1);
    assert!((node["api"].x - positions["api"].x - 10.0).abs() < 1e-8);
    assert!(dragged_positions(&design, &positions, "persist", Point::default()).is_empty());
}

#[test]
fn merge_preserves_disk_elements_metadata_and_unrelated_pins() {
    let original = example();
    let changed = dragged_positions(
        &original,
        &layout(&original),
        "vpc",
        Point { x: 30.0, y: 50.0 },
    );
    let mut disk = original;
    disk.title = "Agent's new title".into();
    disk.description = Some("Updated description".into());
    disk.nodes[0].label = "Updated API".into();
    disk.nodes[0].tags.push("new-tag".into());
    disk.nodes.push(Node {
        id: "new".into(),
        label: "New".into(),
        kind: NodeKind::Cache,
        container: None,
        description: None,
        tags: Vec::new(),
    });
    disk.layout.insert(
        "shopper".into(),
        LayoutEntry {
            x: -900.0,
            y: 800.0,
            size: None,
        },
    );
    let mut expected = disk.clone();
    expected.layout.extend(changed.clone());
    merge_positions(&mut disk, &changed);
    assert_eq!(disk, expected);
    assert!(disk.validate().is_empty());
}

#[test]
fn save_reloads_disk_and_does_not_resurrect_deleted_elements() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("design.json");
    let mut disk = example();
    let changed = dragged_positions(&disk, &layout(&disk), "vpc", Point { x: 50.0, y: 70.0 });
    disk.nodes.retain(|node| node.id != "nested");
    disk.title = "Concurrent edit".into();
    sysy_core::save(&path, &disk).unwrap();
    let saved = save_positions(&path, &changed).unwrap();
    assert_eq!(saved.title, "Concurrent edit");
    assert!(!saved.nodes.iter().any(|node| node.id == "nested"));
    assert!(!saved.layout.contains_key("nested"));
    merge_positions(&mut disk, &changed);
    assert_eq!(saved, disk);
    assert_eq!(sysy_core::load(&path).unwrap(), saved);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn drag_pins_only_changed_elements_and_preserves_other_positions_after_reload() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("design.json");
    let design = example();
    sysy_core::save(&path, &design).unwrap();
    let mut state = ViewerState::new(design);
    let before = state.positions.clone();
    let start = Point { x: 300.0, y: 400.0 };
    state.begin_drag("vpc", start);
    state.drag_to(Point { x: 320.0, y: 450.0 });
    state.drag_to(Point { x: 400.0, y: 350.0 });
    let expected = dragged_positions(&state.design, &before, "vpc", Point { x: 100.0, y: -50.0 });
    for (id, entry) in &expected {
        assert_eq!(state.positions[id], *entry);
    }
    state.finish_drag(&path);
    assert!(!state.is_dragging());
    let mut pins = example().layout;
    pins.extend(expected.clone());
    assert_eq!(state.design.layout, pins);
    let mut all = before;
    all.extend(expected);
    assert_eq!(state.positions, all);
    state.reload(&path);
    assert_eq!(state.positions, all);
    assert_eq!(state.design, sysy_core::load(&path).unwrap());
    let reopened = ViewerState::new(sysy_core::load(&path).unwrap());
    for (id, entry) in pins {
        assert_eq!(reopened.positions[&id], entry);
    }
}

#[test]
fn click_and_cancel_do_not_save_and_invalid_file_rolls_back_drag() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("design.json");
    let design = example();
    sysy_core::save(&path, &design).unwrap();
    let original_bytes = fs::read(&path).unwrap();
    let mut state = ViewerState::new(design);
    let before = state.positions.clone();
    state.begin_drag("api", Point::default());
    state.drag_to(Point::default());
    state.finish_drag(&path);
    assert_eq!(fs::read(&path).unwrap(), original_bytes);
    state.begin_drag("api", Point::default());
    state.drag_to(Point { x: 10.0, y: 20.0 });
    state.cancel_drag();
    assert_eq!(state.positions, before);
    assert_eq!(fs::read(&path).unwrap(), original_bytes);
    state.begin_drag("api", Point::default());
    state.drag_to(Point { x: 10.0, y: 20.0 });
    fs::write(&path, "invalid").unwrap();
    state.finish_drag(&path);
    assert_eq!(fs::read_to_string(&path).unwrap(), "invalid");
    assert_eq!(state.positions, before);
    assert!(state.status().contains("invalid JSON"));
}

#[test]
fn reload_preserves_selection_and_last_good_design_and_recovers() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("design.json");
    let mut state = ViewerState::new(example());
    state.selected = Some("nested".into());
    let before = state.design.clone();
    let positions = state.positions.clone();
    let time = state.last_reload;
    fs::write(&path, r#"{"version":99,"unexpected":true}"#).unwrap();
    state.reload(&path);
    assert_eq!(state.design, before);
    assert_eq!(state.positions, positions);
    assert_eq!(state.last_reload, time);
    assert_eq!(state.selected.as_deref(), Some("nested"));
    let status = state.status();
    assert!(status.contains("99"));
    assert!(status.contains("unexpected"));
    assert!(status.contains("Last reload"));
    sysy_core::save(&path, &before).unwrap();
    state.reload(&path);
    assert!(state.error.is_none());
    assert_eq!(state.selected.as_deref(), Some("nested"));
    let mut updated = before;
    updated.nodes.retain(|node| node.id != "nested");
    sysy_core::save(&path, &updated).unwrap();
    state.reload(&path);
    assert!(state.selected.is_none());
    assert!(!state.positions.contains_key("nested"));
    assert_eq!(state.design, updated);
}

#[test]
fn explicit_layout_changes_take_effect_and_new_elements_get_positions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("design.json");
    let mut state = ViewerState::new(example());
    state.selected = Some("persist".into());
    let original = state.positions.clone();
    let mut disk = state.design.clone();
    disk.nodes.push(Node {
        id: "added".into(),
        label: "Added".into(),
        kind: NodeKind::Service,
        container: Some("vpc".into()),
        description: None,
        tags: Vec::new(),
    });
    sysy_core::save(&path, &disk).unwrap();
    state.reload(&path);
    assert!(state.positions.contains_key("added"));
    for (id, entry) in original {
        assert_eq!(state.positions[&id], entry);
    }
    assert_eq!(state.selected.as_deref(), Some("persist"));
    disk.layout = Layout::new();
    disk.layout = layout(&disk);
    disk.layout.get_mut("shopper").unwrap().x = -1000.0;
    sysy_core::save(&path, &disk).unwrap();
    state.reload(&path);
    assert_eq!(state.positions, disk.layout);
}

#[test]
fn file_events_match_both_rename_paths_and_ignore_other_files_and_access() {
    let target = Path::new("/designs/design.json");
    for kind in [
        EventKind::Create(CreateKind::File),
        EventKind::Modify(ModifyKind::Data(DataChange::Content)),
        EventKind::Remove(RemoveKind::File),
        EventKind::Any,
    ] {
        assert!(event_matches(
            &Event::new(kind).add_path(target.into()),
            target
        ));
        assert!(!event_matches(
            &Event::new(kind).add_path("/designs/other.json".into()),
            target
        ));
    }
    for mode in [
        RenameMode::Both,
        RenameMode::From,
        RenameMode::To,
        RenameMode::Any,
    ] {
        let rename = Event::new(EventKind::Modify(ModifyKind::Name(mode)));
        assert!(event_matches(
            &rename
                .clone()
                .add_path("/designs/.temporary".into())
                .add_path(target.into()),
            target
        ));
        assert!(event_matches(
            &rename
                .clone()
                .add_path(target.into())
                .add_path("/designs/backup.json".into()),
            target
        ));
        assert!(!event_matches(
            &rename
                .add_path("/designs/.temporary".into())
                .add_path("/designs/other.json".into()),
            target
        ));
    }
    assert!(!event_matches(
        &Event::new(EventKind::Access(AccessKind::Read)).add_path(target.into()),
        target
    ));
    assert!(!event_matches(&Event::new(EventKind::Any), target));
    assert!(event_matches(
        &Event::new(EventKind::Any).add_path("design.json".into()),
        target
    ));
}

#[test]
fn ten_events_in_a_burst_yield_exactly_one_reload_after_the_last_event() {
    let start = Instant::now();
    let mut debounce = Debounce::default();
    assert!(!debounce.take_ready(start));
    for index in 0..10 {
        let time = start + Duration::from_millis(index * 10);
        debounce.record(time);
        assert!(!debounce.take_ready(time));
    }
    let deadline = start + Duration::from_millis(90) + DEBOUNCE;
    assert!(!debounce.take_ready(deadline.checked_sub(Duration::from_millis(1)).unwrap()));
    assert!(debounce.take_ready(deadline));
    assert!(!debounce.take_ready(deadline + DEBOUNCE));
    debounce.record(deadline);
    assert!(debounce.take_ready(deadline + DEBOUNCE));
}

#[test]
fn reload_does_not_shrink_unchanged_container_pins_after_layout_grew_them() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("design.json");
    let mut design = example();
    design.layout.insert(
        "vpc".into(),
        LayoutEntry {
            x: 0.0,
            y: 0.0,
            size: Some(sysy_core::Size {
                width: 1.0,
                height: 1.0,
            }),
        },
    );
    sysy_core::save(&path, &design).unwrap();
    let mut state = ViewerState::new(design);
    let frame = state.positions["vpc"];
    assert!(frame.size.unwrap().width > 1.0);
    state.begin_drag("shopper", Point::default());
    state.drag_to(Point { x: -10.0, y: -20.0 });
    state.finish_drag(&path);
    assert_eq!(state.positions["vpc"], frame);
    state.reload(&path);
    assert_eq!(state.positions["vpc"], frame);
}

#[test]
fn directory_watch_keeps_last_good_design_on_invalid_write_and_recovers() {
    use sysy_ui::interaction::LiveDesign;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("design.json");
    let design = example();
    sysy_core::save(&path, &design).unwrap();
    let mut viewer = LiveDesign::open(&path).unwrap();
    viewer.state.selected = Some("api".into());
    fs::write(&path, r#"{"version":99,"unexpected":true}"#).unwrap();
    poll_until(&mut viewer, |state| state.error.is_some());
    assert_eq!(viewer.state.design, design);
    assert_eq!(viewer.state.selected.as_deref(), Some("api"));
    assert!(viewer.state.status().contains("unexpected"));
    let mut updated = design;
    updated.title = "Recovered".into();
    sysy_core::save(&path, &updated).unwrap();
    poll_until(&mut viewer, |state| state.design.title == "Recovered");
    assert!(viewer.state.error.is_none());
    assert_eq!(viewer.state.design, updated);
}

#[test]
fn disk_updates_during_a_drag_are_merged_on_release() {
    use sysy_ui::interaction::LiveDesign;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("design.json");
    let mut disk = example();
    sysy_core::save(&path, &disk).unwrap();
    let mut viewer = LiveDesign::open(&path).unwrap();
    let original = viewer.state.positions.clone();
    viewer.state.begin_drag("api", Point::default());
    viewer.state.drag_to(Point { x: 23.0, y: -17.0 });
    disk.title = "Edited during drag".into();
    sysy_core::save(&path, &disk).unwrap();
    assert!(!viewer.poll(Instant::now() + DEBOUNCE));
    assert_ne!(viewer.state.design.title, disk.title);
    let changed = dragged_positions(&disk, &original, "api", Point { x: 23.0, y: -17.0 });
    viewer.finish_drag();
    merge_positions(&mut disk, &changed);
    assert_eq!(viewer.state.design, disk);
    assert_eq!(sysy_core::load(&path).unwrap(), disk);
    let last_reload = viewer.state.last_reload;
    poll_until(&mut viewer, |state| state.last_reload > last_reload);
    let mut expected = original;
    expected.extend(changed);
    assert_eq!(viewer.state.positions, expected);
}

fn poll_until(viewer: &mut sysy_ui::interaction::LiveDesign, ready: impl Fn(&ViewerState) -> bool) {
    let start = Instant::now();
    loop {
        viewer.poll(Instant::now());
        if ready(&viewer.state) {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "file watch did not reload: {:?}",
            viewer.state.error
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
