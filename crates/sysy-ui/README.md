# sysy-ui

Open a design with `sysy ui PATH`. The viewer loads the design through
`sysy-core` and computes missing positions with `sysy-layout`.

| Input | Action |
| --- | --- |
| Drag empty canvas | Pan |
| Trackpad scroll | Pan |
| Mouse wheel | Zoom around the cursor |
| Ctrl-scroll or Command-scroll | Zoom around the cursor |
| `f` | Fit the design to the canvas, with margin |
| Click an element | Show its metadata and connections |
| Hover an element | Highlight it and edges touching it |

Containers paint in ancestor order, followed by edges, nodes, and notes.
Nodes carry their kind as text; databases have cylinder outlines, queues have
stacked slats, clients have person outlines, and external nodes have dashed
frames. Data edges have a heavier stroke, async edges use dashes, and dependency
edges use dots. Bidirectional edges have arrowheads at both ends.

`scene` contains drawable geometry, line patterns, arrowheads, hit testing,
camera transforms, fit calculation, and detail fields. Its tests use the
Checkout example in `sysy-core/tests/fixtures/checkout.json` and `examples/swarmy.json`, plus cases for
all node kinds, nested frames, notes, self edges, and container endpoints.
`window` converts GPUI events to scene operations and paints the result.
The viewer does not write the file in this version.

To save automatic positions, run `sysy layout PATH`. Existing pins remain
fixed and container sizes can grow. `sysy layout PATH --reset` discards the
pins before computing the layout. The command prints the layout map with
`--json`. Edges are drawn from their endpoints and do not need saved positions.

## Manual validation

On a machine with a display, run:

```sh
cargo run --locked -p sysy-cli -- ui examples/swarmy.json
```

Check node shapes, arrowheads, edge labels, pan, zoom around the cursor, fit,
selection details, and hover highlighting. Use a design containing all nine
node kinds and all four edge kinds to check the full visual vocabulary.
The sandbox and CI cannot perform this window check because they have no
display. GPUI 0.2.2 exposes wheel and scroll input, but no native pinch event.
