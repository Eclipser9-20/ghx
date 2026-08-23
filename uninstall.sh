#!/usr/bin/env bash
# Removes a system-wide (or per-user) ghx install done by install.sh. This
# is the standalone counterpart to `ghx --uninstall` — use it when you
# can't (or don't want to) run the ghx binary itself to remove it, e.g. a
# broken install, remote/unattended cleanup, or removing it for another
# user.
#
# Removes:
#   $LOCAL/ghx/          the install directory
#   $LOCAL/bin/ghx        the PATH symlink
#   $HOME/.ghx/           per-user logs/cache for the CURRENT user only
#
# Leaves the "_GHXmaintenance" group in place, since removing a group can
# strand its membership on other machines/tools that reference it by
# name; delete it yourself (groupdel on Linux, dseditgroup -o delete on
# macOS) if you're sure nothing else depends on it.
#
# Run as root to remove a system-wide (/usr/local) install, or as a
# normal user to remove a per-user ($HOME/.local) install.

set -euo pipefail

am_root=false
if [ "$(id -u)" -eq 0 ]; then am_root=true; fi

if $am_root; then
    LOCAL="/usr/local"
else
    LOCAL="$HOME/.local"
fi

GHX_HOME="$LOCAL/ghx"

if [ -d "$GHX_HOME" ]; then
    rm -rf "$GHX_HOME"
    echo "==> Removed $GHX_HOME"
else
    echo "==> $GHX_HOME not found, nothing to remove there."
fi

if [ -L "$LOCAL/bin/ghx" ] || [ -e "$LOCAL/bin/ghx" ]; then
    rm -f "$LOCAL/bin/ghx"
    echo "==> Removed $LOCAL/bin/ghx"
fi

if [ -d "$HOME/.ghx" ]; then
    rm -rf "$HOME/.ghx"
    echo "==> Removed $HOME/.ghx"
fi

echo "==> ghx has been uninstalled."
