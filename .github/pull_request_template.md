## What & why

<!-- One paragraph: what changed and why. Link the issue if there is one. -->

## Checklist

- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` pass
- [ ] `cargo test` passes (added/updated tests for behavior changes)
- [ ] `schema.json` regenerated (`cargo run -- --dump-schema > schema.json`) if CLI args or the config format changed
- [ ] README updated if user-facing behavior changed
