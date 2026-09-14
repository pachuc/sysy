# sysy-layout

`layout(&Design) -> sysy_core::Layout` computes absolute top-left positions
without changing the design. Pass a design that has passed core validation.
The only runtime dependency is `sysy-core`.

Nodes use longest-path layers, from left to right. The greatest edge id that
participates in a cycle is ignored for layering, repeatedly until the graph
is acyclic. Eight pairs of barycenter sweeps order each layer. Ties use ids,
and container ancestry keeps groups adjacent. Container endpoints expand to
their descendant nodes and empty containers. Bidirectional edges use their
declared `from` and `to` direction.

Container groups are packed from the deepest level outward, with 32 units of
padding per level. Pins reserve their positions before free groups are placed.
A growing anchored frame tries nearby positions for free contents if its
preferred layers would cover an unrelated pin. Pin constraints take precedence
over the preferred layers. Saved container sizes never shrink, and empty
pinned containers retain their saved sizes.

Conflicting pins cannot always satisfy the geometry rules. Examples include
two overlapping pinned nodes, a pinned child above or left of its pinned
parent's padded interior, or an unrelated pin inside a frame forced by other
pins. The function preserves pins in these cases; it does not change the
design or report validation errors for geometric conflicts.

The viewer can use `geometry::{Point, Size, Rect}`, `node_size`, `note_size`,
and `geometry::element_rect` for rendering and hit testing. `bounding_box`
includes nodes, notes, frames, and saved edge positions; it returns `None`
for an empty layout. Attached notes start just outside their target, or near
an edge midpoint, and move downward past occupied nodes and notes. Free notes
form a column to the right. Notes do not enlarge container frames.

## Fixtures

Designs and their exact JSON layout goldens live in `tests/fixtures`. Tests
check ten repeated layouts, reversed input arrays, pin preservation, overlap,
padding, growth, and bounds. Refresh goldens deliberately with:

```sh
SYSY_UPDATE_GOLDENS=1 cargo test --locked -p sysy-layout
```

Run the workspace checks with:

```sh
cargo fmt --all -- --check
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
```
