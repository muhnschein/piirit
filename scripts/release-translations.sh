#!/bin/sh
# Compile every translations/piiri-<lang>.ts into a .qm beside it, or
# into the directory given, with lrelease. What the RPM's %build runs, what
# `make translations` runs, and what ci/packaging-lint.sh runs to prove
# the catalogs compile at all -- a numerus form too few, or a stray tag,
# is an error here and a missing language on a phone.
#
# piiri.ts itself is the untranslated source catalog, kept for lupdate
# to read the strings from; it is not compiled.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
out=${1:-$root/translations}

lrelease=$(command -v lrelease || command -v lrelease-qt5 || true)
if [ -z "$lrelease" ]; then
    echo "release-translations: lrelease not found (install qttools5-dev-tools)" >&2
    exit 1
fi

mkdir -p "$out"
count=0
for catalog in "$root"/translations/piiri-*.ts; do
    [ -f "$catalog" ] || continue
    name=$(basename "$catalog" .ts)
    # -silent: lrelease otherwise reports every catalog's counts, and a
    # warning would be lost in forty of them. It still prints warnings.
    "$lrelease" -silent "$catalog" -qm "$out/$name.qm"
    count=$((count + 1))
done

if [ "$count" -eq 0 ]; then
    echo "release-translations: no translations/piiri-*.ts found" >&2
    exit 1
fi
echo "release-translations: $count catalog(s) compiled into $out"
