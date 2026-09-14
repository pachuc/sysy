//! Drawing and interaction in world coordinates, without a window.

use sysy_core::{Design, EdgeKind, Layout, NodeKind};
use sysy_layout::geometry::{Point, Rect, Size, element_rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    Container,
    Node(NodeKind),
    Edge(EdgeKind),
    Note,
}

#[derive(Clone, Debug)]
pub enum Primitive {
    Box {
        rect: Rect,
        radius: f64,
        filled: bool,
        border: bool,
    },
    Stroke {
        points: Vec<Point>,
        width: f64,
    },
    Polygon(Vec<Point>),
}

#[derive(Clone, Debug)]
pub struct Label {
    pub text: String,
    pub rect: Rect,
    pub font_size: f64,
    pub centered: bool,
}

#[derive(Clone, Debug)]
pub struct Shape {
    pub id: String,
    pub kind: ShapeKind,
    pub bounds: Rect,
    pub primitives: Vec<Primitive>,
    pub labels: Vec<Label>,
    /// The continuous edge route also makes gaps between dashes clickable.
    pub route: Vec<Point>,
    pub arrowheads: Vec<[Point; 3]>,
    pub endpoints: Option<(String, String)>,
}

#[derive(Clone, Debug, Default)]
pub struct Scene {
    /// Paint order: parents, nested frames, edges, nodes, then notes.
    pub shapes: Vec<Shape>,
    pub bounds: Option<Rect>,
}

