//! Repack growing anchored frames when their preferred layers hit other pins.

use std::collections::BTreeMap;

use sysy_core::{Design, LayoutEntry};

use crate::{Block, CONTAINER_PADDING, ELEMENT_GAP, Point, Rect, geometry::element_rect};

fn frame(bounds: Rect, pin: Option<&LayoutEntry>) -> Rect {
    let origin = pin.map_or(
        Point {
            x: bounds.origin.x - CONTAINER_PADDING,
            y: bounds.origin.y - CONTAINER_PADDING,
        },
        |pin| Point { x: pin.x, y: pin.y },
    );
    let width = bounds.right() + CONTAINER_PADDING - origin.x;
    let height = bounds.bottom() + CONTAINER_PADDING - origin.y;
    Rect::new(
        origin.x,
        origin.y,
        width.max(pin.and_then(|pin| pin.size).map_or(0.0, |size| size.width)),
        height.max(pin.and_then(|pin| pin.size).map_or(0.0, |size| size.height)),
    )
}

fn fits(candidate: Rect, occupied: &[Rect], obstacles: &[Rect], pin: Option<&LayoutEntry>) -> bool {
    if pin.is_some_and(|pin| {
        candidate.origin.x < pin.x + CONTAINER_PADDING
            || candidate.origin.y < pin.y + CONTAINER_PADDING
    }) || occupied.iter().any(|rect| rect.intersects(candidate))
    {
        return false;
    }
    let bounds = occupied.iter().copied().fold(candidate, Rect::union);
    let rect = frame(bounds, pin);
    !obstacles.iter().any(|obstacle| obstacle.intersects(rect))
}

fn candidates(
    rect: Rect,
    occupied: &[Rect],
    obstacles: &[Rect],
    pin: Option<&LayoutEntry>,
) -> Vec<Point> {
    let mut xs = vec![rect.origin.x];
    let mut ys = vec![rect.origin.y];
    if let Some(pin) = pin {
        xs.push(pin.x + CONTAINER_PADDING);
        ys.push(pin.y + CONTAINER_PADDING);
    }
    for other in occupied {
        xs.extend([
            other.origin.x,
            other.right() + ELEMENT_GAP,
            other.origin.x - ELEMENT_GAP - rect.size.width,
        ]);
        ys.extend([
            other.origin.y,
            other.bottom() + ELEMENT_GAP,
            other.origin.y - ELEMENT_GAP - rect.size.height,
        ]);
    }
    for other in obstacles {
        xs.extend([
            other.right() + CONTAINER_PADDING,
            other.origin.x - CONTAINER_PADDING - rect.size.width,
        ]);
        ys.extend([
            other.bottom() + CONTAINER_PADDING,
            other.origin.y - CONTAINER_PADDING - rect.size.height,
        ]);
    }
    let mut points: Vec<_> = xs
        .iter()
        .flat_map(|x| ys.iter().map(move |y| Point { x: *x, y: *y }))
        .collect();
    let distance =
        |point: &Point| (point.x - rect.origin.x).abs() + (point.y - rect.origin.y).abs();
    points.sort_by(|a, b| {
        distance(a)
            .total_cmp(&distance(b))
            .then_with(|| a.x.total_cmp(&b.x))
            .then_with(|| a.y.total_cmp(&b.y))
    });
    points.dedup();
    points
}

pub(super) fn avoid_obstacles(
    id: &str,
    children: &mut [Block],
    design: &Design,
    paths: &BTreeMap<String, Vec<String>>,
    placed: &BTreeMap<String, Block>,
) {
    let pin = design.layout.get(id);
    if pin.is_none() && !children.iter().any(|child| child.anchored) {
        return;
    }
    let obstacles: Vec<_> = design
        .layout
        .iter()
        .filter(|(other, _)| {
            *other != id
                && paths
                    .get(*other)
                    .is_some_and(|path| !path.iter().any(|ancestor| ancestor == id))
                && !paths[id].contains(other)
        })
        .map(|(other, entry)| element_rect(other, entry, design))
        // Earlier sibling frames may already have grown beyond their saved
        // sizes. Reserve their complete bounds before growing this frame.
        .chain(
            placed
                .values()
                .filter(|block| block.anchored)
                .map(|block| block.rect),
        )
        .collect();
    let bounds = children
        .iter()
        .map(|child| child.rect)
        .reduce(Rect::union)
        .expect("nonempty container");
    if !obstacles
        .iter()
        .any(|obstacle| obstacle.intersects(frame(bounds, pin)))
    {
        return;
    }

    // Moving a whole anchored frame would move its pins. Try nearby positions
    // for its free children instead, allowing pins to override their layers.
    let mut occupied: Vec<_> = children
        .iter()
        .filter(|child| child.anchored)
        .map(|child| child.rect)
        .collect();
    for child in children.iter_mut().filter(|child| !child.anchored) {
        if let Some(point) = candidates(child.rect, &occupied, &obstacles, pin)
            .into_iter()
            .find(|point| {
                fits(
                    Rect {
                        origin: *point,
                        size: child.rect.size,
                    },
                    &occupied,
                    &obstacles,
                    pin,
                )
            })
        {
            child.translate(Point {
                x: point.x - child.rect.origin.x,
                y: point.y - child.rect.origin.y,
            });
        }
        occupied.push(child.rect);
    }
}
