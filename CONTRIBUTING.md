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

## Release flow

Releases are automated — you never tag or bump versions by hand:

1. **Land PRs on `main`** with conventional commit subjects (`feat:`, `fix:`,
   `perf:`, …). The subject prefix drives the next version number.
2. **A release PR opens itself.** On every push to `main`,
   `release-pr.yml` runs `scripts/release-prepare.sh`, which computes the
   bump from the commits since the last tag (`feat`/`perf` → minor,
   `fix`/`refactor` → patch, breaking → minor on 0.x / major on ≥1.0; only
   `chore`/`docs`/`ci` → no release), then updates every version reference —
   `Cargo.toml`, `Cargo.lock`, `.claude-plugin/plugin.json` — drafts the
   `CHANGELOG.md` section (grouped bullets + compare link), and opens
   `release/vX.Y.Z`.
3. **Review and merge the release PR.** The changelog section is generated
   from commit subjects — edit the PR to make the prose read like the
   sections above before merging. This is the curation step; the release
   notes are exactly what ships.
4. **Merging triggers `release.yml`.** It verifies the version was actually
   bumped, creates the tag and GitHub release, builds binaries ×5, publishes
   to crates.io, pushes the Docker image, then a `verify-install` job proves
   every install path works: release assets download, crates.io serves the
   version, the ghcr.io manifest exists, and the plugin manifest is in sync.

CI's `versions` job fails any PR where `.claude-plugin/plugin.json` and
`Cargo.toml` disagree, so the plugin manifest can't silently go stale again.

Manual escape hatches:

- Cut a release without releasable commits (e.g. dependency-only) —
  *Actions → Release PR → Run workflow* with an explicit level.
- Re-run a botched release — *Actions → Release → Run workflow* with the tag
  (must match `Cargo.toml`).
- Test the whole flow locally: `scripts/release-prepare.sh --dry-run`.

Tag creation and `RELEASE_TOKEN`:

The repo's `tags` ruleset restricts ref creation, and GitHub doesn't allow
`github-actions[bot]` as a bypass actor on personal-repo rulesets — so the
default `GITHUB_TOKEN` cannot create release tags. `release.yml` therefore
uses the `RELEASE_TOKEN` secret when set and falls back to `GITHUB_TOKEN`.
Set up the secret once: a fine-grained PAT scoped to this repository with
**Contents: read/write** only (its owner must be a bypass actor on the
`tags` ruleset — repo admins are by default). Without the secret, releases
still work via the manual hatch: push the `vX.Y.Z` tag yourself (bypass
actors may), and the tag push triggers `release.yml` as usual.

## License

By contributing you agree your work is licensed under the project's
[MIT license](LICENSE).
