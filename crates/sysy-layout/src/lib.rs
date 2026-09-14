//! Deterministic canvas layout, without rendering or platform dependencies.
//!
//! Call [`layout`] with a design validated by `sysy-core`. All coordinates are
//! absolute top-left positions. Fixed pins take precedence over automatic
//! spacing: overlapping pins, or a child pinned above or left of its pinned
//! parent's padded interior, cannot be repaired without moving a pin.

pub mod geometry;
mod layers;
mod pins;

use std::collections::BTreeMap;

use sysy_core::{Design, Layout};

pub use geometry::{Point, Rect, Size, bounding_box, node_size, note_size};

/// Inset on all four sides of each container, including nested frames.
pub const CONTAINER_PADDING: f64 = 32.0;
pub const ELEMENT_GAP: f64 = 40.0;
pub const NOTE_GAP: f64 = 16.0;

struct Block {
    id: String,
    rect: Rect,
    entries: Layout,
    anchored: bool,
    order: f64,
}

impl Block {
    fn translate(&mut self, offset: Point) {
        self.rect.origin.x += offset.x;
        self.rect.origin.y += offset.y;
        for entry in self.entries.values_mut() {
            entry.x += offset.x;
            entry.y += offset.y;
        }
    }

    fn translate_y(&mut self, offset: f64) {
        self.rect.origin.y += offset;
        for entry in self.entries.values_mut() {
            entry.y += offset;
        }
    }
}

