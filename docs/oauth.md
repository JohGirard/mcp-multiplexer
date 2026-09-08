# OAuth

mcp-multiplexer brokers OAuth 2.1 for remote MCP servers so the model (and
you) never touches tokens. Set `"oauth": true` on a `url` server and the first
use walks you through a browser login; after that, tokens refresh themselves.

Standards implemented (via [rmcp](https://crates.io/crates/rmcp)): OAuth 2.1
with PKCE (S256), authorization-server metadata discovery (RFC 8414),
protected-resource metadata (RFC 9728), dynamic client registration
(RFC 7591), and resource indicators (RFC 8707).

## Quick start

```json
{
  "mcpServers": {
    "gitlab": {
      "url": "https://gitlab.com/api/v4/mcp",
      "oauth": true
    }
  }
}
```

1. Call any tool (or `authorize_server`) — you get an authorization URL.
2. Open it, log in, approve. A temporary `127.0.0.1` listener catches the
   redirect and completes the exchange automatically.
3. Retry the call. Done — no re-login on restart, tokens auto-refresh.

The model can drive this itself: the `authorize_server` meta-tool returns the
URL with instructions, and its result tells the model what to do next.

## How it works

```
1. call_tool(gitlab, …)
2. mux: no token → discover provider metadata
   (WWW-Authenticate → .well-known/oauth-protected-resource
    → .well-known/oauth-authorization-server)
3. mux: register a client dynamically (skipped if you set oauth_client_id)
4. mux → you: authorization URL (scope auto-discovered, PKCE challenge,
   resource indicator)
5. browser: you approve → redirect to http://127.0.0.1:<port>/callback
6. mux: code exchange → tokens.json → retry succeeds
```

Scope is discovered from the provider's protected-resource metadata — you
normally don't set `oauth_scopes` at all.

## Configuration reference

| Key | Type | Default | Purpose |
|---|---|---|---|
| `oauth` | bool | `false` | Enable OAuth for this `url` server |
| `oauth_client_id` | string | — | Pre-registered client ID. Skips dynamic registration — needed for providers that don't support RFC 7591 |
| `oauth_scopes` | string[] | auto | Override scopes when auto-discovery picks wrong ones or the provider has no metadata |
| `oauth_redirect_port` | number | ephemeral | Fix the callback port. Needed when your pre-registered OAuth app requires an exact redirect URI (`http://127.0.0.1:<port>/callback`) |

Static `headers` and OAuth can coexist; the OAuth Bearer token wins on the
`Authorization` header.

## Headless (SSH, Docker, no local browser)

The callback listener can't catch a redirect on a machine without a browser,
so paste instead:

1. `authorize_server` (no `pasted_url`) → open the URL **anywhere**.
2. Approve. Your browser tries to load `http://127.0.0.1:…/callback?code=…`
   and fails — that's fine.
3. Copy that final URL from the address bar and call
   `authorize_server` with `pasted_url` set to it.

## Token storage

- Location: `~/.cache/mcp-multiplexer/tokens.json` (`$XDG_CACHE_HOME` aware),
  mode `0600`, one entry per server.
- Contains access + refresh tokens and the dynamically-registered client ID
  (reused across restarts so providers don't accumulate clients).
- Refresh is automatic and transparent; a revoked refresh token simply
  triggers the authorization flow again on next use.
- **Log out:** delete the server's entry from `tokens.json`. Also revoke the
  grant at the provider if you want it fully dead.

## Provider notes

### GitLab (`https://gitlab.com/api/v4/mcp`)

Validated end-to-end against gitlab.com (2026-09, GitLab 19.4): dynamic
registration, scope discovery, refresh all work.

- **Available on all tiers including Free**, but an **owner must enable it per
  top-level group**: Group → **Settings → General → Access and permissions** →
  "Allow access to the MCP server". Your personal namespace does not count —
  it must be a group. If you have none, create a free one.
- Symptom when not enabled: `HTTP 404` on the MCP endpoint *after* successful
  authorization (confusingly, the OAuth flow itself works fine).

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `HTTP 404` after authorizing (GitLab) | MCP not enabled on any top-level group | see Provider notes above |
| `invalid URL, scheme is not http` | build without TLS (bug in v0.1.0) | upgrade to ≥ v0.1.1 |
| Provider rejects the redirect URI | pre-registered app requires an exact URI | set `oauth_redirect_port` and register `http://127.0.0.1:<port>/callback` |
| `oauth init … metadata discovery failed` | provider has no RFC 8414/9728 metadata, or wrong URL | check the `url`; if the provider is non-standard, file an issue |
| Dynamic registration refused | provider doesn't support RFC 7591 | create an OAuth app manually, set `oauth_client_id` (+ `oauth_redirect_port`) |
| Constantly asked to re-authorize | refresh token revoked/expired | expected: re-approve once; check provider's token TTL settings if it recurs |
| Want to force re-login | — | delete the server's entry in `tokens.json` and retry |
