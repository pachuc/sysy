//! Deterministic canvas layout, without rendering or platform dependencies.
//!
//! Call [`layout`] with a design validated by `sysy-core`. All coordinates are
//! absolute top-left positions. Fixed pins take precedence over automatic
//! spacing: overlapping pins, or a child pinned above or left of its pinned
//! parent's padded interior, cannot be repaired without moving a pin.

pub mod edges;
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

/// Left edge of every layer column. Each column is as wide as its widest node,
/// plus the gap and room for the container frames that may border it, so a
/// design is no wider than its content requires.
fn column_offsets(
    ranks: &BTreeMap<String, Point>,
    design: &Design,
    depth: f64,
) -> BTreeMap<u64, f64> {
    let mut widths: BTreeMap<u64, f64> = BTreeMap::new();
    for (id, rank) in ranks {
        // Ranks are small nonnegative integers stored as floats.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let column = rank.x.max(0.0) as u64;
        let width = design
            .nodes
            .iter()
            .find(|node| &node.id == id)
            .map_or(geometry::NODE_MIN_WIDTH, |node| {
                node_size(&node.label).width
            });
        let entry = widths.entry(column).or_insert(0.0);
        *entry = entry.max(width);
    }
    let last = widths.keys().max().copied().unwrap_or(0);
    let mut offsets = BTreeMap::new();
    let mut x = 0.0;
    for column in 0..=last {
        offsets.insert(column, x);
        let width = widths
            .get(&column)
            .copied()
            .unwrap_or(geometry::NODE_MIN_WIDTH);
        x += width + ELEMENT_GAP + 2.0 * depth * CONTAINER_PADDING;
    }
    offsets
}

fn desired_position(
    id: &str,
    ranks: &BTreeMap<String, Point>,
    paths: &BTreeMap<String, Vec<String>>,
    design: &Design,
    columns: &BTreeMap<u64, f64>,
) -> Point {
    let rank = ranks.get(id).copied().unwrap_or_default();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let column = rank.x.max(0.0) as u64;
    let mut point = Point {
        x: columns.get(&column).copied().unwrap_or_default(),
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

/// A node together with the unpinned notes attached to it, so frames grow
/// around the notes and packing keeps them clear of other elements. The notes
/// go below the node, or above it when the node's outgoing edges head down,
/// since edges leave from the node's sides and fan toward their targets.
fn node_block(
    node: &sysy_core::Node,
    point: Point,
    design: &Design,
    ranks: &BTreeMap<String, Point>,
    order: f64,
) -> Block {
    let mut block = leaf(
        &node.id,
        node_size(&node.label),
        false,
        point,
        design,
        order,
    );
    let node_rect = block.rect;
    let notes: Vec<_> = design
        .notes
        .iter()
        .filter(|note| note.on.as_deref() == Some(node.id.as_str()))
        .filter(|note| !design.layout.contains_key(&note.id))
        .collect();
    if notes.is_empty() {
        return block;
    }
    // Rows are only comparable within a layer, but as a hint for which side of
    // the node its edges leave toward, the row index is good enough.
    let row = ranks.get(&node.id).map_or(0.0, |rank| rank.y);
    let targets_below = design
        .edges
        .iter()
        .filter(|edge| edge.from == node.id)
        .filter_map(|edge| ranks.get(&edge.to))
        .filter(|target| target.y > row)
        .count();
    let outgoing = design
        .edges
        .iter()
        .filter(|edge| edge.from == node.id)
        .count();
    let above = outgoing > 0 && targets_below * 2 > outgoing;
    let mut y = if above {
        node_rect.origin.y
    } else {
        node_rect.bottom() + NOTE_GAP
    };
    for note in notes {
        let size = note_size(&note.text);
        if above {
            y -= size.height + NOTE_GAP;
        }
        let rect = Rect {
            origin: Point {
                x: node_rect.origin.x,
                y,
            },
            size,
        };
        if !above {
            y = rect.bottom() + NOTE_GAP;
        }
        block.rect = block.rect.union(rect);
        block.entries.insert(note.id.clone(), rect.entry(false));
    }
    block
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
    let columns = column_offsets(&ranks, design, depth);
    let mut blocks = BTreeMap::new();
    for node in &design.nodes {
        let point = desired_position(&node.id, &ranks, &paths, design, &columns);
        blocks.insert(
            node.id.clone(),
            node_block(node, point, design, &ranks, ranks[&node.id].y),
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
            let point = desired_position(&container.id, &ranks, &paths, design, &columns);
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
        // Edges leave a node from its right side, so a note there would sit on
        // top of them. Put the note below the node when it has outgoing edges.
        let is_node = design.nodes.iter().any(|node| node.id == id);
        let outgoing = design.edges.iter().any(|edge| edge.from == id);
        if is_node && outgoing {
            return Some(Point {
                x: rect.origin.x,
                y: rect.bottom() + NOTE_GAP,
            });
        }
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
