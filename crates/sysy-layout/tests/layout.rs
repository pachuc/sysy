use std::path::PathBuf;

use sysy_core::{Design, Layout, LayoutEntry, Size, load};
use sysy_layout::{
    CONTAINER_PADDING, Point, Rect, bounding_box,
    geometry::{NODE_HEIGHT, NODE_MAX_WIDTH, NODE_MIN_WIDTH, element_rect},
    layout, node_size, note_size,
};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("{name}.json"))
}

fn assert_exact(actual: f64, expected: f64) {
    // Pins and deterministic size rules must agree exactly, including the sign
    // of zero; an approximate comparison would hide persistence changes.
    assert_eq!(actual.to_bits(), expected.to_bits());
}

fn inside(design: &Design, node: &sysy_core::Node, container: &str) -> bool {
    let mut parent = node.container.as_deref();
    while let Some(id) = parent {
        if id == container {
            return true;
        }
        parent = design
            .containers
            .iter()
            .find(|item| item.id == id)
            .unwrap()
            .parent
            .as_deref();
    }
    false
}

fn assert_padding(parent: Rect, child: Rect) {
    assert!(
        parent.contains(Rect::new(
            child.origin.x - CONTAINER_PADDING,
            child.origin.y - CONTAINER_PADDING,
            child.size.width + 2.0 * CONTAINER_PADDING,
            child.size.height + 2.0 * CONTAINER_PADDING,
        )),
        "parent {parent:?} does not pad child {child:?}"
    );
}

fn assert_geometry(design: &Design, result: &Layout) {
    for (index, node) in design.nodes.iter().enumerate() {
        let rect = element_rect(&node.id, &result[&node.id], design);
        assert!(result[&node.id].size.is_none());
        for other in &design.nodes[index + 1..] {
            assert!(
                !rect.intersects(element_rect(&other.id, &result[&other.id], design)),
                "nodes {} and {} overlap",
                node.id,
                other.id
            );
        }
        for container in &design.containers {
            let frame = element_rect(&container.id, &result[&container.id], design);
            if inside(design, node, &container.id) {
                assert_padding(frame, rect);
            } else {
                assert!(
                    !rect.intersects(frame),
                    "node {} overlaps unrelated container {}",
                    node.id,
                    container.id
                );
            }
        }
    }
    for container in &design.containers {
        assert!(result[&container.id].size.is_some());
        if let Some(parent) = &container.parent {
            assert_padding(
                element_rect(parent, &result[parent], design),
                element_rect(&container.id, &result[&container.id], design),
            );
        }
    }
    for (id, pin) in &design.layout {
        let actual = result[id];
        assert_eq!((actual.x, actual.y), (pin.x, pin.y), "pin {id} moved");
        if let Some(size) = pin.size {
            let actual = actual.size.unwrap();
            assert!(actual.width >= size.width && actual.height >= size.height);
        }
    }
    for note in &design.notes {
        assert!(result.contains_key(&note.id));
    }
    if let Some(bounds) = bounding_box(result, design) {
        for (id, entry) in result {
            assert!(bounds.contains(element_rect(id, entry, design)));
        }
    } else {
        assert!(result.is_empty());
    }
}

#[test]
fn fixtures_are_stable_valid_and_match_goldens() {
    let mut fixtures: Vec<_> = std::fs::read_dir(fixture("chain").parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
                && !path.to_string_lossy().ends_with(".layout.json")
        })
        .collect();
    fixtures.sort();
    assert!(fixtures.len() >= 6);
    for path in fixtures {
        let mut design = load(&path).unwrap();
        let original = design.clone();
        let expected = layout(&design);
        assert_geometry(&design, &expected);
        for _ in 0..10 {
            assert_eq!(layout(&design), expected, "{}", path.display());
        }
        assert_eq!(design, original);
        design.nodes.reverse();
        design.containers.reverse();
        design.edges.reverse();
        design.notes.reverse();
        assert_eq!(layout(&design), expected, "array order changed layout");
        let golden = path.with_extension("layout.json");
        let json = format!("{}\n", serde_json::to_string_pretty(&expected).unwrap());
        if std::env::var_os("SYSY_UPDATE_GOLDENS").is_some() {
            std::fs::write(&golden, &json).unwrap();
        }
        assert_eq!(
            std::fs::read_to_string(&golden).unwrap(),
            json,
            "{}",
            golden.display()
        );
    }
}

#[test]
fn unpinned_edges_flow_right_except_the_last_cycle_edge() {
    for name in [
        "chain",
        "fan-out",
        "diamond",
        "cycle",
        "nested",
        "container-edges",
    ] {
        let design = load(fixture(name)).unwrap();
        let result = layout(&design);
        for edge in &design.edges {
            if edge.id == "zz-close" {
                assert!(result[&edge.to].x < result[&edge.from].x);
            } else {
                assert!(
                    result[&edge.to].x > result[&edge.from].x,
                    "{name}: {}",
                    edge.id
                );
            }
        }
    }
}