fn point(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn center(rect: Rect) -> Point {
    point(
        rect.origin.x + rect.size.width / 2.0,
        rect.origin.y + rect.size.height / 2.0,
    )
}

fn stroke(points: Vec<Point>, width: f64) -> Primitive {
    Primitive::Stroke { points, width }
}

fn box_shape(id: &str, kind: ShapeKind, bounds: Rect) -> Shape {
    Shape {
        id: id.to_owned(),
        kind,
        bounds,
        primitives: Vec::new(),
        labels: Vec::new(),
        route: Vec::new(),
        arrowheads: Vec::new(),
        endpoints: None,
    }
}

fn label(text: &str, rect: Rect, font_size: f64, centered: bool) -> Label {
    Label {
        text: text.to_owned(),
        rect,
        font_size,
        centered,
    }
}

fn rectangle(rect: Rect, filled: bool, border: bool) -> Primitive {
    Primitive::Box {
        rect,
        radius: 8.0,
        filled,
        border,
    }
}

fn inset(rect: Rect, amount: f64) -> Rect {
    Rect::new(
        rect.origin.x + amount,
        rect.origin.y + amount,
        (rect.size.width - 2.0 * amount).max(1.0),
        (rect.size.height - 2.0 * amount).max(1.0),
    )
}

/// Intersect the ray from the rectangle's center toward `toward` with its boundary.
#[must_use]
pub fn boundary(rect: Rect, toward: Point) -> Point {
    let c = center(rect);
    let dx = toward.x - c.x;
    let dy = toward.y - c.y;
    if dx.abs() + dy.abs() < f64::EPSILON {
        return point(rect.right(), c.y);
    }
    let tx = if dx.abs() < f64::EPSILON {
        f64::INFINITY
    } else {
        rect.size.width / (2.0 * dx.abs())
    };
    let ty = if dy.abs() < f64::EPSILON {
        f64::INFINITY
    } else {
        rect.size.height / (2.0 * dy.abs())
    };
    let t = tx.min(ty);
    point(c.x + dx * t, c.y + dy * t)
}

fn distance(a: Point, b: Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

fn along(a: Point, b: Point, t: f64) -> Point {
    point(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

/// Split a line into visible dashes in world units. Solid lines have one segment.
#[must_use]
pub fn line_segments(a: Point, b: Point, kind: EdgeKind) -> Vec<Vec<Point>> {
    let (dash, gap) = match kind {
        EdgeKind::Sync | EdgeKind::Data => return vec![vec![a, b]],
        EdgeKind::Async => (10.0, 6.0),
        EdgeKind::Dependency => (2.0, 5.0),
    };
    let length = distance(a, b);
    let mut segments = Vec::new();
    let mut offset = 0.0;
    // Bound work for extreme, but valid, saved coordinates.
    let unit = (length / 100_000.0).max(1.0);
    while offset < length {
        segments.push(vec![
            along(a, b, offset / length),
            along(a, b, (offset + dash * unit).min(length) / length),
        ]);
        offset += (dash + gap) * unit;
    }
    segments
}

fn arrow(tail: Point, tip: Point) -> [Point; 3] {
    let length = distance(tail, tip).max(0.001);
    let dx = (tip.x - tail.x) / length;
    let dy = (tip.y - tail.y) / length;
    [
        tip,
        point(tip.x - 10.0 * dx - 4.5 * dy, tip.y - 10.0 * dy + 4.5 * dx),
        point(tip.x - 10.0 * dx + 4.5 * dy, tip.y - 10.0 * dy - 4.5 * dx),
    ]
}

fn points_bounds(points: &[Point]) -> Rect {
    points
        .iter()
        .map(|p| Rect::new(p.x, p.y, 0.0, 0.0))
        .reduce(Rect::union)
        .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0))
}

fn edge_shape(edge: &sysy_core::Edge, from: Rect, to: Rect, offset: f64) -> Shape {
    let route = if edge.from == edge.to || distance(center(from), center(to)) < 0.001 {
        vec![
            point(from.right(), center(from).y),
            point(from.right() + 32.0, center(from).y),
            point(from.right() + 32.0, from.origin.y - 24.0),
            point(center(to).x, from.origin.y - 24.0),
            point(center(to).x, to.origin.y),
        ]
    } else if offset.abs() > f64::EPSILON {
        let a = center(from);
        let b = center(to);
        let length = distance(a, b);
        // Use a canonical direction so reverse edges get distinct routes too.
        let direction = if edge.from < edge.to { 1.0 } else { -1.0 };
        let bend = point(
            a.x.midpoint(b.x) - (b.y - a.y) / length * offset * direction,
            a.y.midpoint(b.y) + (b.x - a.x) / length * offset * direction,
        );
        vec![boundary(from, bend), bend, boundary(to, bend)]
    } else {
        vec![boundary(from, center(to)), boundary(to, center(from))]
    };
    let mut shape = box_shape(&edge.id, ShapeKind::Edge(edge.kind), points_bounds(&route));
    let width = if edge.kind == EdgeKind::Data {
        3.5
    } else {
        1.8
    };
    for pair in route.windows(2) {
        shape.primitives.extend(
            line_segments(pair[0], pair[1], edge.kind)
                .into_iter()
                .map(|points| stroke(points, width)),
        );
    }
    shape
        .arrowheads
        .push(arrow(route[route.len() - 2], route[route.len() - 1]));
    if edge.bidirectional {
        shape.arrowheads.push(arrow(route[1], route[0]));
    }
    for head in &shape.arrowheads {
        shape.bounds = shape.bounds.union(points_bounds(head));
        shape.primitives.push(Primitive::Polygon(head.to_vec()));
    }
    if let Some(text) = edge.label.as_deref().filter(|s| !s.is_empty()) {
        let anchor = route_midpoint(&route);
        let width = text.chars().take(40).fold(16.0_f64, |w, _| w + 7.0);
        let rect = Rect::new(anchor.x - width / 2.0, anchor.y - 12.0, width, 24.0);
        shape.bounds = shape.bounds.union(rect);
        shape.primitives.push(rectangle(rect, true, false));
        shape.labels.push(label(text, inset(rect, 3.0), 12.0, true));
    }
    shape.route = route;
    shape.endpoints = Some((edge.from.clone(), edge.to.clone()));
    shape
}

fn route_midpoint(route: &[Point]) -> Point {
    let mut remaining = route.windows(2).map(|p| distance(p[0], p[1])).sum::<f64>() / 2.0;
    for pair in route.windows(2) {
        let length = distance(pair[0], pair[1]);
        if remaining <= length {
            return along(pair[0], pair[1], remaining / length.max(0.001));
        }
        remaining -= length;
    }
    route.first().copied().unwrap_or_default()
}

fn arc(c: Point, rx: f64, ry: f64, start: f64, end: f64) -> Vec<Point> {
    (0..=32)
        .map(|i| {
            let angle = start + (end - start) * f64::from(i) / 32.0;
            point(c.x + rx * angle.cos(), c.y + ry * angle.sin())
        })
        .collect()
}

fn database(shape: &mut Shape) {
    let r = shape.bounds;
    let x = center(r).x;
    let top = point(x, r.origin.y + 9.0);
    let bottom = point(x, r.bottom() - 9.0);
    let mut outline = arc(
        top,
        r.size.width / 2.0,
        9.0,
        std::f64::consts::PI,
        std::f64::consts::TAU,
    );
    outline.extend(arc(
        bottom,
        r.size.width / 2.0,
        9.0,
        0.0,
        std::f64::consts::PI,
    ));
    outline.push(outline[0]);
    shape.primitives.push(Primitive::Polygon(outline.clone()));
    shape.primitives.push(stroke(outline, 1.5));
    shape.primitives.push(stroke(
        arc(top, r.size.width / 2.0, 9.0, 0.0, std::f64::consts::PI),
        1.5,
    ));
}

fn client(shape: &mut Shape) {
    let r = shape.bounds;
    let x = r.origin.x + 22.0;
    shape.primitives.push(stroke(
        arc(
            point(x, r.origin.y + 12.0),
            8.0,
            8.0,
            0.0,
            std::f64::consts::TAU,
        ),
        1.8,
    ));
    shape.primitives.push(stroke(
        vec![
            point(x - 16.0, r.bottom() - 4.0),
            point(x - 16.0, r.origin.y + 36.0),
            point(x - 9.0, r.origin.y + 25.0),
            point(x + 9.0, r.origin.y + 25.0),
            point(x + 16.0, r.origin.y + 36.0),
            point(x + 16.0, r.bottom() - 4.0),
            point(x - 16.0, r.bottom() - 4.0),
        ],
        1.8,
    ));
}

fn node_shape(node: &sysy_core::Node, rect: Rect) -> Shape {
    let mut shape = box_shape(&node.id, ShapeKind::Node(node.kind), rect);
    match node.kind {
        NodeKind::Database => database(&mut shape),
        NodeKind::Client => client(&mut shape),
        NodeKind::External => {
            shape.primitives.push(rectangle(rect, true, false));
            let corners = [
                rect.origin,
                point(rect.right(), rect.origin.y),
                point(rect.right(), rect.bottom()),
                point(rect.origin.x, rect.bottom()),
                rect.origin,
            ];
            for pair in corners.windows(2) {
                shape.primitives.extend(
                    line_segments(pair[0], pair[1], EdgeKind::Async)
                        .into_iter()
                        .map(|p| stroke(p, 1.5)),
                );
            }
        }
        NodeKind::Queue => {
            for offset in [8.0, 4.0, 0.0] {
                shape.primitives.push(rectangle(
                    Rect::new(
                        rect.origin.x + offset,
                        rect.origin.y + offset,
                        rect.size.width - 8.0,
                        rect.size.height - 8.0,
                    ),
                    true,
                    true,
                ));
            }
        }
        _ => shape.primitives.push(rectangle(rect, true, true)),
    }
    let left = if node.kind == NodeKind::Client {
        44.0
    } else {
        10.0
    };
    let text_rect = Rect::new(
        rect.origin.x + left,
        rect.origin.y + 23.0,
        rect.size.width - left - 10.0,
        24.0,
    );
    shape.labels.push(label(&node.label, text_rect, 14.0, true));
    shape.labels.push(label(
        node_kind_name(node.kind),
        Rect::new(
            rect.origin.x + left,
            rect.origin.y + 10.0,
            rect.size.width - left - 12.0,
            12.0,
        ),
        9.0,
        false,
    ));
    shape
}

#[must_use]
pub fn node_kind_name(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Service => "service",
        NodeKind::Database => "database",
        NodeKind::Queue => "queue",
        NodeKind::Cache => "cache",
        NodeKind::Storage => "storage",
        NodeKind::Client => "client",
        NodeKind::External => "external",
        NodeKind::Function => "function",
        NodeKind::Generic => "generic",
    }
}

fn edge_kind_name(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::Sync => "sync",
        EdgeKind::Async => "async",
        EdgeKind::Data => "data",
        EdgeKind::Dependency => "dependency",
    }
}

