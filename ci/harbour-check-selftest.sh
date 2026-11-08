#!/bin/bash
# Prove ci/harbour-check.sh still fails what it claims to fail.
#
# A gate printing only "ok" may have stopped looking. Each case breaks one
# Harbour rule in a throwaway tree copy and asserts the check names it.
#
# Binary and bundled server aren't copied: checks needing them SKIP.
set -u

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Dotfiles too: rpm.yml is one thing a case breaks.
pristine="$work/pristine"
mkdir -p "$pristine"
tar -C "$root" -cf - \
    --exclude=./rust/target --exclude=./.git --exclude=./vendor \
    . 2>/dev/null | tar -C "$pristine" -xf -
if [[ ! -d "$pristine/ci" ]] || [[ ! -f "$pristine/.github/workflows/rpm.yml" ]]; then
    echo "selftest: FAIL could not stage a copy of the tree" >&2
    exit 1
fi

if [[ ! -f "$pristine/rpm/harbour-piirit.spec" ]] ||
    [[ ! -f "$pristine/harbour-piirit.desktop" ]]; then
    echo "selftest: FAIL the spec or the .desktop file is not where this test expects it" >&2
    exit 1
fi

status=0
cases=0
skipped=0

# break <expected id> <description> <sed-ish command run inside the copy>
break_and_expect() {
    local expect=$1 what=$2 script=$3
    local dir="$work/case"
    cases=$((cases + 1))

    rm -rf "$dir"
    cp -a "$pristine" "$dir"
    ( cd "$dir" && eval "$script" ) || {
        echo "selftest: FAIL could not apply '$what'" >&2
        status=1
        return
    }

    local out
    out=$("$dir/ci/harbour-check.sh" 2>&1)
    if grep -qF "FAIL [$expect]" <<< "$out"; then
        echo "selftest: ok   $what -> $expect"
    elif sed -n 's/^harbour-check: SKIP \[\([^]]*\)\].*/ \1 /p' <<< "$out" |
        grep -qF " $expect "; then
        echo "selftest: SKIP $what -> $expect (check skipped for want of a tool)"
        skipped=$((skipped + 1))
    else
        echo "selftest: FAIL $what should have been reported as $expect" >&2
        grep -E '^harbour-check: (FAIL|WAIVED)' <<< "$out" >&2 || echo "  (nothing failed)" >&2
        status=1
    fi
    return 0
}

S='rpm/harbour-piirit.spec'
D='harbour-piirit.desktop'
Q='qml/cover/CoverPage.qml'
# Only .qml with relative imports; two cases need it.
R='qml/piirit.qml'

break_and_expect 1.1.1 "package name without the harbour- prefix" \
    "sed -i 's/^Name:.*/Name:       piirit/' $S"
break_and_expect 1.1.3 "Version with a letter in it" \
    "sed -i 's/^Version:.*/Version:    0.1.0rc1/' $S"
break_and_expect 1.1.4 "Release with a letter in it" \
    "sed -i 's/^Release:.*/Release:    1.gabc123/' $S"
# No unescaped $ in case script: eval expands it before sed sees it.
break_and_expect 1.1.4 "an rpm workflow that stamps a git hash into Release" \
    "sed -i 's|^ *release=.*|          release=\"1.17.gabc1234\"|' .github/workflows/rpm.yml"
break_and_expect 1.2.1 "a file installed outside the allowed paths" \
    "sed -i 's|^%{_bindir}/%{name}\$|%{_bindir}/%{name}\\n/etc/harbour-piirit.conf|' $S"
break_and_expect 1.8.1 "a Vendor: tag" \
    "sed -i 's|^Group:.*|Vendor:     acme\\nGroup:      Qt/Qt|' $S"
break_and_expect 1.8.2 "an explicit Provides:" \
    "sed -i 's|^Group:.*|Provides:   piirit\\nGroup:      Qt/Qt|' $S"
