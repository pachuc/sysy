---
name: sysy
description: Create and maintain system designs in sysy JSON files through the sysy CLI once the architecture is agreed.
---

# sysy

sysy stores a system design in one JSON file that lives beside the code.
Nodes are components, containers are real boundaries, edges are directed
interactions, and notes are short callouts. Node and edge kinds control how
they are drawn; they do not add behaviour. Layout is stored separately from
the architecture.

## When to write

Put a design into a file once it is agreed. While the system is still being
discussed, keep the alternatives in the conversation.

## Addressing elements

Every command takes a design path. Every element has a short id made of
lowercase letters, digits, and inner hyphens, such as `worker` or `job-queue`.
Ids must be unique across nodes, containers, edges, and notes in that file.
Use ids in `--in`, `--from`, `--to`, and `--on`; labels are for people.

Read [reference.md](reference.md) for exact commands, flags, and kinds.

## Compose top down

1. Create the file with a title and a short description of its scope.
2. Add containers for actual deployment, network, or ownership boundaries.
   Create parents before children.
3. Add nodes inside those boundaries. Keep external clients and providers
   outside. Use labels a reader would recognise from the system.
4. Add one edge per real interaction, after both endpoints exist. Use `sync`
   for calls, `async` for queued messages, `data` for stored data access, and
   `dependency` for dependencies. Name the operation or payload in the label.
   A call's response usually belongs to the same edge. Separate distinct
   interactions even when they share endpoints.
5. Add notes for constraints that change how the design should be read.
   Attach them to the relevant element; keep long explanations in descriptions.

## Execute

Use the CLI with `--json` to get records you can inspect. For example, for an
agreed worker and queue:

```sh
sysy --json new design.json --title "Job processing"
sysy --json container add design.json control --label "Control plane"
sysy --json node add design.json worker --kind service --label "Step workers" --in control
sysy --json node add design.json jobs --kind queue --label "Job queue"
sysy --json edge add design.json dispatch --from jobs --to worker --kind async --label "Deliver jobs"
sysy --json note add design.json retry --on dispatch --text "Jobs may be delivered again."
sysy --json validate design.json
```

Mutations validate before saving. On a failure, read stderr and fix the
reported problem before continuing. Validation succeeds with `[]`. Command
errors use JSON, but argument parsing errors use text and exit with code 2.

The current CLI has element commands only. Leave layout empty; automatic
layout and the viewer will place elements once `sysy layout` and `sysy ui`
land. Check the generated reference for their availability.

## Read state

Start an edit with `sysy --json show PATH`. Use `node list`, `container list`,
`edge list`, or `note list` with the path and `--json` to inspect one kind.
Plain `show` prints only a summary. Inspect existing ids and boundaries before
adding anything. Finish with `validate` and `show`, and review the file diff.

## Keep it current

Update the design alongside the system change. Use `set` to change labels,
descriptions, membership, or connections while keeping existing ids stable.
Only supplied fields change; `--clear-*` removes optional values. Supplying
`--tag` to `node set` replaces all tags.

Add new components before connecting them. Remove obsolete interactions and
notes. Removing a node also removes its edges and attached notes. Removing a
container moves its children to its parent and removes edges and notes
attached to the container. Read the resulting state to check these effects.
Preserve a person's saved layout when changing architecture.

## Things not to do

- Do not invent components or connections to fill space. Resolve missing
  architecture details before recording them.
- Do not use containers just to arrange boxes or group unrelated components.
- Do not turn every function or replica into a node. Show components at the
  level the agreed design needs.
- Do not use long ids, paragraphs as labels, or vague edges such as "uses".
- Do not add reverse edges just to show replies, or mark unrelated traffic
  bidirectional to reduce the edge count.
- Do not rebuild an existing file to make a small edit or reset saved positions
  as part of an architecture update.