fn edge_offsets(design: &Design) -> std::collections::BTreeMap<&str, f64> {
    let mut groups = std::collections::BTreeMap::<_, Vec<_>>::new();
    for edge in &design.edges {
        let pair = if edge.from < edge.to {
            (&edge.from, &edge.to)
        } else {
            (&edge.to, &edge.from)
        };
        groups.entry(pair).or_default().push(edge.id.as_str());
    }
    let mut offsets = std::collections::BTreeMap::new();
    for group in groups.values_mut() {
        group.sort_unstable();
        let count = group.iter().fold(0.0, |n, _| n + 1.0);
        let mut offset = -(count - 1.0) * 28.0;
        for id in group {
            offsets.insert(*id, offset);
            offset += 56.0;
        }
    }
    offsets
}

impl Scene {
    /// Build from a validated design and the complete result of `sysy_layout::layout`.
    #[must_use]
    pub fn build(design: &Design, layout: &Layout) -> Self {
        let rect = |id: &str| layout.get(id).map(|entry| element_rect(id, entry, design));
        let mut scene = Self::default();
        let mut containers: Vec<_> = design.containers.iter().collect();
        containers.sort_by_key(|container| {
            let mut depth = 0;
            let mut parent = container.parent.as_deref();
            while let Some(id) = parent {
                depth += 1;
                parent = design
                    .containers
                    .iter()
                    .find(|c| c.id == id)
                    .and_then(|c| c.parent.as_deref());
            }
            (depth, &container.id)
        });
        for container in containers {
            if let Some(rect) = rect(&container.id) {
                let mut shape = box_shape(&container.id, ShapeKind::Container, rect);
                shape.primitives.push(rectangle(rect, true, true));
                shape.labels.push(label(
                    &container.label,
                    Rect::new(
                        rect.origin.x + 10.0,
                        rect.origin.y + 7.0,
                        rect.size.width - 20.0,
                        20.0,
                    ),
                    12.0,
                    false,
                ));
                scene.shapes.push(shape);
            }
        }
        let offsets = edge_offsets(design);
        for edge in &design.edges {
            if let (Some(from), Some(to)) = (rect(&edge.from), rect(&edge.to)) {
                scene
                    .shapes
                    .push(edge_shape(edge, from, to, offsets[edge.id.as_str()]));
            }
        }
        for node in &design.nodes {
            if let Some(rect) = rect(&node.id) {
                scene.shapes.push(node_shape(node, rect));
            }
        }
        for note in &design.notes {
            if let Some(rect) = rect(&note.id) {
                let mut shape = box_shape(&note.id, ShapeKind::Note, rect);
                shape.primitives.push(rectangle(rect, true, true));
                shape.primitives.push(stroke(
                    vec![
                        point(rect.right() - 12.0, rect.origin.y),
                        point(rect.right() - 12.0, rect.origin.y + 12.0),
                        point(rect.right(), rect.origin.y + 12.0),
                    ],
                    1.0,
                ));
                shape
                    .labels
                    .push(label(&note.text, inset(rect, 10.0), 13.0, false));
                scene.shapes.push(shape);
            }
        }
        scene.bounds = scene
            .shapes
            .iter()
            .map(|shape| shape.bounds)
            .reduce(Rect::union);
        scene
    }

