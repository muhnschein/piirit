#!/bin/sh
# Print one version's section of CHANGELOG.md, which is what the GitHub
# release for that version says.
#
#     scripts/release-notes.sh <version>        # e.g. 1.0.0
#
# A section starts at "## <version>" -- the heading may carry a date after
# the version -- and runs to the next "## " heading or the end of the file.
# The heading itself is left out: the release page already has the name.
# No such section, or an empty one, is a failure, so a tag pushed before
# the changelog was written stops in the workflow rather than publishing a
# release with nothing in it.
set -eu

version=${1:-}
[ -n "$version" ] || { echo "usage: $0 <version>" >&2; exit 2; }

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
changelog="$root/CHANGELOG.md"

notes=$(awk -v version="$version" '
    # A heading for this version opens the section; any other level-two
    # heading closes it. The regex is built from a literal, so a version
    # with dots matches those dots and not any character.
    BEGIN { pattern = "^## " version "([^0-9.]|$)"; gsub(/\./, "\\.", pattern) }
    /^## / { inside = ($0 ~ pattern); next }
    inside { print }
' "$changelog")

# Trailing blank lines off, and nothing at all is an error.
notes=$(printf '%s\n' "$notes" | sed -e :a -e '/^\n*$/{$d;N;ba' -e '}')
[ -n "$(printf '%s' "$notes" | tr -d '[:space:]')" ] || {
    echo "release-notes: CHANGELOG.md has no section for $version" >&2
    exit 1
}
printf '%s\n' "$notes"
