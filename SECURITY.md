# Security Policy

## Reporting a vulnerability

Please report vulnerabilities privately — **not** in a public issue:

- **Preferred:** [GitHub private vulnerability reporting](https://github.com/JohGirard/mcp-multiplexer/security/advisories/new)
  ("Report a vulnerability" on the repo's Security tab)
- **Fallback:** email johannes.girard@gmail.com

Include the version (`mcp-multiplexer --version`), your config with secrets
redacted, and steps to reproduce. You'll get an acknowledgement within a few
days; fixes are released as patch versions once ready.

## Supported versions

Only the latest release receives security fixes.

| Version | Supported |
|---|---|
| latest release | ✅ |
| older releases | ❌ |

## Scope notes

Things that are **in scope** and worth reporting:

- Secrets leaking: tokens from `~/.cache/mcp-multiplexer/tokens.json` or
  `env`/`headers` values ending up in logs, tool output, or error messages
  visible to the model
- OAuth flaws: token exchange, storage permissions, redirect handling
- Config `allow`/`deny` rules being bypassed on the `call_tool` path
- Malicious upstream servers attacking downstream clients through the proxy
  (or vice versa)

Things that are **by design**, not vulnerabilities:

- mcp-multiplexer spawns and proxies arbitrary local commands from your own
  config file — controlling the config means controlling the process
- Tool results are passed through verbatim; an upstream server you configured
  can return anything it likes, including prompt injection aimed at the model.
  Only connect servers you trust, and use `allow`/`deny` to constrain them
- The stdio protocol channel is unauthenticated by nature (it's your client
  talking to your process)
