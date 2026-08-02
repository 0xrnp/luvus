#!/bin/sh
# Shared helpers. Sourced by every script in this module; never run directly.
#
# Everything arrives in the environment, so nothing here parses JSON:
#   BOHAY_BIN_PATH         the running server's own binary. Use this, never a
#                          bare `bohay` on PATH, or a second install would talk
#                          to a different socket and report "no module".
#   BOHAY_MODULE_STATE_DIR a writable dir for this module's own bookkeeping
#   BOHAY_PANE_ID          the pane the action was invoked from (if any)
#   BOHAY_WORKSPACE_CWD    the folder of the node it was invoked against
#   BOHAY_SETTING_*        the declared settings

bohay="${BOHAY_BIN_PATH:-bohay}"
proj="${BOHAY_WORKSPACE_CWD:-$PWD}"
state="${BOHAY_MODULE_STATE_DIR:-/tmp}"

idf_path=$(printf '%s' "${BOHAY_SETTING_IDF_PATH:-$HOME/esp/esp-idf}" | sed "s|^~|$HOME|")
target="${BOHAY_SETTING_TARGET:-esp32s3}"
port="${BOHAY_SETTING_PORT:-}"
baud="${BOHAY_SETTING_BAUD:-460800}"
flash_method="${BOHAY_SETTING_FLASH_METHOD:-uart}"
auto_monitor="${BOHAY_SETTING_AUTO_MONITOR:-true}"

toast() { "$bohay" ui toast "$1"; }

# The chip this project is *actually* configured for, read from the sdkconfig
# `idf.py` generates. Empty when the project has never been configured.
#
# `$target` above is only the module setting — what a future `set-target` would
# apply. Showing that as if it were the project's chip is a guess presented as
# fact: the default is an arbitrary `esp32s3`, so an `esp32` project would have
# displayed the wrong chip with nothing to hint at it.
project_target() {
  [ -f "$proj/sdkconfig" ] || return 0
  sed -n 's/^CONFIG_IDF_TARGET="\(.*\)"$/\1/p' "$proj/sdkconfig" | head -1
}

# Fail early and visibly rather than typing a broken command into a pane.
require_idf() {
  if [ ! -f "$idf_path/export.sh" ]; then
    toast "ESP-IDF not found at $idf_path — set IDF_PATH in Settings -> Modules"
    exit 1
  fi
}

# The one place that knows how to enter the IDF environment. `-p`/`-b` are
# omitted when unset so idf.py can fall back to its own auto-detection.
idf_cmd() {
  _args=""
  [ -n "$port" ] && _args="$_args -p $port"
  [ -n "$baud" ] && _args="$_args -b $baud"
  printf 'cd %s && . %s/export.sh >/dev/null && idf.py%s %s' \
    "$(quote "$proj")" "$(quote "$idf_path")" "$_args" "$*"
}

quote() { printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"; }

# Where should a command run? The pane the user right-clicked, if there is one;
# otherwise split a fresh pane so a build never takes over an agent's pane.
target_pane() {
  if [ -n "${BOHAY_PANE_ID:-}" ]; then
    printf '%s' "$BOHAY_PANE_ID"
  else
    "$bohay" pane split 2>/dev/null | sed -n 's/.*"pane": *"\([0-9]*\)".*/\1/p' | head -1
  fi
}
