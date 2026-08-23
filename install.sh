#!/usr/bin/env bash
# Installs ghx system-wide (or per-user, if not run as root) on Linux/macOS.
#
# Layout:
#   $LOCAL/ghx/bin/ghx   the real binary
#   $LOCAL/bin/ghx       a symlink to it (on the standard PATH)
#   $LOCAL/lib/ghx/      reserved for future shared support files
#   $HOME/.ghx/logs/     per-user runtime logs (not part of the shared install)
#   $HOME/.ghx/cache/    per-user cache
#
# When run as root, $LOCAL defaults to /usr/local, $LOCAL/ghx is owned by a
# dedicated "ghx" group (setgid, group-writable), and the invoking user
# (the one who ran `sudo ./install.sh`, if any) is added to that group —
# so `ghx --update stable` works afterward without needing sudo again.
# Anyone else on the machine can still run ghx, just not overwrite it,
# unless an admin adds them to the group too (`sudo usermod -aG ghx <user>`
# on Linux, or the macOS equivalent below).
#
# When run as a normal user with no root available, everything installs
# under $HOME/.local instead, owned by that user — no group needed, since
# the user already owns everything they'd want to update.
#
# Env vars:
#   GHX_CHANNEL        stable (default) | beta | dev
#   GHX_LOCAL_BINARY   path to an already-built ghx binary to install
#                      instead of downloading a release (used by
#                      build-from-source.sh)

set -euo pipefail

REPO="Eclipser9-20/ghx"
CHANNEL="${GHX_CHANNEL:-stable}"
GROUP_NAME="ghx"

os="$(uname -s)"
arch="$(uname -m)"

case "$os" in
Linux) platform="linux" ;;
Darwin) platform="macos" ;;
*)
    echo "Unsupported OS: $os" >&2
    exit 1
    ;;
esac

case "$arch" in
x86_64 | amd64) asset_arch="x86_64" ;;
arm64 | aarch64) asset_arch="aarch64" ;;
*)
    echo "Unsupported architecture: $arch" >&2
    exit 1
    ;;
esac

ASSET="ghx-${platform}-${asset_arch}"

am_root=false
if [ "$(id -u)" -eq 0 ]; then am_root=true; fi

if $am_root; then
    LOCAL="/usr/local"
    INSTALL_USER="${SUDO_USER:-root}"
else
    LOCAL="$HOME/.local"
    INSTALL_USER="$(id -un)"
fi

GHX_HOME="$LOCAL/ghx"
BIN_DIR="$GHX_HOME/bin"
LIB_DIR="$LOCAL/lib/ghx"

echo "==> Installing ghx ($CHANNEL) to $GHX_HOME"

mkdir -p "$BIN_DIR" "$LIB_DIR" "$LOCAL/bin"

TARGET_BIN="$BIN_DIR/ghx"

if [ -n "${GHX_LOCAL_BINARY:-}" ]; then
    echo "==> Using local binary: $GHX_LOCAL_BINARY"
    cp "$GHX_LOCAL_BINARY" "$TARGET_BIN"
else
    api_url="https://api.github.com/repos/$REPO/releases"

    case "$CHANNEL" in
    stable) release_url="$api_url/latest" ;;
    dev) release_url="$api_url/tags/dev" ;;
    beta) release_url="" ;;
    *)
        echo "Unknown channel '$CHANNEL' (expected stable, beta, or dev)" >&2
        exit 1
        ;;
    esac

    if [ "$CHANNEL" = "beta" ]; then
        download_url=$(curl -fsSL "$api_url" |
            grep -A5 '"tag_name": ".*-beta\.' |
            grep "browser_download_url.*$ASSET" |
            head -1 |
            sed -E 's/.*"(https[^"]+)".*/\1/')
    else
        download_url=$(curl -fsSL "$release_url" |
            grep "browser_download_url.*$ASSET" |
            head -1 |
            sed -E 's/.*"(https[^"]+)".*/\1/')
    fi

    if [ -z "$download_url" ]; then
        echo "Could not find a release asset '$ASSET' on the $CHANNEL channel." >&2
        exit 1
    fi

    echo "==> Downloading $download_url"
    curl -fsSL -o "$TARGET_BIN" "$download_url"
fi

chmod +x "$TARGET_BIN"

ln -sf "../ghx/bin/ghx" "$LOCAL/bin/ghx"

if $am_root; then
    # Set up the shared-maintenance group.
    if [ "$platform" = "macos" ]; then
        if ! dscl . -read "/Groups/$GROUP_NAME" >/dev/null 2>&1; then
            next_gid=$(dscl . -list /Groups PrimaryGroupID | awk '{print $2}' | sort -n | tail -1)
            next_gid=$((next_gid + 1))
            dseditgroup -o create -i "$next_gid" "$GROUP_NAME"
        fi
        if [ "$INSTALL_USER" != "root" ]; then
            dseditgroup -o edit -a "$INSTALL_USER" -t user "$GROUP_NAME"
        fi
    else
        if ! getent group "$GROUP_NAME" >/dev/null 2>&1; then
            groupadd "$GROUP_NAME"
        fi
        if [ "$INSTALL_USER" != "root" ]; then
            usermod -aG "$GROUP_NAME" "$INSTALL_USER"
        fi
    fi

    chgrp -R "$GROUP_NAME" "$GHX_HOME"
    # setgid on directories so new files (e.g. from `ghx --update`) inherit
    # the group automatically; rwxrwsr-x on dirs, rwxrwxr-x on the binary.
    find "$GHX_HOME" -type d -exec chmod 2775 {} +
    find "$GHX_HOME" -type f -exec chmod 0775 {} +

    echo "==> $GHX_HOME is owned by the '$GROUP_NAME' group (setgid, group-writable)."
    if [ "$INSTALL_USER" != "root" ]; then
        echo "    $INSTALL_USER was added to it — log out/in (or run 'newgrp $GROUP_NAME')"
        echo "    for that to take effect in your current shell."
    fi
    echo "    To let another user run 'ghx --update' without sudo: sudo usermod -aG $GROUP_NAME <user>  (Linux)"
    echo "                                                          sudo dseditgroup -o edit -a <user> -t user $GROUP_NAME  (macOS)"
else
    chmod -R u+rwX,go+rX,go-w "$GHX_HOME"
fi

# Per-user state directory — not part of the shared install, no special
# permissions needed since each user already owns their own home directory.
mkdir -p "$HOME/.ghx/logs" "$HOME/.ghx/cache"

echo "==> Installed: $TARGET_BIN"
if ! echo ":$PATH:" | grep -q ":$LOCAL/bin:"; then
    echo "==> $LOCAL/bin is not on your PATH. Add this to your shell profile:"
    echo "        export PATH=\"$LOCAL/bin:\$PATH\""
fi
