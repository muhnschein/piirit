#!/bin/sh
# Compile translations/piirit-<lang>.ts to .qm, or into dir given. Run by RPM
# %build, make translations, ci/packaging-lint.sh. Bad numerus or stray tag
# fails here, else language missing on phone.
#
# piirit.ts: source catalog for lupdate, not compiled.
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
for catalog in "$root"/translations/piirit-*.ts; do
    [ -f "$catalog" ] || continue
    name=$(basename "$catalog" .ts)
    # -silent: hides per-catalog counts (a warning would drown in forty).
    # Warnings still print.
    "$lrelease" -silent "$catalog" -qm "$out/$name.qm"
    count=$((count + 1))
done

if [ "$count" -eq 0 ]; then
    echo "release-translations: no translations/piirit-*.ts found" >&2
    exit 1
fi
echo "release-translations: $count catalog(s) compiled into $out"
