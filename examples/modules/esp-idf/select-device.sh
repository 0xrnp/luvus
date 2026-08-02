#!/bin/sh
# A DEVICES row was clicked: make that port the active one.
#
# The port is stored as a module *setting* rather than in the state dir, so it
# also shows up (and stays editable) in Settings -> Modules.
set -eu
. "$(dirname "$0")/lib.sh"

chosen="${BOHAY_MODULE_ROW_VALUE:-}"
[ -n "$chosen" ] || { toast "no port on that row"; exit 1; }

"$bohay" module settings example.esp-idf port "$chosen" >/dev/null
toast "port: $chosen"
BOHAY_SETTING_PORT="$chosen" sh "$(dirname "$0")/dock.sh"   # repaint the selection dot