    /// Return the last painted element under the cursor; tolerance is in world units.
    #[must_use]
    pub fn hit_test(&self, point: Point, tolerance: f64) -> Option<&str> {
        self.shapes
            .iter()
            .rev()
            .find(|shape| {
                if matches!(shape.kind, ShapeKind::Edge(_)) {
                    shape
                        .labels
                        .iter()
                        .any(|label| label.rect.contains_point(point))
                        || shape
                            .route
                            .windows(2)
                            .any(|p| segment_distance(point, p[0], p[1]) <= tolerance)
                        || shape
                            .arrowheads
                            .iter()
                            .any(|head| points_bounds(head).contains_point(point))
                } else {
                    shape.bounds.contains_point(point)
                }
            })
            .map(|shape| shape.id.as_str())
    }
}

fn segment_distance(p: Point, a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length_squared = dx * dx + dy * dy;
    if length_squared < f64::EPSILON {
        return distance(p, a);
    }
    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / length_squared;
    distance(p, along(a, b, t.clamp(0.0, 1.0)))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub pan: Point,
    pub scale: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            pan: Point::default(),
            scale: 1.0,
        }
    }
}

impl Camera {
    #[must_use]
    pub fn fit(bounds: Option<Rect>, viewport: Size, margin: f64) -> Self {
        let Some(bounds) = bounds else {
            return Self::default();
        };
        let width = (viewport.width - 2.0 * margin).max(1.0);
        let height = (viewport.height - 2.0 * margin).max(1.0);
        let scale = (width / bounds.size.width.max(1.0))
            .min(height / bounds.size.height.max(1.0))
            .min(1.0);
        Self {
            scale,
            pan: point(
                (viewport.width - bounds.size.width * scale) / 2.0 - bounds.origin.x * scale,
                (viewport.height - bounds.size.height * scale) / 2.0 - bounds.origin.y * scale,
            ),
        }
    }

