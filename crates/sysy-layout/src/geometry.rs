//! Canvas geometry in absolute, unscaled units. Positions are top-left corners.

use sysy_core::{Design, Layout, LayoutEntry};

pub use sysy_core::Size;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Rect {
    #[must_use]
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            origin: Point { x, y },
            size: Size { width, height },
        }
    }

    #[must_use]
    pub fn right(self) -> f64 {
        self.origin.x + self.size.width
    }

    #[must_use]
    pub fn bottom(self) -> f64 {
        self.origin.y + self.size.height
    }

    /// Boundary contact counts as containment.
    #[must_use]
    pub fn contains(self, other: Self) -> bool {
        self.origin.x <= other.origin.x
            && self.origin.y <= other.origin.y
            && self.right() >= other.right()
            && self.bottom() >= other.bottom()
    }

    #[must_use]
    pub fn contains_point(self, point: Point) -> bool {
        point.x >= self.origin.x
            && point.x <= self.right()
            && point.y >= self.origin.y
            && point.y <= self.bottom()
    }

    /// Only positive-area overlap counts; touching borders do not intersect.
    #[must_use]
    pub fn intersects(self, other: Self) -> bool {
        self.size.width > 0.0
            && self.size.height > 0.0
            && other.size.width > 0.0
            && other.size.height > 0.0
            && self.origin.x < other.right()
            && self.right() > other.origin.x
            && self.origin.y < other.bottom()
            && self.bottom() > other.origin.y
    }

    #[must_use]
    pub fn union(self, other: Self) -> Self {
        let x = self.origin.x.min(other.origin.x);
        let y = self.origin.y.min(other.origin.y);
        Self::new(
            x,
            y,
            self.right().max(other.right()) - x,
            self.bottom().max(other.bottom()) - y,
        )
    }

    pub(crate) fn entry(self, container: bool) -> LayoutEntry {
        LayoutEntry {
            x: self.origin.x,
            y: self.origin.y,
            size: container.then_some(self.size),
        }
    }
}

pub const NODE_MIN_WIDTH: f64 = 120.0;
pub const NODE_MAX_WIDTH: f64 = 280.0;
pub const NODE_HEIGHT: f64 = 64.0;

/// Estimate label width using Unicode scalar values, with 32 units of inset.
/// The viewer should truncate or wrap labels longer than the maximum width.
#[must_use]
pub fn node_size(label: &str) -> Size {
    let width = label
        .chars()
        .take(31)
        .fold(32.0_f64, |width, _| width + 8.0);
    Size {
        width: width.clamp(NODE_MIN_WIDTH, NODE_MAX_WIDTH),
        height: NODE_HEIGHT,
    }
}

/// Notes have a fixed width and reserve a line for each 28 characters.
#[must_use]
pub fn note_size(text: &str) -> Size {
    let lines = text.split('\n').fold(0.0, |total, line| {
        let mut width = 0;
        let mut rows = 1.0;
        for _ in line.chars() {
            if width == 28 {
                rows += 1.0;
                width = 0;
            }
            width += 1;
        }
        total + rows
    });
    Size {
        width: 240.0,
        height: 24.0 + lines * 20.0,
    }
}

/// Return an element's rectangle, using the same size rules as automatic layout.
/// Edges have no box; a saved edge position is treated as a point.
#[must_use]
pub fn element_rect(id: &str, entry: &LayoutEntry, design: &Design) -> Rect {
    let size = design
        .nodes
        .iter()
        .find(|node| node.id == id)
        .map(|node| node_size(&node.label))
        .or_else(|| {
            design
                .notes
                .iter()
                .find(|note| note.id == id)
                .map(|note| note_size(&note.text))
        })
        .or(entry.size)
        .unwrap_or(Size {
            width: 0.0,
            height: 0.0,
        });
    Rect {
        origin: Point {
            x: entry.x,
            y: entry.y,
        },
        size,
    }
}

/// Return the bounds of all saved elements, or `None` for an empty layout.
#[must_use]
pub fn bounding_box(layout: &Layout, design: &Design) -> Option<Rect> {
    layout
        .iter()
        .map(|(id, entry)| element_rect(id, entry, design))
        .reduce(Rect::union)
}
