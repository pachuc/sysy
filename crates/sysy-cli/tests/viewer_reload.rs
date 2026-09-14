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
