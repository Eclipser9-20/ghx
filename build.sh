#!/usr/bin/env bash
# Builds ghx in release mode on macOS/Linux and signs it if configured.
# Signing is fully optional — without the relevant env vars this just
# builds and tells you signing was skipped.
#
# macOS codesigning:
#   GHX_SIGN_IDENTITY   "Developer ID Application: Name (TEAMID)" (from `security find-identity -v -p codesigning`)
#   GHX_NOTARY_PROFILE  A keychain profile name set up via:
#                         xcrun notarytool store-credentials <profile> \
#                           --apple-id you@example.com --team-id TEAMID --password <app-specific-password>
#                       If set, the signed binary is submitted for notarization too.
#
# Linux signing (no OS-level codesign standard exists, so this produces a
# detached GPG signature instead, which is the closest equivalent):
#   GHX_SIGN_GPG_KEY    GPG key id/fingerprint to sign with

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

echo "==> cargo build --release"
cargo build --release

BIN="target/release/ghx"
if [ ! -f "$BIN" ]; then
    echo "Build succeeded but $BIN was not found." >&2
    exit 1
fi

OS="$(uname -s)"

case "$OS" in
Darwin)
    if [ -z "${GHX_SIGN_IDENTITY:-}" ]; then
        echo "==> Skipping codesign (GHX_SIGN_IDENTITY not set). Binary: $BIN"
        exit 0
    fi

    echo "==> codesign: $BIN"
    codesign --force --options runtime --timestamp \
        --sign "$GHX_SIGN_IDENTITY" "$BIN"

    echo "==> Verifying signature"
    codesign --verify --verbose=2 "$BIN"

    if [ -n "${GHX_NOTARY_PROFILE:-}" ]; then
        echo "==> Notarizing (this can take a few minutes)"
        ZIP="target/release/ghx-notarize.zip"
        ditto -c -k --keepParent "$BIN" "$ZIP"
        xcrun notarytool submit "$ZIP" --keychain-profile "$GHX_NOTARY_PROFILE" --wait
        rm -f "$ZIP"
        echo "NOTE: stapling only applies to .app/.pkg/.dmg bundles, not a bare"
        echo "CLI binary — Gatekeeper will verify the notarization ticket online"
        echo "instead, which is normal for a plain executable."
    fi

    echo "==> Done: $BIN (signed)"
    ;;

Linux)
    if [ -z "${GHX_SIGN_GPG_KEY:-}" ]; then
        echo "==> Skipping signature (GHX_SIGN_GPG_KEY not set). Binary: $BIN"
        exit 0
    fi

    echo "==> gpg --detach-sign: $BIN"
    gpg --local-user "$GHX_SIGN_GPG_KEY" --armor --detach-sign --output "$BIN.asc" "$BIN"

    echo "==> Verifying signature"
    gpg --verify "$BIN.asc" "$BIN"

    echo "==> Done: $BIN (signed, detached signature at $BIN.asc)"
    ;;

*)
    echo "Unrecognized OS '$OS' — built but not signed. Binary: $BIN"
    ;;
esac
