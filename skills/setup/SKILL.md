---
name: setup
description: Install mcp-multiplexer and migrate existing MCP servers into it — builds the multiplexer config (secrets as ${VAR}, OAuth flags, allow/deny), rewires the client to a single mux entry, and verifies. Use when the user wants to install, set up, or configure mcp-multiplexer, or migrate their MCP servers to it.
---

# mcp-multiplexer setup

Set up mcp-multiplexer end to end. Work through the steps in order. Ask the
user before destructive steps (config edits) and whenever a choice is
genuinely theirs (which servers to migrate, OAuth, governance); otherwise
proceed with the defaults given here.

## 1. Install the binary

Check first: `mcp-multiplexer --version`. If it runs, note the version and
skip to step 2.

Otherwise install a prebuilt binary (no sudo):

```sh
mkdir -p ~/.local/bin
# pick the asset by `uname -s`/`uname -m`:
#   Linux x86_64   mcp-multiplexer-x86_64-unknown-linux-musl.tar.gz
#   Linux ARM64    mcp-multiplexer-aarch64-unknown-linux-musl.tar.gz
#   macOS Intel    mcp-multiplexer-x86_64-apple-darwin.tar.gz
#   macOS ARM      mcp-multiplexer-aarch64-apple-darwin.tar.gz
#   Windows x86_64 mcp-multiplexer-x86_64-pc-windows-msvc.zip
curl -L https://github.com/johgirard/mcp-multiplexer/releases/latest/download/<asset> | tar xz -C ~/.local/bin
```

If `~/.local/bin` is not on the user's PATH, tell them how to add it (or use
`/usr/local/bin` with sudo). Fallback when cargo is available:
`cargo install mcp-multiplexer`. Verify with `mcp-multiplexer --version`.

## 2. Inventory existing MCP servers

Read, don't modify yet:

- Claude Code user scope: `~/.claude.json` → top-level `mcpServers`
- Project scope: `./.mcp.json` → `mcpServers`
- If the user mentions other clients: Cursor `~/.cursor/mcp.json`, etc.

Present the inventory. Then ask which servers to migrate. Recommendations:

- Migrate everything with more than a couple of tools — that's where the
  token savings are.
- Keep tiny (1–2 tool) or latency-critical servers direct, OR migrate them
  with `"expose": true` (tools appear as `server__tool`, bypassing the
  meta-tools).
- Never migrate mcp-multiplexer itself if already present.

## 3. Build the multiplexer config

Default path: `~/.config/mcp-multiplexer/servers.json` (offer it; accept
whatever the user prefers). For each migrated server:

- **stdio** (`command`/`args`/`env`): copy as-is, EXCEPT secrets — replace
  literal token values in `env` with `"${VAR_NAME}"` references. Never write
  a real token into the config. Afterwards, check each referenced var with
  `printenv VAR_NAME` and warn the user about any that are unset (the mux
  refuses to start on unset vars — by design, so typos don't silently drop
  credentials). Slow-to-start local servers (e.g. `uvx --from git+…` building
  on every cold start) can exceed the default 10s connect timeout — set
  `"connect_timeout": 60`.
- **url**: copy `url` and `headers`. Static bearer tokens →
  `"Authorization": "Bearer ${VAR_NAME}"`. OAuth login instead:
  `"oauth": true`. Known OAuth endpoints (set it without asking; otherwise
  ask):
  - `https://gitlab.com/api/v4/mcp`
  - `https://mcp.linear.app/mcp`
  - `https://mcp.notion.com/mcp`
  - `https://mcp.sentry.dev/mcp`
- **Governance** (ask once, don't interrogate per server): for servers whose
  tools should be restricted, set `allow` (exact names or `prefix*` globs)
  and/or `deny` (always wins). Skip both when the user wants full access.

Write the file with a `$schema` key for editor completion:

```json
{
  "$schema": "https://raw.githubusercontent.com/johgirard/mcp-multiplexer/main/schema.json",
  "mcpServers": { }
}
```

## 4. Rewire the client

Always back up first:

```sh
cp ~/.claude.json ~/.claude.json.bak-$(date +%Y%m%d)   # and/or ./.mcp.json
```

Then edit the SAME file(s) you inventoried in step 2: remove the migrated
entries from `mcpServers` and add one entry:

```json
"mux": {
  "command": "mcp-multiplexer",
  "args": ["--config", "/absolute/path/to/servers.json"]
}
```

Use an absolute path. Leave non-migrated servers untouched. Do NOT point the
client at the multiplexer config file itself — `mux` is a stdio command, the
config path is its argument.

## 5. Verify

Smoke-test that the config parses and the mux starts (it answers one
handshake on stdin, then exits):

```sh
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"setup-check","version":"0"}}}' \
  | mcp-multiplexer --config /absolute/path/to/servers.json 2>/dev/null
```

Expect a JSON response with `"serverInfo"`. If it errors, the message names
the problem (unset `${VAR}`, bad JSON, unknown key) — fix and re-run.

Then check each migrated url server actually connects — for every one, run a
`list_tools` through the mux (the same stdin pattern with a `tools/call`).
Two failure shapes have known fixes:

- `Auth required, when send initialize request` → the server needs OAuth and
  you didn't set it (clients like Claude Code hide this — they cache their
  own OAuth tokens). Set `"oauth": true` and re-check.
- `Dynamic client registration not supported` → the provider needs a
  pre-registered OAuth client. Get a client ID from the provider's admin,
  then set `oauth_client_id` (and `oauth_redirect_port` + registering
  `http://127.0.0.1:<port>/callback` if it requires exact redirect URIs —
  Okta-backed providers usually do).
- `AADSTS50011` (redirect URI mismatch) naming a client ID you never
  registered → the provider fronts a fixed first-party OAuth app (e.g.
  Microsoft's own) whose app registration forbids loopback redirects. Not
  fixable by mux — keep that server as a direct client connector.

Then tell the user:

1. **Restart Claude Code** (or run `/mcp` to reconnect) so the `mux` server
   is picked up.
2. First calls to OAuth servers return an authorization URL — open it,
   approve, retry. Details and provider notes:
   https://github.com/JohGirard/mcp-multiplexer/blob/main/docs/oauth.md
3. Daily use: the model drives `list_servers` → `search_tools` →
   `describe_tool` → `call_tool` on its own.

## Rollback

Restore the backup from step 4 and restart the client. The multiplexer config
can stay — it's inert until referenced.
