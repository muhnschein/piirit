#!/bin/bash
# Runs Jolla's validator on a built RPM; judges result against
# ci/harbour/waivers.conf.
#
# Authority that harbour-check.sh stands in for: only built package shows
# generated Requires/Provides, stripped symbols, real modes. Needs an RPM, so
# runs in rpm.yml, not per PR.
#
# Known blockers (waivers.conf, also reported by source check) don't fail it:
# a workflow red for unfixed reasons stops being read. Anything not waived
# fails.
#
#     ci/harbour-validate-rpm.sh <rpm>
#     ci/harbour-validate-rpm.sh --log <saved validation log>
#
# Validator: upstream's clone, not vendored. $HARBOUR_VALIDATOR points at it.
set -u
shopt -s extglob

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
waivers="$root/ci/harbour/waivers.conf"
validator=${HARBOUR_VALIDATOR:-/tmp/harbour-validator}

usage() {
    echo "usage: $0 <rpm> | $0 --log <validation log>" >&2
    exit 2
}

# Run command with SIGPIPE default.
#
# GitHub's Node parent ignores SIGPIPE; ignore survives exec, so dead-pipe
# writes return EPIPE (4945 "Broken pipe" lines once). bash can't undo it (`trap
# - PIPE` doesn't), so reset must happen in a process that execs the validator.
sigpipe_default() {
    if command -v python3 >/dev/null 2>&1; then
        python3 -c 'import os, signal, sys
signal.signal(signal.SIGPIPE, signal.SIG_DFL)
os.execvp(sys.argv[1], sys.argv[1:])' "$@"
    elif command -v perl >/dev/null 2>&1; then
        perl -e '$SIG{PIPE} = "DEFAULT"; exec { $ARGV[0] } @ARGV or die "$!\n"' "$@"
    else
        echo "harbour-rpm: note: no python3 or perl to reset SIGPIPE with;" \
             "the validator's log may carry broken-pipe noise" >&2
        "$@"
    fi
    # Exit status passed through; caller judges.
    return
}

log=""
rpm=""
case "${1:-}" in
    --log) [[ $# -eq 2 ]] || usage; log=$2 ;;
    "" | -*) usage ;;
    *) [[ $# -eq 1 ]] || usage; rpm=$1 ;;
esac

if [[ -n "$rpm" ]]; then
    [[ -f "$rpm" ]] || { echo "harbour-rpm: FAIL no such RPM: $rpm" >&2; exit 1; }
    if [[ ! -x "$validator/rpmvalidation.sh" ]]; then
        # Commit from ci/harbour/UPSTREAM: same Harbour as vendored rules, and
        # CI never runs repo HEAD. scripts/update-harbour-rules.sh moves both.
        commit=$(sed -n 's/^at commit \([0-9a-f]\{40\}\).*/\1/p' \
            "$root/ci/harbour/UPSTREAM" | head -1)
        if [[ -z "$commit" ]]; then
            echo "harbour-rpm: FAIL ci/harbour/UPSTREAM names no commit to pin the validator to" >&2
            exit 1
        fi
        if ! { git init -q "$validator" &&
               git -C "$validator" fetch -q --depth 1 \
                   https://github.com/sailfishos/sdk-harbour-rpmvalidator.git "$commit" &&
               git -C "$validator" checkout -q FETCH_HEAD; } >&2; then
            echo "harbour-rpm: FAIL could not fetch the validator at $commit" >&2
            exit 1
        fi
    fi

    # Vendored rules must equal validator's own; difference = hand-edited file.
    for conf in "$root"/ci/harbour/*.conf; do
        name=$(basename "$conf")
        [[ "$name" = waivers.conf ]] && continue
        [[ -f "$validator/$name" ]] || continue
        if ! diff -q "$conf" "$validator/$name" >/dev/null; then
            echo "harbour-rpm: FAIL ci/harbour/$name is not the validator's own;" \
                 "run scripts/update-harbour-rules.sh" >&2
            exit 1
        fi
    done

    log=$(mktemp)
    trap 'rm -f "$log"' EXIT
    # BATCHERBATCHERBATCHER: `KIND|subject|message`, no colour. Exit status
    # unused: markers decide.
    BATCHERBATCHERBATCHER=1 sigpipe_default "$validator/rpmvalidation.sh" \
        -g "$validator" "$rpm" > "$log" 2>&1 || true
fi

[[ -f "$log" ]] || { echo "harbour-rpm: FAIL no validation log: $log" >&2; exit 1; }

# Strip leftover broken-pipe noise, but count it: broken reset must say so.
noise=$(grep -c 'write error: Broken pipe' "$log" || true)
if [[ "$noise" -gt 0 ]]; then
    echo "harbour-rpm: note: hid $noise broken-pipe line(s) from the" \
         "validator; SIGPIPE was ignored by whatever started this"
fi
grep -v 'write error: Broken pipe' "$log" || true

if ! grep -q '^!END!' "$log"; then
    echo "harbour-rpm: FAIL the validator produced no verdict" >&2
    exit 1
fi

# Waived = subject glob AND message glob both match (bash globs, as upstream).
# Subject alone waived every error about the bundled server (setuid, RPATH...),
# not just the known ones.
waived_line() {
    local subject=$1 message=$2 entry wid wsubject wmessage
    [[ -f "$waivers" ]] || return 1
    while IFS= read -r entry; do
        entry=${entry%%#*}
        read -r wid wsubject wmessage <<< "$entry"
        { [[ -n "${wid:-}" ]] && [[ -n "${wsubject:-}" ]] && [[ -n "${wmessage:-}" ]]; } || continue
        # shellcheck disable=SC2053 # unquoted on purpose: they are globs.
        [[ $subject == $wsubject ]] && [[ $message == $wmessage ]] && return 0
    done < "$waivers"
    return 1
}

errors=0
waived=0
while IFS= read -r line; do
    subject=$(cut -d'|' -f2 <<< "$line")
    message=$(cut -d'|' -f3- <<< "$line")
    if waived_line "$subject" "$message"; then
        echo "harbour-rpm: WAIVED $subject -- $message"
        waived=$((waived + 1))
    else
        echo "harbour-rpm: FAIL $subject -- $message" >&2
        errors=$((errors + 1))
    fi
done < <(grep '^ERROR|' "$log" || true)

while IFS= read -r line; do
    echo "harbour-rpm: warning $(cut -d'|' -f2 <<< "$line") --" \
         "$(cut -d'|' -f3- <<< "$line")"
done < <(grep '^WARNING|' "$log" || true)

echo
if [[ "$errors" -gt 0 ]]; then
    echo "harbour-rpm: FAILED -- $errors finding(s) Harbour would reject" >&2
    echo "harbour-rpm: $waived other finding(s) are waived in ci/harbour/waivers.conf" >&2
    exit 1
fi

if [[ "$waived" -gt 0 ]]; then
    echo "harbour-rpm: ok -- nothing new; $waived waived finding(s) still block" \
         "submission (docs/HARBOUR.md)"
else
    echo "harbour-rpm: ok -- the validator accepts this package"
fi
