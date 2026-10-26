#!/usr/bin/env bash
# Prove ci/apt-install.sh drops the right apt lists, keeps the rest.
#
# Catastrophic failure: delete Ubuntu archive with third-party repos, every job
# installs nothing. Runner images put archive in /etc/apt/sources.list or a
# .sources file under sources.list.d: cases cover both.
set -u

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
script="$root/ci/apt-install.sh"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

if [[ ! -x "$script" ]]; then
    echo "selftest: FAIL $script is missing or not executable" >&2
    exit 1
fi

status=0
cases=0

stage() {
    rm -rf "$work/sources"
    mkdir -p "$work/sources"

    # GitHub image's extras beside Ubuntu's, one-line format.
    cat >"$work/sources/google-chrome.list" <<'EOF'
deb [arch=amd64] https://dl.google.com/linux/chrome/deb/ stable main
EOF
    cat >"$work/sources/microsoft-prod.list" <<'EOF'
deb [arch=amd64] https://packages.microsoft.com/ubuntu/22.04/prod jammy main
EOF
    # Ubuntu archive, deb822 (current image). Dropping it is the mistake that
    # matters.
    cat >"$work/sources/ubuntu.sources" <<'EOF'
Types: deb
URIs: http://azure.archive.ubuntu.com/ubuntu/
Suites: noble noble-updates noble-backports
Components: main universe restricted multiverse
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg
EOF
    # One-line format (older image).
    cat >"$work/sources/archive.list" <<'EOF'
deb http://archive.ubuntu.com/ubuntu jammy main
EOF
    # Not a source list. apt ignores anything without the two extensions; so
    # must this.
    cat >"$work/sources/google-chrome.list.save" <<'EOF'
deb [arch=amd64] https://dl.google.com/linux/chrome/deb/ stable main
EOF
    return 0
}

# survives <file> <yes|no> <why>
survives() {
    local name=$1 want=$2 why=$3
    cases=$((cases + 1))
    local there=no
    if [[ -f "$work/sources/$name" ]]; then
        there=yes
    fi
    if [[ "$there" = "$want" ]]; then
        echo "selftest: ok   $why"
    else
        echo "selftest: FAIL $why -- $name present=$there, wanted $want" >&2
        status=1
    fi
    return 0
}

stage
out=$(APT_SOURCES_DIR="$work/sources" "$script" --prune-only 2>&1)
rc=$?

cases=$((cases + 1))
if [[ "$rc" -eq 0 ]]; then
    echo "selftest: ok   pruning succeeds"
else
    echo "selftest: FAIL pruning should exit 0, got $rc" >&2
    echo "$out" >&2
    status=1
fi

survives google-chrome.list no "a third-party list is dropped"
survives microsoft-prod.list no "every third-party list is dropped, not just the first"
survives ubuntu.sources yes "the archive in deb822 form is kept"
survives archive.list yes "the archive in one-line form is kept"
survives google-chrome.list.save yes "a file apt would not read is left alone"

# Log is what reader sees when job installs wrong thing: name each file acted
# on.
cases=$((cases + 1))
if grep -q "dropping google-chrome.list" <<<"$out" &&
    grep -q "keeping ubuntu.sources" <<<"$out"; then
    echo "selftest: ok   the log names what was dropped and what was kept"
else
    echo "selftest: FAIL the log should name each list it acted on" >&2
    echo "$out" >&2
    status=1
fi

# Empty and missing dir both fine (image keeps sources in
# /etc/apt/sources.list). "accepted", not "not an error": Sonar S7677 reads any
# message with "error" as an error.
rm -rf "$work/sources"
mkdir -p "$work/sources"
cases=$((cases + 1))
if APT_SOURCES_DIR="$work/sources" "$script" --prune-only >/dev/null 2>&1; then
    echo "selftest: ok   an empty sources.list.d is accepted"
else
    echo "selftest: FAIL an empty sources.list.d was rejected" >&2
    status=1
fi

cases=$((cases + 1))
if APT_SOURCES_DIR="$work/nowhere" "$script" --prune-only >/dev/null 2>&1; then
    echo "selftest: ok   a missing sources.list.d is accepted"
else
    echo "selftest: FAIL a missing sources.list.d was rejected" >&2
    status=1
