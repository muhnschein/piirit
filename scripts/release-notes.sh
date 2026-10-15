#!/bin/sh
# Prints one version's CHANGELOG.md section: the text of its GitHub release.
#
#     scripts/release-notes.sh <version>        # e.g. 1.0.0
#
# Section starts at "## <version>" (a date may follow), runs to next "## " or
# EOF. Heading left out: release page has the name. Missing or empty section
# fails: a tag pushed before the changelog stops the workflow, not an empty
# release.
set -eu

version=${1:-}
[ -n "$version" ] || { echo "usage: $0 <version>" >&2; exit 2; }

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
changelog="$root/CHANGELOG.md"

notes=$(awk -v version="$version" '
    # Version heading opens the section; any other ## heading closes it.
    # Regex from a literal: dots match only dots.
    BEGIN { pattern = "^## " version "([^0-9.]|$)"; gsub(/\./, "\\.", pattern) }
    /^## / { inside = ($0 ~ pattern); next }
    inside { print }
' "$changelog")

# Trailing blank lines off; no output at all is an error.
notes=$(printf '%s\n' "$notes" | sed -e :a -e '/^\n*$/{$d;N;ba' -e '}')
[ -n "$(printf '%s' "$notes" | tr -d '[:space:]')" ] || {
    echo "release-notes: CHANGELOG.md has no section for $version" >&2
    exit 1
}
printf '%s\n' "$notes"
