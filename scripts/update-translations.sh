#!/bin/sh
# Regenerate the catalogs from the qsTr() calls in qml/: piiri.ts, the
# untranslated source catalog, and every piiri-<lang>.ts beside it.
# One lupdate run over all of them, so a string added to the source shows
# up as unfinished in every language at once.
#
# The argument is the directory holding the catalogs, for ci/ to run this
# over a copy; the default is the tree's own.
#
# `-locations none`: with locations in, the catalog changes every time a
# line moves, so it would conflict on every unrelated edit and the check
# in ci/ would fail for no reason. Without them it changes only when the
# strings themselves do, which is the thing worth reviewing.
#
# `-no-obsolete`: a string that no longer exists in the source is dropped
# rather than kept as a tombstone. The catalog is regenerated from source,
# not maintained by hand.
#
# To add a language, write its header to translations/piiri-<lang>.ts:
#
#     <?xml version="1.0" encoding="utf-8"?>
#     <!DOCTYPE TS>
#     <TS version="2.1" language="<lang>"></TS>
#
# and run this. lupdate fills in every string, with as many numerus forms
# as the language has. <lang> is what QTranslator matches against the
# reader's locale: `de` serves every German locale, `pt_BR` only Brazil.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
dir=${1:-$root/translations}

lupdate=$(command -v lupdate || command -v lupdate-qt5 || true)
if [ -z "$lupdate" ]; then
    echo "update-translations: lupdate not found (install qttools5-dev-tools)" >&2
    exit 1
fi

# The source catalog first, then the languages in name order, so the
# order lupdate reports them in is the same every run.
set -- "$dir/piiri.ts"
for catalog in "$dir"/piiri-*.ts; do
    [ -f "$catalog" ] || continue
    set -- "$@" "$catalog"
done

"$lupdate" -recursive "$root/qml" -ts "$@" -no-obsolete -locations none
