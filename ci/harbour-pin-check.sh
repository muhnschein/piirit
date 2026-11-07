#!/bin/bash
# Is ci/harbour/UPSTREAM's validator pin still the tip of the ref it came from?
#
# rpm.yml runs Jolla's validator at the pin; Harbour QA runs upstream's latest.
# Red = upstream moved (read its diff, rerun scripts/update-harbour-rules.sh),
# or pinned PR (`pull/N/head`) no longer open. scheduled.yml, weekly.
#
#     ci/harbour-pin-check.sh
#
# Needs network, and gh for a PR pin. Test seams: $HARBOUR_UPSTREAM,
# $HARBOUR_VALIDATOR_REPO, $HARBOUR_PR_STATE (skips gh).
set -u

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
upstream=${HARBOUR_UPSTREAM:-$root/ci/harbour/UPSTREAM}
repo=${HARBOUR_VALIDATOR_REPO:-https://github.com/sailfishos/sdk-harbour-rpmvalidator.git}
web=https://github.com/sailfishos/sdk-harbour-rpmvalidator

fail() {
    echo "harbour-pin: FAIL $*" >&2
    exit 1
}

# "at commit <sha> (<date>)" or, pinned to a ref, "(<date>, <ref>)".
line=$(grep -m1 '^at commit ' "$upstream" 2>/dev/null)
pin=$(sed -n 's/^at commit \([0-9a-f]\{40\}\) .*/\1/p' <<< "$line")
ref=$(sed -n 's/^at commit [0-9a-f]\{40\} ([^,)]*, \([^)]*\)).*/\1/p' <<< "$line")
ref=${ref:-master}
[[ -n "$pin" ]] || fail "$upstream names no commit"

case $ref in
    refs/*) full=$ref ;;
    pull/*) full=refs/$ref ;;
    *) full=refs/heads/$ref ;;
esac
tip=$(git ls-remote "$repo" "$full" 2>/dev/null | awk -v r="$full" '$2 == r { print $1 }')
[[ -n "$tip" ]] || fail "could not read $ref from $repo"

echo "harbour-pin: pinned ${pin:0:12}, $ref at ${tip:0:12}"
[[ "$pin" = "$tip" ]] ||
    fail "validator's $ref moved: $web/compare/$pin...$tip"

if [[ $ref =~ ^pull/([0-9]+)/ ]]; then
    pr=${BASH_REMATCH[1]}
    state=${HARBOUR_PR_STATE:-$(gh api "repos/sailfishos/sdk-harbour-rpmvalidator/pulls/$pr" \
        --jq 'if .merged then "merged" else .state end' 2>/dev/null)}
    [[ $state =~ ^(open|closed|merged)$ ]] || fail "could not read the state of $web/pull/$pr"
    echo "harbour-pin: $web/pull/$pr is $state"
    [[ "$state" = open ]] ||
        fail "pinned PR is $state: rerun scripts/update-harbour-rules.sh without argument (master)"
fi
echo "harbour-pin: ok"
