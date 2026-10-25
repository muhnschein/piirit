#!/usr/bin/env bash
# Prove ci/changes.sh skips Rust jobs for docs alone, never for anything else.
#
# Costly mistake: a code change read as docs-only, CI green without running a
# test. Most cases guard that side.
set -u

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
script="$root/ci/changes.sh"

if [[ ! -x "$script" ]]; then
    echo "selftest: FAIL $script is missing or not executable" >&2
    exit 1
fi

status=0
cases=0

# says <want> <why> <file names, one per line>
says() {
    local want=$1 why=$2 files=$3
    cases=$((cases + 1))
    local got
    got=$("$script" --classify <<<"$files")
    if [[ "$got" = "code=$want" ]]; then
        echo "selftest: ok   $why"
    else
        echo "selftest: FAIL $why -- got $got, wanted code=$want" >&2
        status=1
    fi
    return 0
}

says false "docs alone skip the Rust jobs" $'docs/BUILDING.md\nREADME.md'
says false "changelog alone skips them" "CHANGELOG.md"
says true "a source file runs them" "rust/piirit-shim/src/chat.rs"
says true "a page runs them" $'docs/BUILDING.md\nqml/pages/GalleryPage.qml'
says true "code after docs still runs them" $'README.md\nMakefile'
says true "a vendored notice runs them (real_server reads it)" \
    "vendor/deltachat-rpc-server/SOURCE.md"
says true "third_party docs run them (vendor-check reads them)" \
    "third_party/qmetaobject/README.md"
says true "a file only named like docs runs them" "CHANGELOG.md.orig"
says true "an empty list runs them" ""

# Whole script on own repo: last commit touches docs alone, so only the event
# can make it run everything.
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
git_() { git -C "$work" -c user.name=t -c user.email=t@t "$@" >/dev/null 2>&1; }
git_ init -q
echo a >"$work/main.rs"
git_ add . && git_ commit -qm code

# event <want> <why> <EVENT>: script run in $work.
event() {
    local want=$1 why=$2 got
    cases=$((cases + 1))
    got=$(cd "$work" && EVENT=$3 "$script" 2>/dev/null)
    if [[ "$got" = "code=$want" ]]; then
        echo "selftest: ok   $why"
    else
        echo "selftest: FAIL $why -- got $got, wanted code=$want" >&2
        status=1
    fi
    return 0
}

event true "a first commit (no base to diff) runs everything" pull_request
echo a >"$work/README.md"
git_ add . && git_ commit -qm docs
event false "a PR touching docs alone skips them" pull_request
event true "a push to main runs everything" push
event true "a manual run runs everything" workflow_dispatch
echo b >"$work/main.rs"
git_ add . && git_ commit -qm code
event true "a PR touching code runs them" pull_request

echo "selftest: $cases case(s)"
if [[ "$status" -ne 0 ]]; then
    echo "selftest: FAILED" >&2
fi
exit "$status"
