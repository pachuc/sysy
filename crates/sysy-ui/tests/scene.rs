use sysy_core::{Container, Design, Edge, EdgeKind, LayoutEntry, Node, NodeKind, Note};
use sysy_layout::{Point, Rect, Size, geometry::element_rect, layout};
use sysy_ui::scene::{
    Camera, Primitive, Scene, ShapeKind, boundary, detail_text, highlighted, line_segments,
};

fn example() -> Design {
    sysy_core::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../sysy-core/tests/fixtures/checkout.json"
    ))
    .unwrap()
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8, "{a} != {b}");
}

fn center(rect: Rect) -> Point {
    Point {
        x: rect.origin.x + rect.size.width / 2.0,
        y: rect.origin.y + rect.size.height / 2.0,
    }
}

fn on_boundary(rect: Rect, p: Point) {
    assert!(rect.contains_point(p));
    assert!(
        (p.x - rect.origin.x).abs() < 1e-8
            || (p.x - rect.right()).abs() < 1e-8
            || (p.y - rect.origin.y).abs() < 1e-8
            || (p.y - rect.bottom()).abs() < 1e-8
    );
    assert_ne!(p, center(rect));
}

#[test]
fn checkout_has_one_shape_per_element_and_clipped_edges() {
    let mut design = example();
    design.notes.push(Note {
        id: "remark".into(),
        text: "Keep orders durable".into(),
        on: Some("orders".into()),
    });
    let positions = layout(&design);
    let scene = Scene::build(&design, &positions);
    assert_eq!(
        scene.shapes.len(),
        design.nodes.len() + design.containers.len() + design.edges.len() + design.notes.len()
    );
    for node in &design.nodes {
        assert_eq!(
            scene.shapes.iter().find(|s| s.id == node.id).unwrap().kind,
            ShapeKind::Node(node.kind)
        );
    }
    assert_eq!(scene.shapes[0].kind, ShapeKind::Container);
    let note = scene.shapes.last().unwrap();
    assert_eq!(note.kind, ShapeKind::Note);
    assert_eq!(scene.hit_test(center(note.bounds), 3.0), Some("remark"));
    for edge in &design.edges {
        let shape = scene.shapes.iter().find(|s| s.id == edge.id).unwrap();
        assert_eq!(shape.kind, ShapeKind::Edge(edge.kind));
        on_boundary(
            element_rect(&edge.from, &positions[&edge.from], &design),
            shape.route[0],
        );
        on_boundary(
            element_rect(&edge.to, &positions[&edge.to], &design),
            *shape.route.last().unwrap(),
        );
        assert_eq!(shape.arrowheads.len(), 1);
        assert_eq!(shape.arrowheads[0][0], *shape.route.last().unwrap());
    }
    let edge = scene.shapes.iter().find(|s| s.id == "place-order").unwrap();
    let label = &edge.labels[0];
    assert_eq!(label.text, "POST /orders");
    close(
        center(label.rect).x,
        edge.route[0].x.midpoint(edge.route[1].x),
    );
    close(
        center(label.rect).y,
        edge.route[0].y.midpoint(edge.route[1].y),
    );
}

#[test]
fn nested_frames_and_overlapping_nodes_hit_in_paint_order() {
    let mut design = example();
    design.containers.push(Container {
        id: "inner".into(),
        label: "Inner".into(),
        parent: Some("vpc".into()),
        description: None,
    });
    design.nodes[0].container = Some("inner".into());
    let positions = layout(&design);
    let mut scene = Scene::build(&design, &positions);
    assert_eq!(scene.shapes[0].id, "vpc");
    assert_eq!(scene.shapes[1].id, "inner");
    let api = scene.shapes.iter().find(|s| s.id == "api").unwrap().clone();
    assert_eq!(scene.hit_test(center(api.bounds), 3.0), Some("api"));
    let inner = &scene.shapes[1];
    assert_eq!(
        scene.hit_test(
            Point {
                x: inner.bounds.origin.x + 2.0,
                y: inner.bounds.origin.y + 2.0
            },
            3.0
        ),
        Some("inner")
    );
    let mut overlay = api.clone();
    overlay.id = "topmost".into();
    scene.shapes.push(overlay);
    assert_eq!(scene.hit_test(center(api.bounds), 3.0), Some("topmost"));
    assert_eq!(
        scene.hit_test(
            Point {
                x: -10000.0,
                y: -10000.0
            },
            3.0
        ),
        None
    );
}

#[test]
fn fit_encloses_every_shape_with_margin_and_handles_empty_and_large_designs() {
    let design = example();
    let scene = Scene::build(&design, &layout(&design));
    for bounds in [scene.bounds.unwrap(), Rect::new(-1e8, -2e8, 3e8, 4e8)] {
        let viewport = Size {
            width: 900.0,
            height: 600.0,
        };
        let camera = Camera::fit(Some(bounds), viewport, 40.0);
        let top = camera.world_to_screen(bounds.origin);
        let bottom = camera.world_to_screen(Point {
            x: bounds.right(),
            y: bounds.bottom(),
        });
        assert!(top.x >= 40.0 - 1e-8 && top.y >= 40.0 - 1e-8);
        assert!(
            bottom.x <= viewport.width - 40.0 + 1e-8 && bottom.y <= viewport.height - 40.0 + 1e-8
        );
    }
    assert_eq!(
        Camera::fit(
            None,
            Size {
                width: 0.0,
                height: 0.0
            },
            40.0
        ),
        Camera::default()
    );
    let camera = Camera::fit(
        Some(Rect::new(0.0, 0.0, 0.0, 0.0)),
        Size {
            width: 0.0,
            height: 0.0,
        },
        40.0,
    );
    assert!(camera.scale.is_finite() && camera.pan.x.is_finite());
}

