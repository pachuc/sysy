# sysy design

sysy is a local tool for system designs. Agents build a design through a
command line interface, and people look at it in a desktop window. It plays
the same role for architecture diagrams that tasky plays for task graphs: the
machine-facing side is a small, strict data model with a CLI that always
speaks JSON, and the human-facing side is a canvas that is pleasant to read.

The look is inspired by excalidraw: boxes, arrows, labels, and grouping
frames on an infinite pannable canvas. The content is not free-form drawing.
A design is a typed model of components and the connections between them, and
the tool decides where things go unless a person has moved them.

## Goals

- A design is one JSON file that lives in the repository it describes, so it
  is versioned, diffed, and reviewed with the code.
- Everything an agent needs is reachable through the CLI with `--json` output,
  and the file format is simple enough that an agent may also write the file
  directly and ask the CLI to validate it.
- The viewer needs no configuration: `sysy ui path/to/design.json` opens it.
- Layout is automatic and deterministic. Re-running it on an unchanged design
  produces the same picture. A person can drag a component somewhere else and
  that position sticks.
- No network, no daemon, no database. Rust throughout, GPUI for rendering.

Out of scope for the first version: image export, drawing free shapes in the
window, creating or deleting components in the window, collaboration, and
anything that needs a server.

## The model

A design has a title, an optional description, and four kinds of elements.
Every element has an id that is a slug: lowercase letters, digits, and inner
hyphens. Ids are chosen by the author, because they are typed in commands and
referenced from prose, and must be unique across the whole design.

**Nodes** are the components. A node has a kind, a label, an optional
description, optional tags, and optionally the id of the container it sits in.
The kinds in the first version are `service`, `database`, `queue`, `cache`,
`storage`, `client`, `external`, `function`, and `generic`. The kind changes
how the node is drawn, for example a database is a cylinder and a queue is a
stack of slats, and nothing else; the tool attaches no behaviour to kinds.

**Containers** are boundaries drawn as frames around the nodes inside them: a
network, a region, a team's ownership, a deployment unit. A container has a
label, an optional description, and optionally a parent container. Nesting is
allowed to any depth and must not form a cycle.

**Edges** connect two nodes, or a node and a container, in a direction. An
edge has an optional label, a kind from `sync`, `async`, `data`, and
`dependency`, and a flag for bidirectional. The kind changes the line style.
Several edges may connect the same pair.

**Notes** are short pieces of text attached to a node, a container, an edge,
or nothing. They render as small callouts near what they describe.

Separate from the elements, a design carries **layout state**: a map from
element id to a position and, for containers, a size. Positions are absolute
canvas coordinates in an abstract unit; the viewer scales them. The layout map
is optional per element. An element without an entry is placed by the
automatic layout. An element with an entry is pinned where it was put.

Keeping layout state out of the elements matters for diffs: an agent changing
the architecture touches the element section, a person tidying the picture
touches the layout section, and the two rarely conflict.

## The file format

The file is JSON with a top-level `version` field, currently `1`, and the
sections `design`, `nodes`, `containers`, `edges`, `notes`, and `layout`.
Elements are stored as arrays sorted by id, keys within an object are written
in a fixed order, and the file is pretty-printed with two-space indentation
and a trailing newline. The CLI writes the file atomically: write a temporary
file beside it, then rename. Loading rejects unknown versions, unknown top
level keys, duplicate ids, references to ids that do not exist, and container
cycles, and reports every problem it found rather than the first.

An example:

```json
{
  "version": 1,
  "design": { "title": "Checkout", "description": "Order placement path." },
  "containers": [
    { "id": "vpc", "label": "Production VPC" }
  ],
  "nodes": [
    { "id": "api", "kind": "service", "label": "Checkout API", "container": "vpc" },
    { "id": "orders", "kind": "database", "label": "Orders DB", "container": "vpc" },
    { "id": "shopper", "kind": "client", "label": "Shopper" }
  ],
  "edges": [
    { "id": "place-order", "from": "shopper", "to": "api", "kind": "sync", "label": "POST /orders" },
    { "id": "persist", "from": "api", "to": "orders", "kind": "data" }
  ],
  "notes": [],
  "layout": {
    "api": { "x": 320, "y": 120 }
  }
}
```

