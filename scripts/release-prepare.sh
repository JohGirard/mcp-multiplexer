#!/usr/bin/env bash
# Open (or refresh) a release PR: bump the version everywhere it lives,
# draft the CHANGELOG section, push release/vX.Y.Z, open the PR.
#
# Usage:
#   scripts/release-prepare.sh [--level patch|minor|major] [--dry-run] [--no-pr]
#
# Level auto-detection (0.x aware) from conventional commits since the last tag:
#   feat / perf        -> minor
#   fix/refactor/...   -> patch
#   breaking ("!" or "BREAKING CHANGE" in body) -> minor on 0.x, major on >=1.0
#   only chore/ci/docs/build/test/style -> nothing to release
#
# The script is the single place that knows every version reference:
# Cargo.toml, Cargo.lock, .claude-plugin/plugin.json, CHANGELOG.md.
# ci.yml's version-sync job keeps them honest afterwards.

set -euo pipefail

LEVEL=""
DRY_RUN=0
NO_PR=0
REMOTE="${RELEASE_REMOTE:-origin}"
BASE_BRANCH="${RELEASE_BASE:-main}"
REPO_SLUG="${GITHUB_REPOSITORY:-johgirard/mcp-multiplexer}"

log() { printf '%s\n' "$*" >&2; }
die() { log "error: $*"; exit 1; }

while [ $# -gt 0 ]; do
    case "$1" in
        --level) LEVEL="${2:?--level needs patch|minor|major}"; shift 2 ;;
        --dry-run) DRY_RUN=1; shift ;;
        --no-pr) NO_PR=1; shift ;;
        -h|--help) sed -n '2,16p' "$0"; exit 0 ;;
        *) die "unknown argument: $1" ;;
    esac
done

command -v jq >/dev/null || die "jq is required"
command -v python3 >/dev/null || die "python3 is required"
[ -f Cargo.toml ] || die "run from the repository root"

current_version=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
[ -n "$current_version" ] || die "cannot read version from Cargo.toml"
last_tag=$(git describe --tags --abbrev=0 2>/dev/null || true)
log "current version: $current_version (last tag: ${last_tag:-none})"

# ---------------------------------------------------------------- commits ---
range="${last_tag:+$last_tag..}HEAD"
# %x1f separates subject from body, %x1e separates commits — the delimiter
# must be %x1e (not \n) because bodies span lines; the empty record git
# leaves after the trailing separator is dropped.
mapfile -d $'\x1e' -t raw < <(git log --no-merges --pretty=$'%s%x1f%b%x1e' "$range")
commits=()
for entry in "${raw[@]}"; do
    entry=${entry#$'\n'}
    subject=${entry%%$'\x1f'*}
    [ -n "$subject" ] && commits+=("$entry")
done
if [ ${#commits[@]} -eq 0 ]; then
    log "no commits since ${last_tag:-the beginning} — nothing to release"
    exit 0
fi

auto_level="none"
re_breaking='^[a-z]+(\([^)]*\))?!:'
re_minor='^(feat|perf)(\([^)]*\))?:'
re_patch='^(fix|refactor|revert)(\([^)]*\))?:'
for entry in "${commits[@]}"; do
    s=${entry%%$'\x1f'*}
    body=${entry#*$'\x1f'}
    if [[ "$s" =~ $re_breaking ]] || grep -q "BREAKING CHANGE" <<<"$body"; then
        l="breaking"
    elif [[ "$s" =~ $re_minor ]]; then
        l="minor"
    elif [[ "$s" =~ $re_patch ]]; then
        l="patch"
    else
        l="none"
    fi
    case "$l" in
        breaking) auto_level="breaking" ;;
        minor) [ "$auto_level" = "breaking" ] || auto_level="minor" ;;
        patch) [ "$auto_level" = "none" ] && auto_level="patch" ;;
    esac
done

if [ -n "$LEVEL" ]; then
    bump_level="$LEVEL"
elif [ "$auto_level" = "none" ]; then
    log "nothing to release: only chore/ci/docs/build/test commits since ${last_tag:-the beginning}"
    exit 0
elif [ "$auto_level" = "breaking" ]; then
    major=$(cut -d. -f1 <<<"$current_version")
    if [ "$major" -eq 0 ]; then bump_level="minor"; else bump_level="major"; fi
else
    bump_level="$auto_level"
fi

IFS=. read -r M m p <<<"$current_version"
case "$bump_level" in
    major) new_version="$((M+1)).0.0" ;;
    minor) new_version="$M.$((m+1)).0" ;;
    patch) new_version="$M.$m.$((p+1))" ;;
    *) die "invalid level: $bump_level" ;;
esac
new_tag="v$new_version"
log "release level: $bump_level -> $new_version ($new_tag)"

# ------------------------------------------------------------- changelog ---
# Group commit subjects by Keep-a-Changelog category, stripping the prefix.
category_of() {
    case "$1" in
        feat*) echo "Added" ;;
        fix*|revert*) echo "Fixed" ;;
        perf*) echo "Changed" ;;
        *) echo "Internal" ;;
    esac
}
strip_prefix() { sed -E 's/^[a-z]+(\([^)]*\))?!?: *//' <<<"$1"; }

