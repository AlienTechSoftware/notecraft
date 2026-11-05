#!/usr/bin/env bash
# Shared source identity checks for packaging and its regression tests.
# Callers use set -euo pipefail. No index, Git objects or source files are written.

source_provenance() (
  cd "$1"
  local revision status state fingerprint path entries
  revision="$(git rev-parse --verify HEAD)" || exit 1
  status="$(git --no-optional-locks status --porcelain=v1 --untracked-files=all --ignore-submodules=none)" || exit 1
  # Submodule working trees need their own input tracking; fail closed until supported.
  entries="$(git ls-files --stage)" || exit 1
  if printf '%s\n' "$entries" | grep '^160000 ' >/dev/null; then
    echo 'cannot establish source provenance for submodules' >&2; exit 1
  fi
  entries="$(git ls-files -v)" || exit 1
  if printf '%s\n' "$entries" | LC_ALL=C grep '^[a-zS] ' >/dev/null; then
    echo 'cannot establish source provenance with assume-unchanged or skip-worktree entries' >&2; exit 1
  fi
  state=clean
  [[ -z "$status" ]] || state=dirty
  fingerprint="$(
    {
      printf '%s\0' "$status"
      git diff --no-ext-diff --no-textconv --binary HEAD -- || exit 1
      git ls-files --others --exclude-standard -z | while IFS= read -r -d '' path; do
        printf '%s\0' "$path"
        if [[ -L "$path" ]]; then
          readlink "$path" || exit 1
        elif [[ -f "$path" ]]; then
          if [[ -x "$path" ]]; then printf 'executable\0'; else printf 'file\0'; fi
          shasum -a 256 "./$path" || exit 1
        else
          echo "cannot establish source provenance for $path" >&2; exit 1
        fi
      done
    } | shasum -a 256 | awk '{print $1}'
  )" || exit 1
  printf '%s\n%s\n%s\n' "$revision" "$state" "$fingerprint"
)

check_source_provenance() {
  local recorded="$1" current="$2" reuse="$3"
  if [[ "$recorded" != "$current" ]]; then
    echo 'source provenance mismatch; rebuild all requested targets from the current source' >&2
    return 1
  fi
  if [[ "$reuse" = 1 && "$(printf '%s\n' "$current" | sed -n '2p')" != clean ]]; then
    echo 'source provenance is dirty; --no-build requires a clean source tree and a clean build record' >&2
    return 1
  fi
}