#[test]
fn pinned_container_grows_and_a_second_layout_preserves_it() {
    let mut design = load(fixture("pinned-container")).unwrap();
    let result = layout(&design);
    let before = design.layout["box"].size.unwrap();
    let after = result["box"].size.unwrap();
    assert!(after.width > before.width && after.height > before.height);
    assert_geometry(&design, &result);
    design.layout = result.clone();
    assert_eq!(layout(&design), result);
}

#[test]
fn pinned_container_without_size_and_empty_container_preserve_origins() {
    let mut design = load(fixture("pinned-container")).unwrap();
    design.layout.get_mut("box").unwrap().size = None;
    assert_geometry(&design, &layout(&design));
    let mut design = load(fixture("nested")).unwrap();
    design.layout.insert(
        "empty".into(),
        LayoutEntry {
            x: 3000.0,
            y: 4000.0,
            size: Some(Size {
                width: 1000.0,
                height: 800.0,
            }),
        },
    );
    let result = layout(&design);
    assert_geometry(&design, &result);
    assert_eq!(result["empty"], design.layout["empty"]);
}

#[test]
fn barycenter_sweeps_remove_a_crossing() {
    let design = load(fixture("crossings")).unwrap();
    let result = layout(&design);
    assert_eq!(result["a"].y < result["b"].y, result["d"].y < result["c"].y);
}

#[test]
fn cycle_removal_uses_edge_id_instead_of_traversal_order() {
    let mut design = load(fixture("cycle")).unwrap();
    design
        .edges
        .iter_mut()
        .find(|edge| edge.id == "ab")
        .unwrap()
        .id = "zzz-middle".into();
    let result = layout(&design);
    assert!(result["b"].x < result["c"].x);
    assert!(result["c"].x < result["d"].x);
    assert!(result["d"].x < result["a"].x);
    let mut self_edge = design.edges[0].clone();
    self_edge.id = "self".into();
    self_edge.to.clone_from(&self_edge.from);
    design.edges.push(self_edge);
    assert_eq!(layout(&design), result);
    for edge in &mut design.edges {
        edge.bidirectional = true;
    }
    assert_eq!(layout(&design), result);
}

#[test]
fn saved_edge_positions_are_preserved_and_anchor_notes() {
    let mut design = load(fixture("notes")).unwrap();
    design.layout.insert(
        "ab".into(),
        LayoutEntry {
            x: -500.0,
            y: -400.0,
            size: None,
        },
    );
    let result = layout(&design);
    assert_eq!(result["ab"], design.layout["ab"]);
    assert_exact(result["on-edge"].x, -500.0 + sysy_layout::NOTE_GAP);
    assert_geometry(&design, &result);
}

#[test]
fn empty_pinned_containers_do_not_grow_without_contents() {
    let mut design = load(fixture("nested")).unwrap();
    design.layout.insert(
        "empty".into(),
        LayoutEntry {
            x: 3000.0,
            y: 4000.0,
            size: Some(Size {
                width: 10.0,
                height: 10.0,
            }),
        },
    );
    let result = layout(&design);
    assert_eq!(result["empty"], design.layout["empty"]);
    assert_geometry(&design, &result);
}

#[test]
fn generated_graphs_keep_container_groups_clear() {
    for seed in 0..24 {
        let mut design = load(fixture("nested")).unwrap();
        design.edges.clear();
        for index in 0..24 {
            let mut node = design.nodes[0].clone();
            node.id = format!("generated-{index}");
            node.label = "Long node label ".repeat((index % 4) + 1);
            node.container = [
                None,
                Some("inner"),
                Some("middle"),
                Some("outer"),
                Some("other"),
            ][(index + seed) % 5]
                .map(str::to_owned);
            design.nodes.push(node);
        }
        for index in 0..23 {
            design.edges.push(sysy_core::Edge {
                id: format!("edge-{index}"),
                from: format!("generated-{index}"),
                to: format!("generated-{}", index + 1),
                kind: sysy_core::EdgeKind::Sync,
                label: None,
                bidirectional: false,
            });
        }
        assert!(design.validate().is_empty());
        let result = layout(&design);
        assert_geometry(&design, &result);
        for edge in &design.edges {
            assert!(result[&edge.from].x < result[&edge.to].x);
        }
    }
}

