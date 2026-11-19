#!/usr/bin/env bash
# Sign the release checksum list for Linux installs, and copy the public key next to it.
#
# Usage:
#   packaging/linux/sign-checksums.sh <dist-folder>
#
# Reads the GitHub secrets LINUX_GPG_PRIVATE_KEY (ASCII-armoured private key) and
# LINUX_GPG_PASSPHRASE from the environment. Writes SHA256SUMS.txt.asc (a detached signature)
# and notecraft-linux-signing.public.asc into the folder. With no key set, it prints a warning
# and exits 0, the same as packaging/windows/sign.ps1: the release is then published unsigned.
set -euo pipefail

DIST="${1:?usage: sign-checksums.sh <dist-folder>}"
[[ -f "$DIST/SHA256SUMS.txt" ]] || { echo "sign-checksums: $DIST/SHA256SUMS.txt is missing" >&2; exit 1; }

if [[ -z "${LINUX_GPG_PRIVATE_KEY:-}" ]]; then
    echo "::warning::LINUX_GPG_PRIVATE_KEY is not set; the Linux checksum list is not signed"
    exit 0
fi
: "${LINUX_GPG_PASSPHRASE:?LINUX_GPG_PASSPHRASE must be set with the signing key}"

GNUPG_HOME="$(mktemp -d)"
trap 'rm -rf "$GNUPG_HOME"' EXIT
chmod 700 "$GNUPG_HOME"
export GNUPGHOME="$GNUPG_HOME"

printf '%s\n' "$LINUX_GPG_PRIVATE_KEY" | gpg --batch --quiet --import
FPR="$(gpg --batch --list-secret-keys --with-colons | awk -F: '/^fpr/{print $10; exit}')"
[[ -n "$FPR" ]] || { echo "sign-checksums: the key did not import" >&2; exit 1; }

printf '%s' "$LINUX_GPG_PASSPHRASE" | gpg --batch --yes --quiet --pinentry-mode loopback \
    --passphrase-fd 0 --local-user "$FPR" --armor --detach-sign \
    --output "$DIST/SHA256SUMS.txt.asc" "$DIST/SHA256SUMS.txt"

gpg --batch --quiet --armor --export "$FPR" > "$DIST/notecraft-linux-signing.public.asc"

# Check the signature we just wrote, against the public key we just exported.
VERIFY_HOME="$(mktemp -d)"
trap 'rm -rf "$GNUPG_HOME" "$VERIFY_HOME"' EXIT
GNUPGHOME="$VERIFY_HOME" gpg --batch --quiet --import "$DIST/notecraft-linux-signing.public.asc"
GNUPGHOME="$VERIFY_HOME" gpg --batch --quiet --verify "$DIST/SHA256SUMS.txt.asc" "$DIST/SHA256SUMS.txt"
echo "sign-checksums: signed SHA256SUMS.txt with key $FPR"