## The command line

The binary is `sysy`. Every command takes the design file path, prints a JSON
document describing the result when `--json` is passed, and exits 1 with a
JSON error on stderr when something is wrong. Mutating commands validate the
whole design before writing and refuse to write an invalid one.

- `sysy new PATH --title TITLE [--description TEXT]` creates an empty design.
- `sysy node add PATH ID --kind KIND --label LABEL [--description TEXT]
  [--tag TAG]... [--in CONTAINER]`, and matching `node set`, `node remove`,
  and `node list`. Removing a node removes its edges and notes and its layout
  entry.
- `sysy container add PATH ID --label LABEL [--description TEXT]
  [--in PARENT]`, with `set`, `remove`, and `list`. Removing a container
  moves its children to its parent.
- `sysy edge add PATH ID --from A --to B [--kind KIND] [--label TEXT]
  [--bidirectional]`, with `set`, `remove`, and `list`.
- `sysy note add PATH ID --text TEXT [--on ELEMENT]`, with `set`, `remove`,
  and `list`.
- `sysy show PATH` prints the whole design; `sysy validate PATH` prints the
  list of problems and exits 1 if there are any.
- `sysy layout PATH [--reset]` runs the automatic layout and writes the
  resulting positions into the layout section. Without `--reset` pinned
  elements stay where they are; with it every position is recomputed.
- `sysy ui PATH` opens the viewer. The viewer watches the file and reloads
  when it changes on disk.

## Layout

The automatic layout is layered and flows left to right. Nodes are assigned
to layers by following edges from sources to sinks, cycles are broken on the
edge that appears last, and nodes inside one container are kept adjacent
within a layer so the container frame stays compact. Containers are sized to
their contents plus padding, nested containers padded inside their parents.
Within a layer, nodes are ordered to reduce edge crossings with the usual
barycenter pass. Edges are drawn as straight or gently curved lines with an
arrowhead at the target end, and labels sit at the midpoint.

Pinned elements are respected: a pinned node keeps its position, and the
unpinned nodes are placed around it. A pinned container keeps its position and
size and clips nothing; if its contents no longer fit, the layout grows it and
records the new size.

The layout is a pure function from a design to a layout map. It lives in its
own crate so the CLI and the viewer share it and so it can be tested with
fixtures: the same input always produces the same output, no two nodes
overlap, and every node lies inside its container's frame.

## The viewer

The viewer is a GPUI window with one canvas. It draws containers as rounded
frames with their label in the top left, nodes as kind-specific shapes with
their label inside, edges as lines with arrowheads and labels, and notes as
callouts. Pan by dragging the background or scrolling, zoom with the wheel
or pinch around the cursor, and fit the whole design with a key. Clicking an
element selects it and shows its description, tags, and connections in a side
panel; hovering highlights the element and its edges.

Dragging a node moves it and, on release, writes its new position into the
layout section of the file. Dragging a container moves it and everything
inside it. Because the viewer watches the file, an agent editing the design
while the window is open sees the result immediately, and the two do not
fight over the file because the viewer only ever rewrites the layout section
and writes atomically.

## Crates

- `sysy-core`: the model, the file format, validation, and atomic save. No
  rendering, no layout.
- `sysy-layout`: the automatic layout as a pure function.
- `sysy-ui`: the GPUI viewer, taking a path and using the two crates above.
- `sysy-cli`: the `sysy` binary.

The workspace follows tasky's conventions: Rust 2024, pedantic clippy denied,
`unsafe_code` forbidden, GPUI pinned to the same version as tasky, and a CI
workflow that builds on Linux and macOS.

## Agent usage

`skills/sysy/SKILL.md` explains to an agent how to author a design, in the
same shape as tasky's skill, and `skills/sysy/reference.md` is generated from
the CLI's help output. An example design of a real system lives under
`examples/` and doubles as a fixture for the viewer.