fn pack(blocks: &mut [Block]) {
    blocks.sort_by(|a, b| {
        b.anchored
            .cmp(&a.anchored)
            .then_with(|| a.order.total_cmp(&b.order))
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut occupied = Vec::<Rect>::new();
    for block in blocks {
        if !block.anchored {
            while let Some(bottom) = occupied
                .iter()
                .filter(|rect| rect.intersects(block.rect))
                .map(|rect| rect.bottom())
                .max_by(f64::total_cmp)
            {
                block.translate_y(bottom + ELEMENT_GAP - block.rect.origin.y);
            }
        }
        occupied.push(block.rect);
    }
}

fn desired_position(
    id: &str,
    ranks: &BTreeMap<String, Point>,
    paths: &BTreeMap<String, Vec<String>>,
    design: &Design,
    stride: f64,
) -> Point {
    let rank = ranks.get(id).copied().unwrap_or_default();
    let mut point = Point {
        x: rank.x * stride,
        y: rank.y * (geometry::NODE_HEIGHT + ELEMENT_GAP),
    };
    let mut inset = CONTAINER_PADDING;
    for ancestor in paths[id].iter().rev() {
        if let Some(pin) = design.layout.get(ancestor) {
            point.x = point.x.max(pin.x + inset);
            point.y = point.y.max(pin.y + inset);
        }
        inset += CONTAINER_PADDING;
    }
    point
}

fn leaf(id: &str, size: Size, container: bool, point: Point, design: &Design, order: f64) -> Block {
    let pin = design.layout.get(id);
    let rect = pin.map_or(
        Rect {
            origin: point,
            size,
        },
        |pin| {
            let saved = pin.size.unwrap_or(size);
            Rect::new(pin.x, pin.y, saved.width, saved.height)
        },
    );
    Block {
        id: id.to_owned(),
        rect,
        entries: BTreeMap::from([(id.to_owned(), rect.entry(container))]),
        anchored: pin.is_some(),
        order,
    }
}

fn frame(
    id: &str,
    mut children: Vec<Block>,
    design: &Design,
    paths: &BTreeMap<String, Vec<String>>,
    placed: &BTreeMap<String, Block>,
) -> Block {
    pack(&mut children);
    pins::avoid_obstacles(id, &mut children, design, paths, placed);
    let bounds = children
        .iter()
        .map(|child| child.rect)
        .reduce(Rect::union)
        .expect("nonempty container");
    let pin = design.layout.get(id);
    let origin = pin.map_or(
        Point {
            x: bounds.origin.x - CONTAINER_PADDING,
            y: bounds.origin.y - CONTAINER_PADDING,
        },
        |pin| Point { x: pin.x, y: pin.y },
    );
    let saved = pin.and_then(|entry| entry.size).unwrap_or(Size {
        width: 0.0,
        height: 0.0,
    });
    let rect = Rect {
        origin,
        size: Size {
            width: saved
                .width
                .max(bounds.right() + CONTAINER_PADDING - origin.x),
            height: saved
                .height
                .max(bounds.bottom() + CONTAINER_PADDING - origin.y),
        },
    };
    let anchored = pin.is_some() || children.iter().any(|child| child.anchored);
    let order = children
        .iter()
        .map(|child| child.order)
        .min_by(f64::total_cmp)
        .unwrap_or_default();
    let mut entries: Layout = children
        .into_iter()
        .flat_map(|child| child.entries)
        .collect();
    entries.insert(id.to_owned(), rect.entry(true));
    Block {
        id: id.to_owned(),
        rect,
        entries,
        anchored,
        order,
    }
}

fn place_elements(design: &Design) -> Layout {
    let paths = layers::paths(design);
    let ranks = layers::arrange(design, &paths);
    let depth = paths
        .values()
        .map(|path| path.iter().fold(0.0, |n, _| n + 1.0))
        .max_by(f64::total_cmp)
        .unwrap_or_default();
    // Reserve horizontal room for both frames bordering a layer gap. Packing
    // then only moves groups vertically, preserving left-to-right node layers.
    let stride = geometry::NODE_MAX_WIDTH + ELEMENT_GAP + 2.0 * depth * CONTAINER_PADDING;
    let mut blocks = BTreeMap::new();
    for node in &design.nodes {
        let point = desired_position(&node.id, &ranks, &paths, design, stride);
        blocks.insert(
            node.id.clone(),
            leaf(
                &node.id,
                node_size(&node.label),
                false,
                point,
                design,
                ranks[&node.id].y,
            ),
        );
    }
    let mut containers: Vec<_> = design.containers.iter().collect();
    containers.sort_by(|a, b| {
        paths[&b.id]
            .len()
            .cmp(&paths[&a.id].len())
            .then_with(|| a.id.cmp(&b.id))
    });
    for container in containers {
        let ids: Vec<_> = blocks
            .keys()
            .filter(|id| paths[*id].last() == Some(&container.id))
            .cloned()
            .collect();
        let children: Vec<_> = ids.iter().filter_map(|id| blocks.remove(id)).collect();
        let block = if children.is_empty() {
            let point = desired_position(&container.id, &ranks, &paths, design, stride);
            leaf(
                &container.id,
                Size {
                    width: geometry::NODE_MIN_WIDTH + 2.0 * CONTAINER_PADDING,
                    height: 2.0 * CONTAINER_PADDING,
                },
                true,
                point,
                design,
                ranks[&container.id].y,
            )
        } else {
            frame(&container.id, children, design, &paths, &blocks)
        };
        blocks.insert(container.id.clone(), block);
    }
    let mut roots: Vec<_> = blocks.into_values().collect();
    pack(&mut roots);
    let mut result = design.layout.clone();
    for block in roots {
        result.extend(block.entries);
    }
    result
}

fn note_anchor(id: &str, layout: &Layout, design: &Design) -> Option<Point> {
    if let Some(entry) = layout.get(id) {
        let rect = geometry::element_rect(id, entry, design);
        return Some(Point {
            x: rect.right() + NOTE_GAP,
            y: rect.origin.y,
        });
    }
    let edge = design.edges.iter().find(|edge| edge.id == id)?;
    let from = geometry::element_rect(&edge.from, layout.get(&edge.from)?, design);
    let to = geometry::element_rect(&edge.to, layout.get(&edge.to)?, design);
    Some(Point {
        x: (from.origin.x + from.size.width / 2.0 + to.origin.x + to.size.width / 2.0) / 2.0
            + NOTE_GAP,
        y: (from.origin.y + from.size.height / 2.0 + to.origin.y + to.size.height / 2.0) / 2.0
            + NOTE_GAP,
    })
}

fn place_notes(layout: &mut Layout, design: &Design) {
    let mut notes: Vec<_> = design.notes.iter().collect();
    notes.sort_by(|a, b| {
        a.on.is_none()
            .cmp(&b.on.is_none())
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut occupied: Vec<_> = design
        .nodes
        .iter()
        .map(|node| geometry::element_rect(&node.id, &layout[&node.id], design))
        .chain(design.notes.iter().filter_map(|note| {
            layout
                .get(&note.id)
                .map(|entry| geometry::element_rect(&note.id, entry, design))
        }))
        .collect();
    let mut column = None;
    let mut column_y = bounding_box(layout, design).map_or(0.0, |bounds| bounds.origin.y);
    for note in notes {
        if layout.contains_key(&note.id) {
            continue;
        }
        let point = note
            .on
            .as_deref()
            .and_then(|id| note_anchor(id, layout, design))
            .unwrap_or_else(|| {
                let x = *column.get_or_insert_with(|| {
                    bounding_box(layout, design).map_or(0.0, |bounds| bounds.right() + ELEMENT_GAP)
                });
                Point { x, y: column_y }
            });
        let mut rect = Rect {
            origin: point,
            size: note_size(&note.text),
        };
        while let Some(bottom) = occupied
            .iter()
            .filter(|other| other.intersects(rect))
            .map(|other| other.bottom())
            .max_by(f64::total_cmp)
        {
            rect.origin.y = bottom + NOTE_GAP;
        }
        if note.on.is_none() {
            column_y = rect.bottom() + NOTE_GAP;
        }
        occupied.push(rect);
        layout.insert(note.id.clone(), rect.entry(false));
    }
}

/// Return positions for every node and note and positions and sizes for every
/// container. Existing entries are pins; saved container sizes can only grow.
///
/// The input must have passed `sysy_core::validate`. The function does not change
/// the design, remove edges, or depend on element array order. Bidirectional
/// edges use their declared `from` and `to` direction for layering.
#[must_use]
pub fn layout(design: &Design) -> Layout {
    let mut result = place_elements(design);
    place_notes(&mut result, design);
    result
}
