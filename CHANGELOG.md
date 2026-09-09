# Changelog

All notable changes to mcp-multiplexer. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning is
[SemVer](https://semver.org/).

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

[0.3.1]: https://github.com/johgirard/mcp-multiplexer/compare/v0.2.1...v0.3.1
[0.2.1]: https://github.com/johgirard/mcp-multiplexer/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/johgirard/mcp-multiplexer/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/johgirard/mcp-multiplexer/releases/tag/v0.1.0
