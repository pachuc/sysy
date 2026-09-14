# Working on sysy

sysy is a local tool for system designs: agents build a typed design through
the `sysy` command line, and people read it in a GPUI window. Read
`docs/DESIGN.md` before changing anything; it defines the model, the file
format, the commands, the layout rules, and the viewer. The task text you were
given is the source of truth for scope.

## Building and testing

The toolchain is pinned in `rust-toolchain.toml`; `rustup show` installs it.
Every pull request must pass the same commands CI runs:

```sh
cargo fmt --all -- --check
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
```

Clippy runs with the `pedantic` group denied, so write code that satisfies it
rather than silencing it. If a lint is genuinely wrong for a piece of code,
allow that one lint at the narrowest scope with a comment explaining why.
`unsafe_code` is forbidden workspace-wide.

GPUI needs system libraries to build on Linux; the CI workflow lists them and
the development image has them installed. There is no display in CI or in the
sandbox, so the viewer's logic that can be tested without a window (layout
input, hit testing, file watching, position persistence) must live in plain
functions with unit tests, and the window code stays thin.

## Code conventions

- Rust 2024 edition. Add dependencies to `[workspace.dependencies]` in the
  root `Cargo.toml` and reference them with `workspace = true` from crates.
- The model, file format, and validation live in `sysy-core`; layout in
  `sysy-layout`; the window in `sysy-ui`; the binary in `sysy-cli`.
- Use `thiserror` for library errors and `anyhow` only in the binary.
- The CLI always supports `--json`, prints the affected record on success,
  and prints `{"error":{"message":"..."}}` on stderr with exit 1 on failure.
- Keep `Cargo.lock` committed and up to date.

## Writing

- Never use the section sign symbol (U+00A7) anywhere: code, comments, docs,
  commit messages, or pull request text. Write "section" instead.
- Write comments and docs in plain English with straightforward sentences.
  Explain why, not what, and do not use analogies.

## Pull requests

- One task per pull request, on the branch the launcher created. Do not touch
  files outside the task's scope, and do not weaken lints, tests, or CI.
- Commit messages have an imperative subject line and a body explaining why.
- The pull request description says what was built, lists the exact commands
  you ran to validate it with their results, and notes anything from the test
  plan you could not verify in the sandbox and why.
- Never commit secrets or `target/`.
