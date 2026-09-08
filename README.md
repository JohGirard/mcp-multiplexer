# mcp-multiplexer

[![CI](https://github.com/johgirard/mcp-multiplexer/actions/workflows/ci.yml/badge.svg)](https://github.com/johgirard/mcp-multiplexer/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/mcp-multiplexer.svg)](https://crates.io/crates/mcp-multiplexer)

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

**Prebuilt binary** (Linux x86_64, macOS Intel/ARM, Windows x86_64 — no Rust
needed): grab the archive for your platform from
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

- `expose`: boolean — this server's tools appear directly as `server__tool`,
  bypassing the meta-tools.
- `allow`: list of exact names or `prefix*` globs — only these tools are visible.
- `deny`: list, always wins over `allow`.

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

## OAuth

Remote servers that speak OAuth 2.1 (the MCP authorization spec) are handled
with `"oauth": true` on a `url` server — no other setup needed in the common
case:

- First use fails with an error containing an authorization URL. Open it in a
  browser and approve; a temporary `127.0.0.1` listener catches the redirect
  and completes the exchange. Retry the call and it works. The model can also
  drive this itself via the `authorize_server` meta-tool.
- Tokens live in `~/.cache/mcp-multiplexer/tokens.json` (mode 0600). Refresh
  is automatic and survives restarts — you authorize once per server.
- **Headless** (SSH, Docker): open the URL anywhere, then call
  `authorize_server` with `pasted_url` set to the final redirect URL
  (`http://127.0.0.1:.../callback?code=...`) your browser tried to reach.

Optional per-server tuning: `oauth_client_id` (skip dynamic registration with
a pre-registered client), `oauth_scopes` (list), `oauth_redirect_port` (fixed
callback port for providers that require an exact pre-registered redirect
URI). Static `headers` and OAuth can coexist; the OAuth Bearer token wins.

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

Releases are tagged `v*`; CI runs tests/clippy/fmt on push and publishes to
crates.io and ghcr.io on tags.

## License

MIT. Free for any use, including commercial — the only requirement is keeping the copyright notice.
