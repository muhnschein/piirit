#!/bin/sh
# Static checks on what decides RPM install and launcher. BuildRequires need the
# SDK (docs/BUILDING.md).
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
status=0
ran=0

# PACKAGING_LINT_STRICT=1: missing tool fails. CI sets it.
strict=${PACKAGING_LINT_STRICT:-0}
skip() {
    # POSIX sh: no `local`; these names are unique in file.
    tool=$1
    hint=$2
    if [ "$strict" = 1 ]; then
        echo "packaging-lint: FAIL $tool is not installed ($hint) (strict mode)" >&2
        status=1
    else
        echo "packaging-lint: SKIP $tool ($hint)"
    fi
    return 0
}

if command -v rpmspec >/dev/null 2>&1; then
    ran=$((ran + 1))
    # -P expands and parses only; BuildRequires are SDK's.
    if rpmspec -P "$root/rpm/harbour-piirit.spec" >/dev/null; then
        echo "packaging-lint: rpm/harbour-piirit.spec parses"
    else
        echo "packaging-lint: FAIL rpm/harbour-piirit.spec does not parse" >&2
        status=1
    fi
else
    skip rpmspec "install rpm"
fi

# SDK's rpm expands macros in comments (newer host rpm doesn't): `%build`
# becomes the build preamble, starting `LANG=C`, read as a tag ("Unknown tag").
# Host rpmspec parses it fine; only a direct check catches it.
ran=$((ran + 1))
bare=$(awk '/^[[:space:]]*#/ {
        stripped = $0
        gsub(/%%/, "", stripped)
        if (stripped ~ /%/) printf "%s:%d: %s\n", FILENAME, FNR, $0
    }' "$root/rpm/harbour-piirit.spec")
if [ -z "$bare" ]; then
    echo "packaging-lint: spec comments escape their macros"
else
    echo "$bare" >&2
    echo "packaging-lint: FAIL a spec comment has an unescaped % (write %%)" >&2
    status=1
fi

if command -v desktop-file-validate >/dev/null 2>&1; then
    ran=$((ran + 1))
    # Sailfish keys are not freedesktop: each expected warning named;
    # anything else fails.
    out=$(desktop-file-validate "$root/harbour-piirit.desktop" 2>&1 |
        grep -v 'value "silica-qt5" for key "X-Nemo-Application-Type"' |
        grep -v 'key "X-Nemo-Application-Type" .* is not known' || true)
    if [ -z "$out" ]; then
        echo "packaging-lint: harbour-piirit.desktop valid"
    else
        echo "$out" >&2
        echo "packaging-lint: FAIL harbour-piirit.desktop" >&2
        status=1
    fi
else
    skip desktop-file-validate "install desktop-file-utils"
fi

# Build must be distinguishable: spec pins Version/Release, mb2 -X means no git.
# Without a stamp every build is harbour-piirit-0.1.0-1, and `rpm -U` refuses
# it.
ran=$((ran + 1))
if grep -q '^Release:' "$root/rpm/harbour-piirit.spec" &&
    ! grep -q 'sed -i "s/\^Release:' "$root/.github/workflows/rpm.yml"; then
    echo "packaging-lint: FAIL the rpm workflow no longer stamps Release, so" \
         "every build would be the same NEVRA and refuse to install over" \
         "the last" >&2
    status=1
else
    echo "packaging-lint: the rpm workflow stamps a unique Release"
fi

# nextest skips doctests silently, so `cargo test --doc` runs beside it.
# Comments are stripped first: a Makefile comment saying "--doc" would satisfy
# the check.
ran=$((ran + 1))
doc_missing=""
for file in .github/workflows/ci.yml Makefile; do
    commands=$(grep -v '^[[:space:]]*#' "$root/$file")
    if printf '%s\n' "$commands" | grep -q 'nextest run' &&
        ! printf '%s\n' "$commands" | grep -q 'test .*--doc'; then
        doc_missing="$doc_missing $file"
    fi
done
if [ -n "$doc_missing" ]; then
    echo "packaging-lint: FAIL nextest runs the suite in$doc_missing without" \
         "\`cargo test --doc\` beside it, so doctests would silently stop" \
         "running" >&2
    status=1
else
    echo "packaging-lint: doctests run wherever nextest does"
fi