declare -A groups=()
for entry in "${commits[@]}"; do
    s=${entry%%$'\x1f'*}
    c=$(category_of "$s")
    bullet="- $(strip_prefix "$s")"
    groups[$c]+="$bullet"$'\n'
done

today=$(date +%F)
section="## [$new_version] - $today"$'\n'
for c in Added Changed Fixed Internal; do
    [ -n "${groups[$c]:-}" ] || continue
    section+=$'\n'"### $c"$'\n\n'"${groups[$c]}"
done

if [ -n "$last_tag" ]; then
    compare="[$new_version]: https://github.com/$REPO_SLUG/compare/$last_tag...$new_tag"
else
    compare="[$new_version]: https://github.com/$REPO_SLUG/releases/tag/$new_tag"
fi

if [ "$DRY_RUN" = 1 ]; then
    log "--- CHANGELOG section to insert ---"
    printf '%s' "$section"
    log "--- compare link ---"
    printf '%s\n' "$compare"
    log "--- files that would change ---"
    printf 'Cargo.toml:      %s -> %s\n' "$current_version" "$new_version"
    printf 'Cargo.lock:      %s -> %s\n' "$current_version" "$new_version"
    printf 'plugin.json:     %s -> %s\n' "$(jq -r .version .claude-plugin/plugin.json)" "$new_version"
    log "(dry run — no changes made)"
    exit 0
fi

# ------------------------------------------------------------------ edits ---
sed -i "0,/^version = \".*\"/s//version = \"$new_version\"/" Cargo.toml
jq --arg v "$new_version" '.version = $v' .claude-plugin/plugin.json > .claude-plugin/plugin.json.tmp
mv .claude-plugin/plugin.json.tmp .claude-plugin/plugin.json
# Sync Cargo.lock's own package version textually — `cargo check --offline`
# needs a warm registry cache, which fresh CI runners don't have.
VERSION="$new_version" python3 - <<'EOF'
import os, re, pathlib

lock = pathlib.Path("Cargo.lock")
text = lock.read_text()
pattern = r'(\[\[package\]\]\nname = "mcp-multiplexer"\nversion = ")[^"]+(")'
new_text, n = re.subn(pattern, rf"\g<1>{os.environ['VERSION']}\g<2>", text)
assert n == 1, "mcp-multiplexer entry not found exactly once in Cargo.lock"
lock.write_text(new_text)
EOF

SECTION="$section" COMPARE="$compare" python3 - <<'EOF'
import os, re, pathlib

cl = pathlib.Path("CHANGELOG.md")
text = cl.read_text()
section = os.environ["SECTION"]
compare = os.environ["COMPARE"]

m = re.search(r"^## \[", text, re.M)
assert m, "no existing release section in CHANGELOG.md"
text = text[: m.start()] + section + "\n" + text[m.start():]

# keep compare links newest-first, directly above the previous first link
m = re.search(r"^\[[0-9][^\]]*\]: https://", text, re.M)
if m:
    text = text[: m.start()] + compare + "\n" + text[m.start():]
else:
    if not text.endswith("\n"):
        text += "\n"
    text += "\n" + compare + "\n"

cl.write_text(text)
EOF

grep -q "version = \"$new_version\"" Cargo.toml || die "Cargo.toml bump failed"
grep -A1 'name = "mcp-multiplexer"' Cargo.lock | grep -q "version = \"$new_version\"" \
    || die "Cargo.lock bump failed"
[ "$(jq -r .version .claude-plugin/plugin.json)" = "$new_version" ] || die "plugin.json bump failed"

# ---------------------------------------------------------------- release ---
branch="release/$new_tag"
git checkout -B "$branch" "$BASE_BRANCH" 2>/dev/null || git checkout -B "$branch"
git add Cargo.toml Cargo.lock .claude-plugin/plugin.json CHANGELOG.md
git commit -m "chore: release $new_tag" -m "$(printf '%s' "$section")"
git push --force-with-lease -u "$REMOTE" "$branch"
log "pushed $branch to $REMOTE"

if [ "$NO_PR" = 1 ]; then
    log "--no-pr: skipping pull request creation"
    exit 0
fi

command -v gh >/dev/null || die "gh is required to open the PR (or rerun with --no-pr)"

pr_body=$(cat <<EOF
## Release $new_tag

Automated by \`scripts/release-prepare.sh\` — bumps \`Cargo.toml\`, \`Cargo.lock\`,
\`.claude-plugin/plugin.json\`, drafts the \`$new_version\` CHANGELOG section.

**Before merging:** review the generated changelog prose below and edit it —
the sections are grouped from commit subjects, a starting point, not the final
word. Merging triggers \`release.yml\`: tag, GitHub release, binaries ×5,
crates.io, Docker, then install verification.

$section
EOF
)

existing=$(gh pr list --head "$branch" --json number --jq '.[0].number // empty' 2>/dev/null || true)
if [ -n "$existing" ]; then
    gh pr edit "$existing" --title "chore: release $new_tag" --body "$pr_body"
    log "updated existing PR #$existing"
else
    gh pr create --title "chore: release $new_tag" --body "$pr_body" --head "$branch" --base "$BASE_BRANCH"
fi
