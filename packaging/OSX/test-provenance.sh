#!/usr/bin/env bash
# Exercise source provenance with real, disposable Git repositories; no Rust builds.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
source "$ROOT/packaging/OSX/provenance.sh"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/notecraft-provenance.XXXXXX")"
trap 'rm -rf -- "$WORK"' EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
REPO="$WORK/source"
git init -q "$REPO"
printf '0.0.8\n' > "$REPO/version"
printf 'original\n' > "$REPO/input.rs"
git -C "$REPO" add .
# Only the disposable fixture receives commit objects. The contribution is untouched.
export GIT_AUTHOR_NAME='Provenance Test' GIT_AUTHOR_EMAIL='test@example.invalid'
export GIT_COMMITTER_NAME="$GIT_AUTHOR_NAME" GIT_COMMITTER_EMAIL="$GIT_AUTHOR_EMAIL"
tree="$(git -C "$REPO" write-tree)"
first="$(printf 'fixture one\n' | git -C "$REPO" commit-tree "$tree")"
git -C "$REPO" update-ref HEAD "$first"
original="$(source_provenance "$REPO")"
check_source_provenance "$original" "$original" 1

reject() {
  if check_source_provenance "$1" "$2" 1 > "$WORK/output" 2>&1; then
    echo "unexpected provenance acceptance: $3" >&2; exit 1
  fi
  grep -Fq 'source provenance' "$WORK/output"
  echo "rejected $3"
}

second="$(printf 'fixture two, same version and tree\n' | git -C "$REPO" commit-tree "$tree" -p "$first")"
git -C "$REPO" update-ref HEAD "$second"
current="$(source_provenance "$REPO")"
reject "$original" "$current" 'same-version different revision'
# The same validation runs for every slice before lipo is allowed to combine them.
check_source_provenance "$current" "$current" 1
reject "$original" "$current" 'universal slice with different source provenance'

printf 'unstaged\n' > "$REPO/input.rs"
dirty="$(source_provenance "$REPO")"
reject "$current" "$dirty" 'unstaged source change'
check_source_provenance "$dirty" "$dirty" 0
reject "$dirty" "$dirty" 'reuse of an unchanged dirty build'
git -C "$REPO" add input.rs
reject "$current" "$(source_provenance "$REPO")" 'staged source change'
printf 'another edit\n' > "$REPO/input.rs"
if check_source_provenance "$dirty" "$(source_provenance "$REPO")" 0 > "$WORK/output" 2>&1; then
  echo 'source changed during build but was accepted' >&2; exit 1
fi
# Restore only the disposable fixture, never the user's checkout.
printf 'original\n' > "$REPO/input.rs"
git -C "$REPO" add input.rs
printf 'new source\n' > "$REPO/new input.rs"
untracked="$(source_provenance "$REPO")"
reject "$current" "$untracked" 'untracked source input'
printf 'changed new source\n' > "$REPO/new input.rs"
[[ "$untracked" != "$(source_provenance "$REPO")" ]]
git -C "$REPO" update-index --assume-unchanged input.rs
if source_provenance "$REPO" > "$WORK/output" 2>&1; then
  echo 'hidden tracked changes were accepted' >&2; exit 1
fi
grep -Fq 'cannot establish source provenance' "$WORK/output"
echo 'source provenance regression checks passed'
