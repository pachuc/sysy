# sysy

System designs as data. Agents build a design through the `sysy` command line
and people read it in a desktop window. Start with [docs/DESIGN.md](docs/DESIGN.md).

## Usage

Install Rust with rustup, then install from a checkout:

```sh
git clone https://github.com/pachuc/sysy.git
cd sysy
rustup show
cargo install --locked --path crates/sysy-cli
export PATH="$HOME/.cargo/bin:$PATH"
```

Create a design once the architecture is agreed:

```sh
sysy --json new design.json --title "Checkout"
sysy --json node add design.json api --kind service --label "Checkout API"
sysy --json node add design.json orders --kind database --label "Orders DB"
sysy --json edge add design.json persist --from api --to orders --kind data --label "Save orders"
sysy --json validate design.json
sysy --json show design.json
```

The viewer is still being built. Once available, open the design with
`sysy ui design.json`; layout will be automatic. The current CLI supports
creating, editing, and validating designs without a window.

Agents should read [skills/sysy/SKILL.md](skills/sysy/SKILL.md) for the authoring
workflow and its [generated reference](skills/sysy/reference.md) for exact flags.
See [examples/swarmy.json](examples/swarmy.json) for an agent swarm design with
services, state, a client, and an external model provider.