break_and_expect 1.8.3 "a dependency that is not on the allowed list" \
    "sed -i 's|^Requires:   sailfishsilica-qt5\$|Requires:   sailfishsilica-qt5\\nRequires:   libcurl|' $S"
break_and_expect 1.8.4 "a versioned Requires" \
    "sed -i 's|^Requires:   sailfishsilica-qt5\$|Requires:   sailfishsilica-qt5 >= 0.10.9|' $S"
break_and_expect 1.8.5 "an RPM scriptlet" \
    "printf '\\n%%post\\n/bin/true\\n' >> $S"
break_and_expect 1.8.7 "requiring the sailfish-qml launcher without using it" \
    "sed -i 's|^Requires:   sailfishsilica-qt5\$|Requires:   sailfishsilica-qt5\\nRequires:   libsailfishapp-launcher|' $S"

break_and_expect 1.3.2 "an Exec= that is not the package name" \
    "sed -i 's|^Exec=.*|Exec=/usr/bin/harbour-piirit|' $D"
break_and_expect 1.3.3 "an Icon= with a path in it" \
    "sed -i 's|^Icon=.*|Icon=/usr/share/icons/x.png|' $D"
break_and_expect 1.3.4 "a missing Type=Application" \
    "sed -i 's|^Type=Application\$|Type=Service|' $D"
break_and_expect 1.3.5 "an X-Nemo-Application-Type that is not silica-qt5" \
    "sed -i 's|^X-Nemo-Application-Type=.*|X-Nemo-Application-Type=generic|' $D"
break_and_expect 1.3.6 "a [Sailjail] section header" \
    "sed -i 's|^\\[X-Sailjail\\]\$|[Sailjail]|' $D"
break_and_expect 1.4.1 "an OrganizationName with illegal characters" \
    "sed -i 's|^OrganizationName=.*|OrganizationName=Piirit!|' $D"
break_and_expect 1.4.2 "an OrganizationName component starting with a digit" \
    "sed -i 's|^OrganizationName=.*|OrganizationName=9piirit|' $D"
break_and_expect 1.4.4 "an ApplicationName with illegal characters" \
    "sed -i 's|^ApplicationName=.*|ApplicationName=.piirit|' $D"
break_and_expect 1.4.5 "a permission that is not on the whitelist" \
    "sed -i 's|^Permissions=.*|Permissions=Internet;Telepathy|' $D"
break_and_expect 1.4.5 "the Compatibility permission" \
    "sed -i 's|^Permissions=.*|Permissions=Internet;Compatibility|' $D"
break_and_expect 1.4.7 "a key that is not allowed in [X-Sailjail]" \
    "printf 'DBusName=org.example\\n' >> $D"
break_and_expect 2.5 "a sandbox grant the app's data path does not match" \
    "sed -i 's|^ApplicationName=.*|ApplicationName=Piirit|' $D"

break_and_expect 1.6.4 "a QML import at a version Harbour does not allow" \
    "sed -i 's|^import QtQuick 2.0\$|import QtQuick 2.7|' $Q"
break_and_expect 1.6.4 "an import that was dropped from the platform" \
    "sed -i '1i import QtWebKit 3.0' $Q"
break_and_expect 1.6.4 "a private QML module under a blocked prefix" \
    "sed -i 's|^import Piirit 1.0\$|import Nemo.Piirit 1.0|' $Q"
break_and_expect 1.6.5 "an absolute-path QML import" \
    "sed -i 's|^import \"pages\"\$|import \"/usr/share/harbour-piirit/qml/pages\"|' $R"
break_and_expect 1.6.6 "a relative import pointing outside the installed tree" \
    "sed -i 's|^import \"pages\"\$|import \"../icons\"|' $R"
break_and_expect 1.8.6 "XmlListModel used without requiring its package" \
    "sed -i '1i import QtQuick.XmlListModel 2.0' $Q"