    #[must_use]
    pub fn world_to_screen(self, p: Point) -> Point {
        point(p.x * self.scale + self.pan.x, p.y * self.scale + self.pan.y)
    }

    #[must_use]
    pub fn screen_to_world(self, p: Point) -> Point {
        point(
            (p.x - self.pan.x) / self.scale,
            (p.y - self.pan.y) / self.scale,
        )
    }

    pub fn pan_by(&mut self, delta: Point) {
        self.pan.x += delta.x;
        self.pan.y += delta.y;
    }

    pub fn zoom_at(&mut self, cursor: Point, factor: f64) {
        let world = self.screen_to_world(cursor);
        // Fitting very large designs may start below the normal interactive minimum.
        self.scale = (self.scale * factor).clamp(self.scale.min(0.02), 4.0);
        self.pan = point(
            cursor.x - world.x * self.scale,
            cursor.y - world.y * self.scale,
        );
    }
}

/// Whether a shape is the hovered element or an edge touching it.
#[must_use]
pub fn highlighted(shape: &Shape, hovered: Option<&str>) -> bool {
    hovered.is_some_and(|id| {
        shape.id == id
            || shape
                .endpoints
                .as_ref()
                .is_some_and(|(from, to)| from == id || to == id)
    })
}

/// Plain text fields, shared by the window and tests. Missing metadata has an explicit value.
#[must_use]
pub fn detail_text(design: &Design, id: &str) -> Option<Vec<(String, String)>> {
    let mut fields = vec![("Id".into(), id.into())];
    let (kind, text, description, tags) =
        if let Some(node) = design.nodes.iter().find(|n| n.id == id) {
            (
                node_kind_name(node.kind),
                node.label.as_str(),
                node.description.as_deref(),
                node.tags.join(", "),
            )
        } else if let Some(container) = design.containers.iter().find(|c| c.id == id) {
            (
                "container",
                container.label.as_str(),
                container.description.as_deref(),
                String::new(),
            )
        } else if let Some(edge) = design.edges.iter().find(|e| e.id == id) {
            fields.push((
                "Direction".into(),
                if edge.bidirectional {
                    "bidirectional"
                } else {
                    "one way"
                }
                .into(),
            ));
            (
                edge_kind_name(edge.kind),
                edge.label.as_deref().unwrap_or(""),
                None,
                String::new(),
            )
        } else {
            let note = design.notes.iter().find(|n| n.id == id)?;
            fields.push((
                "Attached to".into(),
                note.on.as_deref().unwrap_or("None").into(),
            ));
            ("note", note.text.as_str(), None, String::new())
        };
    for (heading, value) in [
        ("Kind", kind),
        ("Label", text),
        ("Description", description.unwrap_or("")),
        ("Tags", tags.as_str()),
    ] {
        fields.push((
            heading.into(),
            if value.is_empty() { "None" } else { value }.into(),
        ));
    }
    let connections = design
        .edges
        .iter()
        .filter(|edge| edge.id == id || edge.from == id || edge.to == id)
        .map(|edge| {
            format!(
                "{}: {} {} {} ({}){}",
                edge.id,
                edge.from,
                if edge.bidirectional { "↔" } else { "→" },
                edge.to,
                edge_kind_name(edge.kind),
                edge.label
                    .as_ref()
                    .map_or_else(String::new, |label| format!(" — {label}"))
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    fields.push((
        "Connections".into(),
        if connections.is_empty() {
            "None".into()
        } else {
            connections
        },
    ));
    Some(fields)
}
