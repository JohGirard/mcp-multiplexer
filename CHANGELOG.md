# Changelog

All notable changes to mcp-multiplexer. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning is
[SemVer](https://semver.org/).

## [0.5.0] - 2026-09-25

### Fixed

- resultType on proxied tools/call (SEP-2322) + automatic OAuth browser redirect (#13)

### Internal

- fix release-prepare.sh lock sync on fresh CI runners
- has none, so the Cargo.toml version bump could not sync into Cargo.lock
- ("no matching package named anyhow found"). Patch the lock's own
- package entry textually instead — deterministic, offline-safe, and
- ci.yml's locked builds on the release PR validate it afterwards.
- 
- 
- Clients that negotiate protocol 2026-07-28 (discover lifecycle) require
- resultType on every tools/call result. Mux's upstream links always use the
- legacy handshake (<= 2025-11-25), so proxied results arrive without the
- field and were forwarded verbatim — strict clients rejected every call_tool
- (meta-tool and exposed alike) as malformed. Normalize at the ups.call
- boundary; the server handler still strips the discriminator for legacy
- peers. Regression-asserted in the discover-lifecycle e2e.
- 
- * feat: open the OAuth authorization page in the browser automatically
- 
- begin_flow returned the provider URL as text only (error string or the
- authorize_server tool result), so nothing happened unless the client parsed
- the URL out of the message. The callback listener already binds 127.0.0.1
- on the mux host, so open the system browser there on both the new-flow and
- in-progress paths; headless machines fail gracefully and keep the printed
- URL. Messages and docs updated.
- automated releases — release PR flow, install verification, version sync (#12)
- nothing checked install paths after publishing.
- 
- Flow (documented in CONTRIBUTING.md 'Release flow'):
- - release-pr.yml runs scripts/release-prepare.sh on every main push:
-   derives the bump level from conventional commits since the last tag,
-   bumps Cargo.toml / Cargo.lock / plugin.json, drafts the CHANGELOG
-   section, and opens release/vX.Y.Z. The PR is the curation step.
- - Merging that PR triggers release.yml: a gate job detects the version
-   bump (normal main pushes no-op), then tag + GitHub release + binaries x5
-   + crates.io + docker as before, plus a new verify-install job proving
-   every advertised install path works on the released version.
- - ci.yml gains a versions job: plugin.json must match Cargo.toml.
- 
- Verified locally: script bump matrix (feat/fix/chore/breaking/override),
- gate logic for all trigger modes, cargo install --path, release asset
- downloads, crates.io + ghcr.io manifests for v0.4.0 (the crates.io check
- needs a User-Agent — 403 otherwise, caught during verification).
- 
- Also fixes the stale plugin.json (0.2.1 -> 0.4.0).

## [0.4.0] - 2026-09-24

### Fixed

- `tools/list` now includes the SEP-2549 cache hints (`ttlMs`, `cacheScope`)
  required by protocol version 2026-07-28. rmcp 3.4's `#[tool_handler]` macro
  adds them to its generated `list_tools`, but the multiplexer's hand-written
  override (which merges exposed upstream tools) unconditionally omitted them —
  clients on 2026-07-28 (e.g. Claude Code) rejected the entire tool list as
  invalid, making every upstream tool unreachable.

### Changed

- Server instructions now name all 7 meta-tools with a one-line purpose each
  (`list_servers`, `list_tools`, `search_tools`, `describe_tool`,
  `call_tool`, `refresh_tools`, `authorize_server`), so an agent can plan its
  calls straight from `initialize` — before any `tools/list` round-trip.

### Internal

- rmcp 3.2 → 3.4.1, replacing the deprecated `ServerInfo` alias with
  `ServerConfig`; clap 4.6.7; reqwest 0.13.5. GitHub Actions updated
  (actions/checkout v7, docker/login-action v4, and others).

## [0.3.1] - 2026-09-09

### Added

- `connect_timeout` per-server config (seconds, default 10) — slow-to-start
  local servers (e.g. `uvx --from git+…` building on cold start) no longer
  get killed mid-initialization.

### Fixed

- OAuth: providers that delegate authorization to a different domain than the
  MCP endpoint (resource on one host, declared `issuer` on another) failed
  discovery with `Authorization server issuer mismatch`. mux now falls back to
  trusting the issuer the discovery document declares (logged as a warning),
  per RFC 8414 §3.3's client-side reading.
- OAuth: a token refresh failing with `invalid_client` (provider lost the
  dynamic registration, e.g. non-durable DCR) now discards the stale
  registration and transparently re-registers instead of surfacing the raw
  error — you re-authorize once.

### Documentation

- Setup skill and OAuth guide: the fixed-first-party-app failure shape
  (provider-owned OAuth client that forbids loopback redirects — not
  mux-fixable; keep such servers as direct connectors), the now-auto-handled
  issuer-mismatch and `invalid_client` shapes.

## [0.2.1] - 2026-09-08

### Fixed

- OAuth: read the access token via the inner manager once the state machine
  reaches `Authorized` — in-process browser flows no longer report a bogus
  "Already authorized" error.
- Setup skill: document migrate-time auth failures (`Auth required` → set
  `oauth: true`; DCR-less providers need `oauth_client_id`).

## [0.2.0] - 2026-09-08

### Added

- Claude Code plugin with `setup` skill: installs the binary, migrates
  existing servers into a multiplexer config (secrets become `${VAR}`
  references, OAuth servers flagged), rewires the client, verifies.
- `--stats` report and runtime counters: startup-context token savings,
  proxied call counts, per-meta-tool usage; `/mcp-multiplexer:gain` plugin
  command.
- Prebuilt `aarch64-unknown-linux-musl` release binaries (native ARM runner).

### Fixed

- Enable rustls for rmcp's reqwest client — all https upstreams failed with
  `scheme is not http` in v0.1.0.

## [0.1.0] - 2026-09-08

Initial release.

- One stdio MCP server multiplexing many upstreams (stdio + HTTP) behind 7
  meta-tools: `list_servers`, `list_tools`, `search_tools`, `describe_tool`,
  `call_tool`, `refresh_tools`, `authorize_server`.
- Lazy upstream connections, on-disk tool-index cache, self-healing calls
  (reconnect → re-index → retry once).
- Per-server `expose` / `allow` / `deny` governance, `${VAR}` config
  expansion, JSON Schema (`--dump-schema`).
- OAuth 2.1 (PKCE) for remote servers with dynamic client registration.
- Prebuilt binaries for Linux, macOS, and Windows; crates.io and ghcr.io
  publishing.

[0.5.0]: https://github.com/johgirard/mcp-multiplexer/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/johgirard/mcp-multiplexer/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/johgirard/mcp-multiplexer/compare/v0.2.1...v0.3.1
[0.2.1]: https://github.com/johgirard/mcp-multiplexer/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/johgirard/mcp-multiplexer/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/johgirard/mcp-multiplexer/releases/tag/v0.1.0