# mb2 takes package name from dir (rpm/<that>.spec). build-in-docker.sh mounts
# checkout at a path it chooses: that name and spec's must agree, or build
# stops before rpmbuild.
ran=$((ran + 1))
spec_base=$(basename "$(find "$root/rpm" -maxdepth 1 -name '*.spec' | sort | head -1)" .spec)
# shellcheck disable=SC2016 # $home is the script's text, not ours.
builddir=$(sed -n 's|^builddir="\$home/\([^"]*\)".*|\1|p' \
    "$root/scripts/build-in-docker.sh" | head -1)
if [ "$spec_base" = "$builddir" ]; then
    echo "packaging-lint: the docker build runs in a directory named for the spec"
else
    echo "packaging-lint: FAIL scripts/build-in-docker.sh mounts the checkout as" \
         "'$builddir' but the spec is rpm/$spec_base.spec; mb2 would not find it" >&2
    status=1
fi

# Catalogs regenerated and compared, not eyeballed: a missing qsTr() entry is
# invisible until someone translates.
if command -v lupdate >/dev/null 2>&1 || command -v lupdate-qt5 >/dev/null 2>&1; then
    ran=$((ran + 1))
    # .ts only: `make translations` leaves .qm beside them, not lupdate's.
    fresh_dir=$(mktemp -d)
    cp "$root"/translations/*.ts "$fresh_dir/"
    stale=""
    if "$root/scripts/update-translations.sh" "$fresh_dir" >/dev/null 2>&1; then
        for catalog in "$root"/translations/*.ts; do
            name=$(basename "$catalog")
            diff -q "$catalog" "$fresh_dir/$name" >/dev/null || stale="$stale $name"
        done
    else
        stale=" (lupdate failed)"
    fi
    if [ -z "$stale" ]; then
        echo "packaging-lint: every translations/*.ts is up to date"
    else
        echo "packaging-lint: FAIL stale catalog(s):$stale; run scripts/update-translations.sh" >&2
        status=1
    fi
    rm -rf "$fresh_dir"
else
    skip lupdate "install qttools5-dev-tools"
fi

# Must compile: lrelease runs on SDK minutes into build. Every warning is an
# untranslated string on the phone.
if command -v lrelease >/dev/null 2>&1 || command -v lrelease-qt5 >/dev/null 2>&1; then
    ran=$((ran + 1))
    qm_dir=$(mktemp -d)
    if out=$("$root/scripts/release-translations.sh" "$qm_dir" 2>&1) &&
        ! echo "$out" | grep -qi 'warning'; then
        echo "packaging-lint: every translations/piirit-*.ts compiles cleanly"
    else
        echo "$out" >&2
        echo "packaging-lint: FAIL a catalog does not compile cleanly with lrelease" >&2
        status=1
    fi
    rm -rf "$qm_dir"
else
    skip lrelease "install qttools5-dev-tools"
fi

# Every docs/<name>.md a comment or doc points at must exist.
ran=$((ran + 1))
missing=$(grep -rhoE 'docs/[A-Za-z0-9_-]+\.md' "$root" \
        --include='*.rs' --include='*.qml' --include='*.js' --include='*.sh' \
        --include='*.yml' --include='*.toml' --include='*.md' --include='*.spec' \
        --include='.gitignore' --include='Makefile' --include='*.conf' \
        --exclude-dir=.git --exclude-dir=target --exclude-dir=vendor \
        --exclude-dir=third_party |
    sort -u | while read -r ref; do
        [ -f "$root/$ref" ] || echo "  $ref"
    done)
if [ -z "$missing" ]; then
    echo "packaging-lint: every docs/*.md referenced exists"
else
    echo "$missing" >&2
    echo "packaging-lint: FAIL these documents are referenced but do not exist; repoint or restore them" >&2
    status=1
fi

if command -v shellcheck >/dev/null 2>&1; then
    ran=$((ran + 1))
    if shellcheck "$root"/ci/*.sh "$root"/scripts/*.sh; then
        echo "packaging-lint: shell scripts clean"
    else
        echo "packaging-lint: FAIL shellcheck" >&2
        status=1
    fi
else
    skip shellcheck "install shellcheck"
fi

if [ "$ran" -eq 0 ]; then
    echo "packaging-lint: FAIL (no checker was available; this job proves nothing)" >&2
    exit 1
fi

exit "$status"
