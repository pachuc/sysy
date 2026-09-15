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

/// The side of a rectangle an edge attaches to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

impl Side {
    /// Outward unit normal, used as the curve tangent at the port.
    fn normal(self) -> Point {
        match self {
            Self::Left => point(-1.0, 0.0),
            Self::Right => point(1.0, 0.0),
            Self::Top => point(0.0, -1.0),
            Self::Bottom => point(0.0, 1.0),
        }
    }

    fn horizontal(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

/// One end of an edge: which element side it leaves from and where along it.
#[derive(Clone, Debug)]
struct Port {
    side: Side,
    /// Position along the side, 0 to 1, assigned after sorting the side's ports.
    fraction: f64,
}

fn port_point(rect: Rect, port: &Port) -> Point {
    // Keep ports away from the corners so arrowheads stay on the flat part.
    let f = 0.15 + 0.7 * port.fraction;
    match port.side {
        Side::Left => point(rect.origin.x, rect.origin.y + rect.size.height * f),
        Side::Right => point(rect.right(), rect.origin.y + rect.size.height * f),
        Side::Top => point(rect.origin.x + rect.size.width * f, rect.origin.y),
        Side::Bottom => point(rect.origin.x + rect.size.width * f, rect.bottom()),
    }
}

/// Choose the sides for an edge from the relative placement of its endpoints.
/// Forward edges in the left-to-right flow leave the right side and enter the
/// left side; edges between vertically stacked elements use top and bottom.
fn choose_sides(from: Rect, to: Rect) -> (Side, Side) {
    let gap = 24.0;
    if to.origin.x >= from.right() + gap {
        (Side::Right, Side::Left)
    } else if from.origin.x >= to.right() + gap {
        (Side::Left, Side::Right)
    } else if to.origin.y >= from.bottom() {
        (Side::Bottom, Side::Top)
    } else if from.origin.y >= to.bottom() {
        (Side::Top, Side::Bottom)
    } else {
        // Overlapping rectangles, such as a node and its own container.
        (Side::Right, Side::Left)
    }
}

/// Sample a cubic curve between two ports whose tangents follow the side normals.
fn curve(a: Point, a_side: Side, b: Point, b_side: Side) -> Vec<Point> {
    let reach = if a_side.horizontal() {
        ((b.x - a.x).abs() * 0.5).max(48.0)
    } else {
        ((b.y - a.y).abs() * 0.5).max(48.0)
    };
    let na = a_side.normal();
    let nb = b_side.normal();
    let c1 = point(a.x + na.x * reach, a.y + na.y * reach);
    let c2 = point(b.x + nb.x * reach, b.y + nb.y * reach);
    let steps = 24;
    (0..=steps)
        .map(|i| {
            let t = f64::from(i) / f64::from(steps);
            let u = 1.0 - t;
            point(
                u * u * u * a.x + 3.0 * u * u * t * c1.x + 3.0 * u * t * t * c2.x + t * t * t * b.x,
                u * u * u * a.y + 3.0 * u * u * t * c1.y + 3.0 * u * t * t * c2.y + t * t * t * b.y,
            )
        })
        .collect()
}

fn self_loop(from: Rect, to: Rect) -> Vec<Point> {
    vec![
        point(from.right(), center(from).y),
        point(from.right() + 32.0, center(from).y),
        point(from.right() + 32.0, from.origin.y - 24.0),
        point(center(to).x, from.origin.y - 24.0),
        point(center(to).x, to.origin.y),
    ]
}

/// One edge end waiting to be spread along an element side.
struct SidePort {
    /// Coordinate of the far end along the side's axis; ports are sorted by it.
    key: f64,
    edge: String,
    is_from: bool,
}

/// Assign every edge end a side and a spread position along that side, so
/// several edges leaving one element fan out instead of stacking on one point.
fn assign_ports(design: &Design, rects: &BTreeMap<&str, Rect>) -> BTreeMap<String, (Port, Port)> {
    let mut ports: BTreeMap<String, (Port, Port)> = BTreeMap::new();
    let mut sides: BTreeMap<(&str, Side), Vec<SidePort>> = BTreeMap::new();
    for edge in design.list_edges() {
        let (Some(&from), Some(&to)) = (rects.get(edge.from.as_str()), rects.get(edge.to.as_str()))
        else {
            continue;
        };
        if edge.from == edge.to {
            continue;
        }
        let (from_side, to_side) = choose_sides(from, to);
        // Sort ports along a side by where the other end sits, so lines do not cross
        // right at the element boundary.
        let key = |side: Side, other: Rect| {
            if side.horizontal() {
                center(other).y
            } else {
                center(other).x
            }
        };
        sides
            .entry((edge.from.as_str(), from_side))
            .or_default()
            .push(SidePort {
                key: key(from_side, to),
                edge: edge.id.clone(),
                is_from: true,
            });
        sides
            .entry((edge.to.as_str(), to_side))
            .or_default()
            .push(SidePort {
                key: key(to_side, from),
                edge: edge.id.clone(),
                is_from: false,
            });
        ports.insert(
            edge.id.clone(),
            (
                Port {
                    side: from_side,
                    fraction: 0.5,
                },
                Port {
                    side: to_side,
                    fraction: 0.5,
                },
            ),
        );
    }
    for entries in sides.values_mut() {
        entries.sort_by(|a, b| a.key.total_cmp(&b.key).then_with(|| a.edge.cmp(&b.edge)));
        let last = entries.iter().skip(1).fold(0.0_f64, |n, _| n + 1.0);
        let mut index = 0.0_f64;
        for entry in entries.iter() {
            let fraction = if last == 0.0 { 0.5 } else { index / last };
            index += 1.0;
            let pair = ports.get_mut(&entry.edge).expect("port registered");
            if entry.is_from {
                pair.0.fraction = fraction;
            } else {
                pair.1.fraction = fraction;
            }
        }
    }
    ports
}

fn route(edge: &Edge, from: Rect, to: Rect, ports: Option<&(Port, Port)>) -> Vec<Point> {
    match ports {
        Some((a, b)) if edge.from != edge.to && distance(center(from), center(to)) >= 0.001 => {
            curve(port_point(from, a), a.side, port_point(to, b), b.side)
        }
        _ => self_loop(from, to),
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

/// Derive routes and label rectangles without adding automatic label pins to the file.
/// Labels prefer their route midpoint, then nearby points on the route. If all
/// are occupied, place the label below the obstruction and retain its anchor.
#[must_use]
pub fn edge_geometry(design: &Design, layout: &Layout) -> BTreeMap<String, EdgeGeometry> {
    let rects: BTreeMap<&str, Rect> = layout
        .iter()
        .map(|(id, entry)| (id.as_str(), element_rect(id, entry, design)))
        .collect();
    let ports = assign_ports(design, &rects);
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
            ports.get(&edge.id),
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
