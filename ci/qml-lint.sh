#!/bin/sh
# Parse every .qml file, and every .js the package ships beside them.
#
# Syntax only: qmllint cannot resolve `Sailfish.Silica`. The Qt 5.6 dialect
# rules it cannot express live in rust/piiri-shim/tests/qml_syntax.rs.
#
# The .js files are not QML imports -- qml/webxdc/catch.js is a frame
# script for the browser engine -- but they are shipped, they are parsed
# by something on the phone, and a typo in one fails there and nowhere
# else. qmllint parses JavaScript too.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)

if ! command -v qmllint >/dev/null 2>&1; then
    echo "qml-lint: FAIL (qmllint not found; install qtdeclarative5-dev-tools)" >&2
    exit 1
fi

count=0
status=0
# Read rather than word-split: a path with a space in it would otherwise
# arrive as two arguments and neither would exist.
while IFS= read -r file; do
    [ -n "$file" ] || continue
    count=$((count + 1))
    if ! qmllint "$file"; then
        status=1
    fi
done <<EOF
$(find "$root/qml" \( -name '*.qml' -o -name '*.js' \) | sort)
EOF

if [ "$count" -eq 0 ]; then
    echo "qml-lint: FAIL (no .qml files found -- did the tree move?)" >&2
    exit 1
fi

[ "$status" -eq 0 ] && echo "qml-lint: ok ($count files)"
exit "$status"
