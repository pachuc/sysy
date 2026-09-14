//! Edge routes and collision-free automatic label placement in canvas units.

use std::collections::BTreeMap;

use sysy_core::{Design, Edge, Layout};

use crate::{Point, Rect, geometry::element_rect};

#[derive(Clone, Debug, PartialEq)]
pub struct EdgeGeometry {
    pub route: Vec<Point>,
    pub label: Option<Rect>,
    /// The route anchor connects a displaced label back to its edge.
    pub anchor: Point,
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

fn route(edge: &Edge, from: Rect, to: Rect, offset: f64) -> Vec<Point> {
    if edge.from == edge.to || distance(center(from), center(to)) < 0.001 {
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
    }
}

fn route_midpoint(route: &[Point]) -> Point {
    route_point(route, 0.5)
}

fn route_point(route: &[Point], fraction: f64) -> Point {
    let mut remaining = route.windows(2).map(|p| distance(p[0], p[1])).sum::<f64>() * fraction;
    for pair in route.windows(2) {
        let length = distance(pair[0], pair[1]);
        if remaining <= length {
            return along(pair[0], pair[1], remaining / length.max(0.001));
        }
        remaining -= length;
    }
    route.first().copied().unwrap_or_default()
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

/// Derive routes and label rectangles without adding automatic label pins to the file.
/// Labels prefer their route midpoint, then nearby points on the route. If all
/// are occupied, place the label below the obstruction and retain its anchor.
#[must_use]
pub fn edge_geometry(design: &Design, layout: &Layout) -> BTreeMap<String, EdgeGeometry> {
    let offsets = edge_offsets(design);
    let mut occupied: Vec<_> = design
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .chain(design.notes.iter().map(|note| note.id.as_str()))
        .filter_map(|id| layout.get(id).map(|entry| element_rect(id, entry, design)))
        .collect();
    // Container interiors are available, but their headings must remain readable.
    occupied.extend(design.containers.iter().filter_map(|container| {
        let rect = element_rect(&container.id, layout.get(&container.id)?, design);
        Some(Rect::new(
            rect.origin.x,
            rect.origin.y,
            rect.size.width,
            28.0,
        ))
    }));
    let mut result = BTreeMap::new();
    for edge in design.list_edges() {
        let (Some(from), Some(to)) = (layout.get(&edge.from), layout.get(&edge.to)) else {
            continue;
        };
        let route = route(
            edge,
            element_rect(&edge.from, from, design),
            element_rect(&edge.to, to, design),
            offsets[edge.id.as_str()],
        );
        let mut anchor = route_midpoint(&route);
        let label = edge
            .label
            .as_deref()
            .filter(|text| !text.is_empty())
            .map(|text| {
                let width = text.chars().take(40).fold(16.0_f64, |width, _| width + 7.0);
                let at =
                    |point: Point| Rect::new(point.x - width / 2.0, point.y - 12.0, width, 24.0);
                let mut rect = at(anchor);
                for fraction in [0.5, 0.4, 0.6, 0.3, 0.7, 0.2, 0.8] {
                    let candidate = route_point(&route, fraction);
                    if !occupied.iter().any(|other| other.intersects(at(candidate))) {
                        anchor = candidate;
                        rect = at(candidate);
                        break;
                    }
                }
                while let Some(bottom) = occupied
                    .iter()
                    .filter(|other| other.intersects(rect))
                    .map(|other| other.bottom())
                    .max_by(f64::total_cmp)
                {
                    rect.origin.y = bottom + 6.0;
                }
                // Leave a small gap between successive label backgrounds.
                occupied.push(Rect::new(
                    rect.origin.x - 3.0,
                    rect.origin.y - 3.0,
                    rect.size.width + 6.0,
                    rect.size.height + 6.0,
                ));
                rect
            });
        result.insert(
            edge.id.clone(),
            EdgeGeometry {
                route,
                label,
                anchor,
            },
        );
    }
    result
}
