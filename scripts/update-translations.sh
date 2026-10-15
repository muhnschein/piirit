#!/bin/sh
# Regenerate catalogs from qsTr() in qml/: piirit.ts and each piirit-<lang>.ts.
# One lupdate run: new string shows unfinished everywhere.
#
# Arg: catalog dir (ci/ runs it on a copy). Default: tree's own.
#
# `-locations none`: locations change whenever a line moves, conflicting and
# failing ci/ check for nothing. Without them catalog changes only with strings.
#
# `-no-obsolete`: removed strings dropped. Never hand-maintained.
#
# New language: write translations/piirit-<lang>.ts as
#
#     <?xml version="1.0" encoding="utf-8"?>
#     <!DOCTYPE TS>
#     <TS version="2.1" language="<lang>"></TS>
#
# then run; lupdate fills every string with the language's numerus forms.
# <lang> is matched by QTranslator to locale: `de` serves all German, `pt_BR`
# Brazil only.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
dir=${1:-$root/translations}

lupdate=$(command -v lupdate || command -v lupdate-qt5 || true)
if [ -z "$lupdate" ]; then
    echo "update-translations: lupdate not found (install qttools5-dev-tools)" >&2
    exit 1
fi

# Source first, then languages by name: stable lupdate report order.
set -- "$dir/piirit.ts"
for catalog in "$dir"/piirit-*.ts; do
    [ -f "$catalog" ] || continue
    set -- "$@" "$catalog"
done

"$lupdate" -recursive "$root/qml" -ts "$@" -no-obsolete -locations none
