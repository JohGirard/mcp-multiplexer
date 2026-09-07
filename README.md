# mcp-multiplexer

One MCP server fronting many. Point your AI client at the aggregator and it
presents **5 meta-tools** instead of every upstream server's full tool schemas —
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

The model discovers tools lazily — list and search first, fetch a full schema
only when it's about to call. The tool index is cached at
`~/.cache/mcp-multiplexer/index.json` for instant startup, and upstream servers
connect lazily.

## Install

```sh
cargo install mcp-multiplexer
```

## Configuration

Standard `mcpServers` format (Claude Code / Claude Desktop compatible), plus
per-server extras:

- `expose`: boolean — this server's tools appear directly as `server__tool`,
  bypassing the meta-tools.
- `allow`: list of exact names or `prefix*` globs — only these tools are visible.
- `deny`: list, always wins over `allow`.

```json
{
  "$schema": "https://raw.githubusercontent.com/johgirard/mcp-multiplexer/main/schema.json",
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/home/me/docs"],
      "allow": ["read_file", "list_directory"]
    },
    "web": {
      "url": "https://example.com/mcp",
      "headers": { "Authorization": "Bearer token" },
      "deny": ["admin_*"]
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

## Claude Code

Replace all your `mcpServers` entries with one pointing at the aggregator:

```json
{
  "mcpServers": {
    "aggregator": {
      "command": "mcp-multiplexer",
      "args": ["--config", "/home/me/.mcp.json"]
    }
  }
}
```

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
- No auth flows for remote servers (static headers only).
- No env-var expansion in the config file.

## Development

`src/bin/mcp-mock.rs` builds an `mcp-mock` dev binary (echo/add/fail tools)
used by the integration tests.

## License

MIT OR Apache-2.0.
