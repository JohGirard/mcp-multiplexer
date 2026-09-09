# mcp-multiplexer

[![CI](https://github.com/johgirard/mcp-multiplexer/actions/workflows/ci.yml/badge.svg)](https://github.com/johgirard/mcp-multiplexer/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/mcp-multiplexer.svg)](https://crates.io/crates/mcp-multiplexer)
[![Listed on mcpservers.org](https://mcpservers.org/badge.svg)](https://mcpservers.org/servers/johgirard/mcp-multiplexer)

One MCP server fronting many. Point your AI client at the multiplexer and it
presents **7 meta-tools** instead of every upstream server's full tool schemas —
slashing the tokens spent loading tool definitions into the model's context at
session start.

## What it does

Every MCP server you configure pushes all of its tool schemas into the model's
context up front. With a dozen servers that's tens of thousands of tokens the
model mostly never uses. mcp-multiplexer is **one** MCP server (stdio command-
based and remote HTTP upstreams) that exposes only:

| Meta-tool | What it returns |
|---|---|
| `list_servers` | Overview: name, status, tool count, instructions |
| `list_tools(server)` | Tool names, one-line descriptions, annotations — no schemas |
| `search_tools(query, server?, limit=5)` | Matching tools **with full input schemas** |
| `describe_tool(server, tool)` | One exact tool's full input schema |
| `call_tool(server, tool, arguments)` | Proxied call; results returned verbatim |
| `refresh_tools(server?)` | Reconnect and rebuild the tool index |
| `authorize_server(server, pasted_url?)` | Start/complete OAuth login for a server |

The model discovers tools lazily — list and search first, fetch a full schema
only when it's about to call. The tool index is cached at
`~/.cache/mcp-multiplexer/index.json` for instant startup, and upstream servers
connect lazily.

## Install

**Prebuilt binary** (Linux x86_64/ARM64, macOS Intel/ARM, Windows x86_64 — no
Rust needed): grab the archive for your platform from
[Releases](https://github.com/johgirard/mcp-multiplexer/releases) and put
`mcp-multiplexer` on your `PATH`.

```sh
# example: Linux x86_64
curl -L https://github.com/johgirard/mcp-multiplexer/releases/latest/download/mcp-multiplexer-x86_64-unknown-linux-musl.tar.gz | tar xz
sudo install mcp-multiplexer /usr/local/bin/
```

**From source:**

```sh
cargo install mcp-multiplexer
```

Either also installs `mcp-mock`, a tiny echo server used by the test suite —
harmless, ignore it.

## Configuration

Standard `mcpServers` format (Claude Code / Claude Desktop compatible), plus
per-server extras:

- `expose`: boolean — this server's tools also appear directly as
  `server__tool`, bypassing the meta-tools. See
  [Hybrid mode: `expose`](#hybrid-mode-expose).
- `allow`: list of exact names or `prefix*` globs — only these tools are visible.
- `deny`: list, always wins over `allow`.
- `connect_timeout`: seconds — connection/startup timeout (default 10). Raise
  it for slow-to-start local servers, e.g. `uvx --from git+…` that builds on
  every cold start.

Strings in `command`, `args`, `env`, `url`, and `headers` support `${VAR}`
environment expansion (same as Claude Code). An unset variable or unclosed
`${` fails startup with a clear error — so keep secrets out of the config:

```json
{
  "$schema": "https://raw.githubusercontent.com/johgirard/mcp-multiplexer/main/schema.json",
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/home/me/docs"],
      "allow": ["read_file", "list_directory"]
    },
    "github": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "env": { "GITHUB_TOKEN": "${GITHUB_TOKEN}" }
    },
    "web": {
      "url": "https://example.com/mcp",
      "headers": { "Authorization": "Bearer ${API_TOKEN}" },
      "deny": ["admin_*"]
    },
    "linear": {
      "url": "https://mcp.linear.app/mcp",
      "oauth": true
    },
    "fast": {
      "command": "mcp-fast-server",
      "expose": true
    }
  }
}
```

Run with `mcp-multiplexer --config /path/to/.mcp.json` (defaults to
`./.mcp.json`).

## Hybrid mode: `expose`

Multiplexing trades a discovery hop (`list_tools` → `describe_tool` →
`call_tool`) for a near-empty startup context. For servers you call *every
session, many times*, that hop is pure overhead — you already know the tool,
and its schema costs a handful of tokens. Set `"expose": true` and that
server's tools mount **directly** as first-class `server__tool` tools, schema
and all, next to the meta-tools:

```json
"serena": { "command": "uvx", "args": ["serena", "start-mcp-server"], "expose": true }
```

The model then calls `serena__find_symbol` like any directly-connected tool —
no search, no describe, no proxy hop. The server stays reachable through the
meta-tools too, and `allow`/`deny` still apply. Rule of thumb: multiplex the
fleet, expose the favorites.

## OAuth

Remote servers that speak OAuth 2.1 (the MCP authorization spec) work with
`"oauth": true` on a `url` server — no other setup in the common case:

- First use fails with an authorization URL. Open it, approve; a temporary
  `127.0.0.1` listener completes the exchange. Retry the call and it works.
  The model can also drive this itself via the `authorize_server` meta-tool.
- Tokens live in `~/.cache/mcp-multiplexer/tokens.json` (mode 0600), refresh
  automatically, and survive restarts — authorize once per server.

Scopes are auto-discovered; dynamic client registration is used when the
provider supports it — and self-heals when a provider forgets the
registration. Providers that delegate auth to a different domain than the MCP
endpoint (Freshworks-style cross-domain issuers) work out of the box. Full
guide — headless paste flow, `oauth_client_id` / `oauth_scopes` /
`oauth_redirect_port` tuning, provider notes (GitLab's group toggle gotcha),
troubleshooting: **[docs/oauth.md](docs/oauth.md)**.

## Claude Code

Replace all your `mcpServers` entries with one pointing at the multiplexer:

```json
{
  "mcpServers": {
    "mux": {
      "command": "mcp-multiplexer",
      "args": ["--config", "/home/me/.mcp.json"]
    }
  }
}
```

**Plugin (does this for you):** this repo is a Claude Code plugin whose
`setup` skill installs the binary, migrates your existing servers into a
multiplexer config (secrets become `${VAR}` references, OAuth servers get
flagged), rewires your client config, and verifies:

```
/plugin marketplace add JohGirard/mcp-multiplexer
/plugin install mcp-multiplexer@mcp-multiplexer
```

Then ask Claude to "set up mcp-multiplexer" (or run `/mcp-multiplexer:setup`).

## When upstream tools change

The index is built on first connect and cached on disk. If an upstream server
adds or removes tools, call `refresh_tools` (optionally with a server name) to
re-index — no restart needed. `call_tool` also self-heals: a failed call
triggers one reconnect, re-index, and retry before surfacing the error.

## Docker

```sh
docker build -t mcp-multiplexer .
docker run -i -v $HOME/.mcp.json:/config/.mcp.json:ro mcp-multiplexer
```

Docker mode is for **HTTP/remote upstreams only** — stdio upstreams need their
runtimes (node, uv, …) inside the image.

## Logging

- `--log-file <path>` appends logs to a file; otherwise logs go to stderr
  (stdout is protocol-only — do not pipe or redirect it).
- `RUST_LOG` env var controls the level (e.g. `RUST_LOG=debug`); `--verbose`
  is shorthand for debug logging.

## Stats

`mcp-multiplexer --stats` prints what the mux is saving you (no server start;
reads `~/.cache/mcp-multiplexer/`):

```
Startup context per session:
  without mux: ~16333 tokens (46 tools)
  with mux:    ~553 tokens (meta-tools)
  saved:       ~15780 tokens (96%)
```

(example: one GitLab server) plus on-demand schema bytes served, proxied call
counts, and per-meta-tool usage. Tokens are estimated as bytes/4 — a
heuristic, not a real tokenizer. With the Claude Code plugin installed,
`/mcp-multiplexer:gain` shows the same report in chat.

## Non-goals

- MCP resources and prompts (tools only).
- Upstream sampling, elicitation, and roots.
- No truncation of tool results — returned verbatim.
- OAuth for `url` servers only (stdio servers use `env` for secrets).
- No `tools/list_changed` notification forwarding — use `refresh_tools`.

## Development

Contributions welcome — see [CONTRIBUTING.md](CONTRIBUTING.md) for setup and
the CI checklist, and [SECURITY.md](SECURITY.md) for reporting vulnerabilities.

`src/bin/mcp-mock.rs` builds an `mcp-mock` dev binary (echo/add/fail tools)
used by the integration tests.

```sh
cargo test
```

Debug interactively with the MCP Inspector — note the `--`, which keeps the
inspector's own `--config` flag from eating ours:

```sh
npx @modelcontextprotocol/inspector --web -- \
  mcp-multiplexer --config /path/to/.mcp.json
```

## Releases

[CHANGELOG.md](CHANGELOG.md) documents every release. Cutting one:

1. Add a `## [X.Y.Z]` section to CHANGELOG.md and bump `version` in Cargo.toml.
2. Commit as `chore: release vX.Y.Z`, tag `vX.Y.Z`, push commit and tag.

The tag workflow verifies the tag matches the crate version, creates the
GitHub release with the changelog section as its notes, attaches per-platform
binaries **with SHA256 checksums**, publishes to crates.io, and pushes
`ghcr.io/johgirard/mcp-multiplexer` images tagged `latest` and `vX.Y.Z`.

## License

MIT. Free for any use, including commercial — the only requirement is keeping the copyright notice.
