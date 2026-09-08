---
description: Show how many context tokens mcp-multiplexer is saving you (startup withholding + on-demand schema serving)
---

Run:

```sh
mcp-multiplexer --stats
```

If the binary is not found, tell the user to install it first (see the setup
skill) — do not guess numbers.

Present the output compactly in chat:

- **Startup saved** is the headline: tokens that would have been injected by
  connecting all upstream servers directly, minus what the mux's own
  meta-tools cost.
- **On-demand schemas served** shows lazy loading working: the model only
  pulled full schemas for tools it actually used.
- **Tool calls proxied** is total upstream usage.

If the report says "No cached index yet", the mux hasn't run — the index is
built on first use. If saved is 0% with a tiny tool count, point out that's
expected for one small server; the win grows with every server added.

End with one line: tokens are estimated as bytes/4 (heuristic), and the raw
data lives in `~/.cache/mcp-multiplexer/`.