break_and_expect 1.5.1 "a missing icon size" \
    "rm icons/128x128/harbour-piirit.png"
break_and_expect 1.5.4 "an icon whose pixels do not match its directory" \
    "cp icons/86x86/harbour-piirit.png icons/172x172/harbour-piirit.png"
break_and_expect 1.5.3 "an icon that is not a PNG" \
    "printf 'not a png' > icons/86x86/harbour-piirit.png"

break_and_expect 2.1 "a hardcoded /home/nemo path" \
    "sed -i 's|\"/usr/share/harbour-piirit/qml\"|\"/home/nemo/qml\"|' rust/piirit-app/src/main.rs"
break_and_expect 2.6 "a write into the installed data directory" \
    "sed -i 's|    let installed = |    let _ = std::fs::create_dir_all(\"/usr/share/harbour-piirit/x\");\\n    let installed = |' rust/piirit-app/src/main.rs"
break_and_expect 1.2.3 "a cargo binary named something other than the package" \
    "sed -i 's|^name = \"harbour-piirit\"\$|name = \"piirit\"|' rust/piirit-app/Cargo.toml"

# validate-rpm judges real validator output against the same waivers. Matching
# is subtle: layout errors name the path; linking errors name the *binary*,
# library in message. Fed saved logs (built RPM needs SDK).
validate_rpm() {
    local expect=$1 what=$2 body=$3
    local log="$work/validation.log"
    cases=$((cases + 1))
    printf '%s' "$body" > "$log"

    if "$pristine/ci/harbour-validate-rpm.sh" --log "$log" >/dev/null 2>&1; then
        local got=pass
    else
        local got=fail
    fi
    if [[ "$got" = "$expect" ]]; then
        echo "selftest: ok   $what -> $expect"
    else
        echo "selftest: FAIL $what should $expect, got $got" >&2
        "$pristine/ci/harbour-validate-rpm.sh" --log "$log" >&2
        status=1
    fi
    return 0
}

validate_rpm pass "an RPM breaking only the waived rules" \
'!BEGIN!x
ERROR|/usr/libexec/harbour-piirit|Installation not allowed in this location
ERROR|/usr/libexec/harbour-piirit/deltachat-rpc-server|ELF binary in wrong location
ERROR|/usr/libexec/harbour-piirit/deltachat-rpc-server|File must not be executable (current permissions: 755)
WARNING|/usr/bin/harbour-piirit|file is not stripped!
!END!FAIL!x
'
# Vendored qmetaobject patch keeps QtWidgets out of the binary; if lost, fail.
validate_rpm fail "a binary that links QtWidgets again" \
'!BEGIN!x
ERROR|/usr/bin/harbour-piirit|Cannot link to shared library: libQt5Widgets.so.5
!END!FAIL!x
'
# Waiver = path + accepted errors. Path-only match would swallow other errors
# there.
validate_rpm fail "a waived path with an error the waiver does not name" \
'!BEGIN!x
ERROR|/usr/libexec/harbour-piirit/deltachat-rpc-server|setuid, setgid or sticky bit set
!END!FAIL!x
'
validate_rpm fail "a waived path that has become a dynamic binary" \
'!BEGIN!x
ERROR|/usr/libexec/harbour-piirit/deltachat-rpc-server|Cannot link to shared library: libc.so.6
!END!FAIL!x
'
validate_rpm fail "a waived message about a path the waiver does not name" \
'!BEGIN!x
ERROR|/usr/share/harbour-piirit/qml/piirit.qml|File must not be executable (current permissions: 755)
!END!FAIL!x
'
validate_rpm fail "an RPM installing somewhere new" \
'!BEGIN!x
ERROR|/etc/harbour-piirit.conf|Installation not allowed in this location
!END!FAIL!x
'
validate_rpm fail "an RPM whose binary stopped exporting main()" \
'!BEGIN!x
ERROR|/usr/bin/harbour-piirit|Binary must export main() symbol for booster to work (Q_DECL_EXPORT)
!END!FAIL!x
'
validate_rpm fail "a dependency the allow-list does not carry" \
'!BEGIN!x
ERROR|libcurl.so.4|Dependency not allowed
!END!FAIL!x
'
validate_rpm pass "an RPM the validator accepts outright" \
'!BEGIN!x
!END!PASS!x
'
validate_rpm fail "a validation log that was cut off before the verdict" \
'ERROR|/usr/bin/harbour-piirit|something
'

