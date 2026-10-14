#!/bin/sh
# Parse every .qml and .js shipped beside them.
#
# Syntax only: qmllint can't resolve `Sailfish.Silica`. Qt 5.6 rules:
# rust/piirit-shim/tests/qml_syntax.rs.
#
# .js aren't all QML imports (qml/webxdc/catch.js: browser frame script), but
# all are parsed on phone; typo fails only there. qmllint parses JS too.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)

if ! command -v qmllint >/dev/null 2>&1; then
    echo "qml-lint: FAIL (qmllint not found; install qtdeclarative5-dev-tools)" >&2
    exit 1
fi

count=0
status=0
# Read, not word-split: spaces in path.
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