fi

# No packages, no --prune-only: workflow lost its list. Refuse, not silent no-op
# (next step would fail with "command not found").
stage
cases=$((cases + 1))
if APT_SOURCES_DIR="$work/sources" "$script" >/dev/null 2>&1; then
    echo "selftest: FAIL naming no packages should not exit 0" >&2
    status=1
else
    echo "selftest: ok   naming no packages is refused"
fi

# Mirror down: stand-in apt fails `update` while $APT_DOWN holds "once" (first
# call) or "always"; `install` alike with "install-once", "install-always"
# (index fine, package download fails). Logs every call.
cat >"$work/apt-get" <<'EOF'
#!/usr/bin/env bash
echo "$*" >>"$APT_LOG"
case " $* " in
*" update "*)
    mode=$(cat "$APT_DOWN" 2>/dev/null || true)
    if [[ "$mode" = once ]]; then
        rm -f "$APT_DOWN"
        exit 100
    fi
    if [[ "$mode" = always ]]; then
        exit 100
    fi
    ;;
*" install "*)
    mode=$(cat "$APT_DOWN" 2>/dev/null || true)
    if [[ "$mode" = install-once ]]; then
        rm -f "$APT_DOWN"
        exit 100
    fi
    if [[ "$mode" = install-always ]]; then
        exit 100
    fi
    ;;
esac
exit 0
EOF
chmod +x "$work/apt-get"

# run_install <down mode>: script with stand-in apt, staged lists.
run_install() {
    local mode=$1
    stage
    printf 'deb http://de.archive.ubuntu.com/ubuntu jammy main\n' >"$work/sources.list"
    rm -f "$work/apt.log" "$work/down"
    if [[ -n "$mode" ]]; then
        echo "$mode" >"$work/down"
    fi
    APT_SOURCES_DIR="$work/sources" APT_SOURCES_LIST="$work/sources.list" \
        APT_GET="$work/apt-get" APT_LOG="$work/apt.log" APT_DOWN="$work/down" \
        "$script" qtbase5-dev >"$work/out" 2>&1
    return $?
}

# check <why> <command...>: one case, passes when command does.
check() {
    local why=$1
    shift
    cases=$((cases + 1))
    if "$@"; then
        echo "selftest: ok   $why"
    else
        echo "selftest: FAIL $why" >&2
        cat "$work/out" >&2
        status=1
    fi
    return 0
}

run_install ""
check "a working mirror installs at once" test "$(grep -c '' "$work/apt.log")" -eq 2
check "apt retries each file and gives up on a stalled one" \
    grep -q 'Acquire::Retries=3 -o Acquire::http::Timeout=30' "$work/apt.log"
check "a working mirror is left as it is" \
    grep -q 'azure.archive.ubuntu.com' "$work/sources/ubuntu.sources"

run_install once
check "a failed mirror is tried once more" test "$(grep -c '' "$work/apt.log")" -eq 3
check "the retry goes to archive.ubuntu.com (deb822)" \
    grep -q 'URIs: http://archive.ubuntu.com/ubuntu/' "$work/sources/ubuntu.sources"
check "the retry goes to archive.ubuntu.com (sources.list)" \
    grep -q 'deb http://archive.ubuntu.com/ubuntu jammy' "$work/sources.list"
check "the log says the mirror was switched" \
    grep -q 'now uses archive.ubuntu.com' "$work/out"

run_install install-once
check "a failed download is tried once more" test "$(grep -c '' "$work/apt.log")" -eq 4
check "the download retry goes to archive.ubuntu.com" \
    grep -q 'URIs: http://archive.ubuntu.com/ubuntu/' "$work/sources/ubuntu.sources"

# fails <mode> <why>: archive no better, job must fail.
fails() {
    local mode=$1 why=$2
    cases=$((cases + 1))
    if run_install "$mode"; then
        echo "selftest: FAIL $why should not exit 0" >&2
        status=1
    else
        echo "selftest: ok   $why"
    fi
    return 0
}
fails always "index from the archive failing too fails the job"
fails install-always "download from the archive failing too fails the job"

echo "selftest: $cases case(s)"
if [[ "$status" -ne 0 ]]; then
    echo "selftest: FAILED" >&2
fi
exit "$status"