# Node parent ignores SIGPIPE; children inherit it, so broken pipes print per
# write (4945 lines once). Wrapper resets it and hides leftovers.
validate_rpm pass "a log buried in broken-pipe noise" \
"!BEGIN!x
$(for _ in $(seq 1 50); do
    printf '%s\n' '/tmp/harbour-validator/rpmvalidation.sh: line 773: echo: write error: Broken pipe'
done)
WARNING|/usr/bin/harbour-piirit|file is not stripped!
!END!PASS!x
"

# Noise never echoed, however it got into log: verdict must be findable.
cases=$((cases + 1))
noise_log="$work/noisy.log"
printf '%s\n' \
    'sh: line 773: echo: write error: Broken pipe' \
    '!BEGIN!x' '!END!PASS!x' > "$noise_log"
noise_out=$("$pristine/ci/harbour-validate-rpm.sh" --log "$noise_log" 2>&1)
if grep -q 'Broken pipe' <<< "$noise_out"; then
    echo "selftest: FAIL the wrapper echoed the validator's broken-pipe noise" >&2
    status=1
else
    echo "selftest: ok   broken-pipe noise -> not echoed"
fi

# Waiver file honest both ways: stale entry = defect, like missing check.
cases=$((cases + 1))
dir="$work/case"
rm -rf "$dir"
cp -a "$pristine" "$dir"
printf '9.9.9  /nowhere  # excuses nothing\n' >> "$dir/ci/harbour/waivers.conf"
if "$dir/ci/harbour-check.sh" 2>&1 | grep -q "stale waiver '9.9.9 "; then
    echo "selftest: ok   a waiver that matches nothing -> stale waiver"
else
    echo "selftest: FAIL a waiver that matches nothing should be reported as stale" >&2
    status=1
fi

# Skipped check never looked: its waivers not stale, others still are. PATH
# copy minus rpmspec stands in for machine without rpm.
cases=$((cases + 1))
norpm="$work/norpm"
mkdir -p "$norpm"
IFS=: read -ra path_dirs <<< "$PATH"
for d in "${path_dirs[@]}"; do
    for f in "$d"/*; do
        n=${f##*/}
        [[ $n = rpmspec || -e $norpm/$n || -L $norpm/$n || ! -x $f ]] && continue
        ln -s "$f" "$norpm/$n"
    done
done
out=$(PATH="$norpm" HARBOUR_CHECK_STRICT=0 "$dir/ci/harbour-check.sh" 2>&1)
if ! grep -qE 'SKIP \[1\.2\.1[] ]' <<< "$out"; then
    echo "selftest: FAIL could not hide rpmspec from the check" >&2
    status=1
elif grep 'stale waiver' <<< "$out" | grep -qv "stale waiver '9.9.9 " ||
    ! grep -q "stale waiver '9.9.9 " <<< "$out"; then
    echo "selftest: FAIL with rpmspec missing, only the waiver for a check that ran should be stale" >&2
    grep 'stale waiver' <<< "$out" >&2 || echo "  (no stale waiver)" >&2
    status=1
else
    echo "selftest: ok   rpmspec missing -> its waivers not stale, others still are"
fi

echo
if [[ "$status" -eq 0 ]]; then
    echo "selftest: ok ($cases cases, $skipped skipped for want of a tool)"
else
    echo "selftest: FAILED" >&2
fi
exit "$status"
