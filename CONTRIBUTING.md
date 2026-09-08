# Contributing

Thanks for considering a contribution. Bug reports, docs fixes, and code are
all welcome — for anything bigger than a small fix, open an issue first so we
can agree on the shape before you spend time on it.

## Setup

Stable Rust, nothing else:

```sh
cargo build
cargo test
```

`src/bin/mcp-mock.rs` builds an `mcp-mock` echo server that the integration
tests drive over stdio — tests are self-contained, no network or credentials
needed.

To poke at the binary interactively, use the MCP Inspector (note the `--`,
which keeps the inspector's own `--config` flag from eating ours):

```sh
npx @modelcontextprotocol/inspector --web -- \
  target/debug/mcp-multiplexer --config /path/to/.mcp.json
```

## Before you push

CI runs all of these and will fail the PR otherwise:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

If you changed the CLI arguments or the JSON config schema, regenerate the
checked-in schema:

```sh
cargo run -- --dump-schema > schema.json
```

## Guidelines

- **Keep the token budget in mind.** The whole point of this project is small
  responses. Meta-tool output should stay terse — no decorative text in tool
  results.
- **Match the existing style.** Conventional-ish commit prefixes (`feat:`,
  `fix:`, `chore:`, `test:`, `docs:`), `anyhow` for errors, `tracing` for
  logs (never write to stdout — it's the protocol channel).
- **Tests for behavior changes.** Unit tests next to the code, end-to-end
  tests in `tests/` using `mcp-mock`.
- **One thing per PR.** Small PRs get reviewed fast; grab-bags get picked apart.

## Reporting security issues

Please don't open a public issue — see [SECURITY.md](SECURITY.md).

## License

By contributing you agree your work is licensed under the project's
[MIT license](LICENSE).