#[test]
fn notes_use_attachments_and_a_separate_column() {
    let design = load(fixture("notes")).unwrap();
    let result = layout(&design);
    for (note, target) in [("on-node", "a"), ("on-container", "box")] {
        assert_exact(
            result[note].x,
            element_rect(target, &result[target], &design).right() + sysy_layout::NOTE_GAP,
        );
    }
    assert_exact(result["free-a"].x, result["free-b"].x);
    assert!(result["free-b"].y > result["free-a"].y);
    assert!(result["free-a"].x > result["b"].x + node_size("b").width);
    for note in &design.notes {
        let rect = element_rect(&note.id, &result[&note.id], &design);
        for other in design.notes.iter().filter(|other| other.id != note.id) {
            assert!(!rect.intersects(element_rect(&other.id, &result[&other.id], &design)));
        }
    }
}

#[test]
fn geometry_handles_borders_negative_coordinates_and_empty_layouts() {
    let rect = Rect::new(-10.0, -20.0, 100.0, 80.0);
    assert!(rect.contains(rect));
    assert!(rect.contains_point(Point { x: -10.0, y: 60.0 }));
    assert!(!rect.contains_point(Point { x: -11.0, y: 0.0 }));
    assert!(!rect.intersects(Rect::new(90.0, -20.0, 10.0, 10.0)));
    assert!(rect.intersects(Rect::new(89.0, -20.0, 10.0, 10.0)));
    assert!(!rect.intersects(Rect::new(0.0, 0.0, 0.0, 0.0)));
    assert_eq!(
        rect.union(Rect::new(-20.0, -30.0, 10.0, 10.0)),
        Rect::new(-20.0, -30.0, 110.0, 90.0)
    );
    assert_eq!(bounding_box(&Layout::new(), &Design::default()), None);
}

#[test]
fn sizes_are_bounded_and_count_unicode_characters() {
    assert_eq!(
        node_size(""),
        Size {
            width: NODE_MIN_WIDTH,
            height: NODE_HEIGHT
        }
    );
    assert_exact(node_size(&"x".repeat(1000)).width, NODE_MAX_WIDTH);
    assert_eq!(node_size(&"é".repeat(20)), node_size(&"a".repeat(20)));
    assert_exact(node_size(&"a".repeat(20)).width, 224.0);
    assert!(note_size("first\nsecond").height > note_size("first").height);
    assert!(note_size(&"x".repeat(29)).height > note_size(&"x".repeat(28)).height);
}

#[test]
fn acceptance_examples_have_stable_contained_nodes_and_disjoint_edge_labels() {
    for name in ["swarmy", "checkout"] {
        let mut design = load(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../examples/{name}.json")),
        )
        .unwrap();
        for reset in [false, true] {
            if reset {
                design.layout.clear();
            }
            let positions = layout(&design);
            assert_geometry(&design, &positions);
            let edges = sysy_layout::edges::edge_geometry(&design, &positions);
            for (id, edge) in &edges {
                let Some(rect) = edge.label else { continue };
                for (other_id, other) in &edges {
                    if id != other_id {
                        assert!(
                            !other.label.is_some_and(|other| other.intersects(rect)),
                            "{name}: labels {id} and {other_id} overlap"
                        );
                    }
                }
                for node in &design.nodes {
                    assert!(
                        !rect.intersects(element_rect(&node.id, &positions[&node.id], &design)),
                        "{name}: label {id} overlaps node {}",
                        node.id
                    );
                }
            }
            design.nodes.reverse();
            design.containers.reverse();
            design.edges.reverse();
            design.notes.reverse();
            assert_eq!(layout(&design), positions);
            assert_eq!(
                sysy_layout::edges::edge_geometry(&design, &positions),
                edges
            );
        }
    }
}

#[test]
fn adding_a_node_to_a_saved_nested_container_grows_its_frame() {
    let mut design =
        load(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/checkout.json"))
            .unwrap();
    design.layout = layout(&design);
    let before = design.layout.clone();
    let mut node = design
        .nodes
        .iter()
        .find(|node| node.id == "payment")
        .unwrap()
        .clone();
    node.id = "payment-reconciler".into();
    node.label = "Payment reconciler".into();
    design.add_node(node).unwrap();
    let result = layout(&design);
    assert_geometry(&design, &result);
    let old = before["payments"].size.unwrap();
    let new = result["payments"].size.unwrap();
    assert!(new.width > old.width || new.height > old.height);
    design.layout = result.clone();
    assert_eq!(layout(&design), result);
}

#[test]
fn labels_reserve_space_for_the_client_icon_before_reaching_the_width_cap() {
    for label in ["swarmy CLI", "Shopper browser", "Payment reconciler"] {
        let estimated_text = label.chars().fold(0.0, |width, _| width + 8.0);
        assert!(node_size(label).width >= estimated_text + 54.0);
    }
}