#[test]
fn zoom_keeps_cursor_world_position_and_pan_round_trips() {
    let mut camera = Camera {
        pan: Point { x: -150.0, y: 35.0 },
        scale: 0.7,
    };
    let cursor = Point { x: 270.0, y: 80.0 };
    let world = camera.screen_to_world(cursor);
    for factor in [1.2, 0.2, 1000.0, 0.001] {
        camera.zoom_at(cursor, factor);
        let actual = camera.world_to_screen(world);
        close(actual.x, cursor.x);
        close(actual.y, cursor.y);
    }
    camera.pan_by(Point { x: 21.0, y: -14.0 });
    let screen = camera.world_to_screen(world);
    close(screen.x, cursor.x + 21.0);
    close(screen.y, cursor.y - 14.0);
    close(camera.screen_to_world(screen).x, world.x);
}

#[test]
fn every_node_kind_has_geometry_and_a_kind_label() {
    let mut design = Design::default();
    for kind in [
        NodeKind::Service,
        NodeKind::Database,
        NodeKind::Queue,
        NodeKind::Cache,
        NodeKind::Storage,
        NodeKind::Client,
        NodeKind::External,
        NodeKind::Function,
        NodeKind::Generic,
    ] {
        let name = sysy_ui::scene::node_kind_name(kind);
        design.nodes.push(Node {
            id: name.into(),
            kind,
            label: name.into(),
            description: None,
            tags: Vec::new(),
            container: None,
        });
    }
    let scene = Scene::build(&design, &layout(&design));
    assert_eq!(scene.shapes.len(), 9);
    for shape in &scene.shapes {
        assert!(!shape.primitives.is_empty());
        assert_eq!(shape.labels[1].text, shape.id);
        match shape.kind {
            ShapeKind::Node(NodeKind::Database) => {
                assert!(matches!(shape.primitives[0], Primitive::Polygon(_)));
            }
            ShapeKind::Node(NodeKind::Client) => assert!(
                shape
                    .primitives
                    .iter()
                    .all(|p| matches!(p, Primitive::Stroke { .. }))
            ),
            ShapeKind::Node(NodeKind::Queue) => assert_eq!(shape.primitives.len(), 3),
            ShapeKind::Node(NodeKind::External) => assert!(shape.primitives.len() > 10),
            _ => assert_eq!(shape.primitives.len(), 1),
        }
    }
}

#[test]
fn line_styles_arrowheads_container_endpoints_and_edge_hits() {
    let mut design = example();
    design.edges = vec![Edge {
        id: "container-link".into(),
        from: "shopper".into(),
        to: "vpc".into(),
        kind: EdgeKind::Async,
        label: Some("delivery".into()),
        bidirectional: true,
    }];
    let positions = layout(&design);
    let scene = Scene::build(&design, &positions);
    let edge = scene
        .shapes
        .iter()
        .find(|s| s.id == "container-link")
        .unwrap();
    on_boundary(
        element_rect("vpc", &positions["vpc"], &design),
        edge.route[1],
    );
    assert_eq!(edge.arrowheads.len(), 2);
    assert_eq!(edge.arrowheads[1][0], edge.route[0]);
    assert_eq!(
        scene.hit_test(center(edge.labels[0].rect), 3.0),
        Some("container-link")
    );
    let start = edge.route[0];
    let end = edge.route[1];
    let near_start = Point {
        x: start.x + (end.x - start.x) * 0.2,
        y: start.y + (end.y - start.y) * 0.2,
    };
    assert_eq!(scene.hit_test(near_start, 3.0), Some("container-link"));
    assert!(highlighted(edge, Some("shopper")));
    assert!(highlighted(edge, Some("vpc")));
    assert!(!highlighted(edge, Some("orders")));
    assert!(!highlighted(edge, None));
    let a = Point { x: 0.0, y: 0.0 };
    let b = Point { x: 100.0, y: 0.0 };
    assert_eq!(line_segments(a, b, EdgeKind::Sync), vec![vec![a, b]]);
    assert_eq!(line_segments(a, b, EdgeKind::Data), vec![vec![a, b]]);
    let dashed = line_segments(a, b, EdgeKind::Async);
    let dotted = line_segments(a, b, EdgeKind::Dependency);
    close(dashed[0][1].x, 10.0);
    close(dotted[0][1].x, 2.0);
    assert!(dotted.len() > dashed.len());
    assert!(line_segments(a, a, EdgeKind::Async).is_empty());
    let data = Scene::build(&example(), &layout(&example()));
    let edge = data.shapes.iter().find(|s| s.id == "persist").unwrap();
    assert!(matches!(edge.primitives[0], Primitive::Stroke { width, .. } if width > 3.0));
}

