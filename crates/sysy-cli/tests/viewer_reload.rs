use std::{
    process::Command,
    thread,
    time::{Duration, Instant},
};

use sysy_ui::interaction::LiveDesign;

#[test]
fn cli_atomic_saves_reload_the_viewer_within_one_second_without_a_window() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("design.json");
    sysy_core::save(
        &path,
        &sysy_core::Design {
            title: "Live design".into(),
            ..Default::default()
        },
    )
    .unwrap();
    let mut viewer = LiveDesign::open(&path).unwrap();
    // Repeated replacements verify that the directory watch survives atomic saves.
    for id in ["first", "second", "third"] {
        let start = Instant::now();
        let output = Command::new(env!("CARGO_BIN_EXE_sysy"))
            .args(["--json", "node", "add"])
            .arg(&path)
            .args([id, "--kind", "service", "--label", id])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected = sysy_core::load(&path).unwrap();
        loop {
            viewer.poll(Instant::now());
            if viewer.state.design == expected {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(1),
                "viewer did not reload CLI edit: {:?}",
                viewer.state.error
            );
            thread::sleep(Duration::from_millis(10));
        }
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(viewer.state.positions.contains_key(id));
    }
}

#[test]
fn checkout_drags_merge_cli_edits_and_reload_nested_container_growth() {
    use sysy_layout::{Point, geometry::element_rect};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("checkout.json");
    let design = sysy_core::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/checkout.json"
    ))
    .unwrap();
    sysy_core::save(&path, &design).unwrap();
    let mut viewer = LiveDesign::open(&path).unwrap();
    for id in ["shopper", "fulfillment", "processor"] {
        let before = viewer.state.positions[id];
        viewer.state.begin_drag(id, Point::default());
        viewer.state.drag_to(Point { x: 20.0, y: 60.0 });
        cli_edit(&path, &["set"], &["--description", "Edited while dragging"]);
        viewer.finish_drag();
        assert!(viewer.state.error.is_none());
        let saved = sysy_core::load(&path).unwrap();
        assert_eq!(saved.description.as_deref(), Some("Edited while dragging"));
        assert!((saved.layout[id].x - before.x - 20.0).abs() < 1e-8);
        assert!((saved.layout[id].y - before.y - 60.0).abs() < 1e-8);
    }
    let old = viewer.state.positions["payments"];
    cli_edit(
        &path,
        &["node", "add"],
        &[
            "reconciler",
            "--kind",
            "service",
            "--label",
            "Payment reconciler",
            "--in",
            "payments",
        ],
    );
    let expected = sysy_core::load(&path).unwrap();
    let start = Instant::now();
    while viewer.state.design != expected {
        viewer.poll(Instant::now());
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "CLI edit was not reloaded"
        );
        thread::sleep(Duration::from_millis(10));
    }
    let positions = &viewer.state.positions;
    let frame = element_rect("payments", &positions["payments"], &viewer.state.design);
    assert!(frame.contains(element_rect(
        "reconciler",
        &positions["reconciler"],
        &viewer.state.design
    )));
    let old_size = old.size.unwrap();
    assert!(frame.size.width > old_size.width || frame.size.height > old_size.height);
    cli_edit(&path, &["layout"], &[]);
    let reopened = LiveDesign::open(&path).unwrap();
    let saved = sysy_core::load(&path).unwrap();
    for id in ["shopper", "fulfillment", "processor"] {
        assert_eq!(reopened.state.positions[id], saved.layout[id]);
    }
}

fn cli_edit(path: &std::path::Path, command: &[&str], args: &[&str]) {
    let output = Command::new(env!("CARGO_BIN_EXE_sysy"))
        .arg("--json")
        .args(command)
        .arg(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
