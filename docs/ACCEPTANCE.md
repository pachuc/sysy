# First-version acceptance pass

## Authoring record

The checkout design was authored before reading implementation source, using
`skills/sysy/SKILL.md` and its generated reference. Repository rules and the
design document were read first as required. The system describes online
checkout, payment authorization, and asynchronous fulfillment.

The sandbox did not have `sysy` installed. After `rustup show` and
`cargo build --locked -p sysy-cli`, the authoring commands ran in
`bash --noprofile --norc`, with `target/debug` on `PATH`. The commands are kept
in `scripts/author-checkout.sh`; pass a new output path to replay them:

```sh
export PATH="$PWD/target/debug:$PATH"
bash --noprofile --norc scripts/author-checkout.sh /tmp/checkout.json
```

All architecture records were created through the CLI, including a later node
edit after saving layout. The fixture has 14 nodes, three containers with
`payments` nested inside `commerce`, 16 edges covering all four kinds, and
three notes. `sysy --json validate` returned `[]`.

## Friction and disposition

| Friction | Disposition |
| --- | --- |
| The skill said layout and the viewer were still unavailable, despite listing them in the reference. | Updated the workflow, viewer controls, pin behavior, and instructions for later CLI edits. |
| No installed binary, and no build instructions in the skill. | Added the build command, PATH guidance, and display prerequisite. |
| No command could revise the design title or description. | Added `sysy set` through a validated `Design::set` method, with `--title`, `--description`, and `--clear-description`. A title is required metadata, so it has no clear flag. |
| Plain `show` only printed counts, making architecture review require JSON. | It now prints title and description, nested containers and nodes, directed edges with kinds and labels, and attached or free notes. JSON is unchanged. |
| Swarmy edge labels overlapped near the control plane. | Moved edge geometry into the layout crate. Labels prefer the midpoint, try nearby route positions, then move below obstructions with a line back to the edge. Fixture tests check both examples for overlap and the viewer uses those same rectangles. |
| A route crossing another edge label could obscure its text and intercept clicks. | Edge labels now paint above every route and take priority over routes in hit testing. The fixture test checks each label is clickable, and a scene test checks paint order. |
| New nodes straddled saved container borders in the live viewer. | The layout crate already grew the frames. Reload discarded those returned sizes. The viewer now retains computed growth, including when an existing label gets wider. Layout, viewer, and CLI-to-watcher tests cover saved nested containers. |
| The client label was truncated despite being short. | The shared node width estimate now reserves room for the client icon and widens up to the existing 280-unit maximum. Both fixture client labels have enough estimated text space. Labels beyond the cap can still truncate; unlimited labels require a separate multiline typography policy. |
| A viewer launch without a display waited without a useful result. | The Linux entry point now reports a missing X11/Wayland display immediately. |
| Runtime failures without `--json` used plain stderr despite the documented error contract. | Runtime failures now consistently emit the JSON error envelope. Clap argument errors retain their documented text output and exit code 2. |
| Long routes and wide left-to-right layers remain on these connected designs. | General route obstacle avoidance and more compact layer assignment are follow-ups. This pass fixes label collisions while retaining the specified layered layout and pin semantics. Fit-level readability remains a display acceptance check. |

The sandbox also ran out of disk while linking several GPUI test binaries.
Failed linker artifacts were removed and compilation was serialized. Linked
executable debug information was stripped, using a temporary `cc` wrapper
outside the checkout that adds `-Wl,--strip-debug`. No CI workflow, lint
setting, source profile, or dependency was changed for this environment issue.

Export and in-window creation remain outside the first-version scope. They
are possible next-version work, not part of this acceptance pass.

## Automated coverage

- Both example files validate and serve as layout and viewer fixtures.
- Layout checks deterministic output, container padding, preserved pins, and
  edge-label separation from other labels and nodes.
- Viewer scene checks verify shared edge geometry, clickable labels, and room
  for client labels.
- Reload checks add a node inside a saved nested container and widen it, retain
  frame growth, and keep existing positions.
- A CLI-to-viewer integration test uses the checkout fixture, drags a node,
  a whole container, and an external provider through the window's plain state
  functions, changes metadata through the actual CLI during each drag, and
  verifies saved positions survive. It then adds a nested node through the CLI
  and verifies the directory watcher reloads it within one second and grows
  the frame. This exercises viewer logic without mouse input or a window.
- Metadata edits, clear-flag conflicts, unchanged JSON output, readable text
  output, missing-display errors, and the runtime error envelope have tests.

## Display acceptance: orchestrator to perform

There is no desktop display in this sandbox. The original
`sysy ui examples/checkout.json` launch was attempted and terminated after it
waited without a visible window. The corrected command reports the missing
display. No visual or mouse-based check is claimed here.

On a display, use a disposable copy so the fixture stays unchanged:

```sh
cp examples/checkout.json /tmp/checkout-visual.json
sysy ui /tmp/checkout-visual.json
```

1. Press `f`. Check that the whole design is visible and labels are readable
   at fit level without zooming into individual labels. Inspect nested frame
   headings, note placement, all node kinds present, and all four edge styles.
2. Open `examples/swarmy.json` too. Inspect Inference jobs, Scan runnable
   sessions, Reap expired leases, and Live events for separated, legible labels
   with clear associations to their edges. Check the full client label.
3. In the checkout copy, drag `shopper`, `fulfillment`, and `processor`.
   Confirm the fulfillment frame moves with its descendants, positions survive
   closing and reopening, and pan, zoom, selection, and fit still work.
4. While the window is open, run:

   ```sh
   sysy set /tmp/checkout-visual.json --description 'Updated during review'
   sysy node set /tmp/checkout-visual.json checkout --label 'Order checkout API'
   sysy node add /tmp/checkout-visual.json reconciler --kind service --label 'Payment reconciler' --in payments
   ```

   Confirm the label and new node appear promptly, the payment and commerce
   frames contain the new node, selection and camera stay stable, and saved
   dragged positions remain in place. Run `sysy layout` on the copy, reopen it,
   and confirm the grown sizes and positions persist.
5. Make a CLI metadata edit during an active drag and release it. Verify both
   the architecture edit and dragged position survive.

The agent-authoring part of the goal is confirmed by the CLI-only fixture.
Automated tests verify reading geometry, arranging and saving positions, and
seeing later edits through the actual file watcher. The complete human-facing
goal is pending the orchestrator's display checks above.