#[test]
fn clipping_handles_vertical_diagonal_and_self_edges() {
    let rect = Rect::new(10.0, 20.0, 100.0, 60.0);
    for toward in [
        Point { x: 60.0, y: -100.0 },
        Point { x: 60.0, y: 1000.0 },
        Point {
            x: -100.0,
            y: -100.0,
        },
        center(rect),
    ] {
        on_boundary(rect, boundary(rect, toward));
    }
    let mut design = example();
    design.edges[0].to = design.edges[0].from.clone();
    let scene = Scene::build(&design, &layout(&design));
    let edge = scene.shapes.iter().find(|s| s.id == "persist").unwrap();
    assert!(edge.route.len() > 2);
    assert_ne!(edge.route.first(), edge.route.last());
    assert!(scene.bounds.unwrap().contains(edge.bounds));
}

#[test]
fn details_include_metadata_direction_connections_and_note_attachment() {
    let mut design = example();
    design.nodes[0].description = Some("Accepts orders".into());
    design.nodes[0].tags = vec!["public".into(), "http".into()];
    design.edges[0].bidirectional = true;
    design.notes.push(Note {
        id: "note".into(),
        text: "Review".into(),
        on: Some("api".into()),
    });
    let fields = detail_text(&design, "api").unwrap();
    let get = |name| {
        fields
            .iter()
            .find(|(key, _)| key == name)
            .unwrap()
            .1
            .as_str()
    };
    assert_eq!(get("Id"), "api");
    assert_eq!(get("Kind"), "service");
    assert_eq!(get("Label"), "Checkout API");
    assert_eq!(get("Description"), "Accepts orders");
    assert_eq!(get("Tags"), "public, http");
    assert!(get("Connections").contains("api ↔ orders (data)"));
    assert!(get("Connections").contains("POST /orders"));
    assert!(
        detail_text(&design, "persist")
            .unwrap()
            .contains(&("Direction".into(), "bidirectional".into()))
    );
    assert!(
        detail_text(&design, "note")
            .unwrap()
            .contains(&("Attached to".into(), "api".into()))
    );
    assert!(
        detail_text(&design, "vpc")
            .unwrap()
            .contains(&("Kind".into(), "container".into()))
    );
    assert!(detail_text(&design, "missing").is_none());
}

#[test]
fn saved_positions_are_used_without_mutating_the_design() {
    let mut design = example();
    design.layout.insert(
        "shopper".into(),
        LayoutEntry {
            x: -500.0,
            y: 400.0,
            size: None,
        },
    );
    let before = design.clone();
    let scene = Scene::build(&design, &layout(&design));
    assert_eq!(
        scene
            .shapes
            .iter()
            .find(|s| s.id == "shopper")
            .unwrap()
            .bounds
            .origin,
        Point {
            x: -500.0,
            y: 400.0
        }
    );
    assert_eq!(design, before);
    assert!(
        Scene::build(&Design::default(), &sysy_core::Layout::default())
            .shapes
            .is_empty()
    );
}

#[test]
fn swarmy_example_preserves_every_element_and_separates_parallel_connections() {
    let design = sysy_core::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/swarmy.json"
    ))
    .unwrap();
    let positions = layout(&design);
    let scene = Scene::build(&design, &positions);
    assert_eq!(
        scene.shapes.len(),
        design.containers.len() + design.nodes.len() + design.edges.len() + design.notes.len()
    );
    for edge in &design.edges {
        let shape = scene.shapes.iter().find(|s| s.id == edge.id).unwrap();
        on_boundary(
            element_rect(&edge.from, &positions[&edge.from], &design),
            shape.route[0],
        );
        on_boundary(
            element_rect(&edge.to, &positions[&edge.to], &design),
            *shape.route.last().unwrap(),
        );
        assert_eq!(shape.labels[0].text, *edge.label.as_ref().unwrap());
    }
    for (first, second) in [
        ("dispatch", "enqueue"),
        ("enqueue", "publish"),
        ("events", "lease"),
        ("reap", "scan"),
    ] {
        let a = scene.shapes.iter().find(|s| s.id == first).unwrap();
        let b = scene.shapes.iter().find(|s| s.id == second).unwrap();
        assert_ne!(center(a.labels[0].rect), center(b.labels[0].rect));
    }
    let camera = Camera::fit(
        scene.bounds,
        Size {
            width: 1000.0,
            height: 700.0,
        },
        32.0,
    );
    for shape in &scene.shapes {
        let top = camera.world_to_screen(shape.bounds.origin);
        let bottom = camera.world_to_screen(Point {
            x: shape.bounds.right(),
            y: shape.bounds.bottom(),
        });
        assert!(top.x >= 32.0 - 1e-8 && top.y >= 32.0 - 1e-8);
        assert!(bottom.x <= 968.0 + 1e-8 && bottom.y <= 668.0 + 1e-8);
    }
}
