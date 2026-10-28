#!/bin/bash
# Harbour listing rules, from the source tree.
#
# Authority: `sfdk check -s harbour` (needs built RPM, rpm.yml). This reads
# sources, so intake failures fail the PR.
#
# Rules: vendored validator allow-lists (ci/harbour/, refreshed by
# scripts/update-harbour-rules.sh); only logic reimplemented, from
# rpmvalidation.sh. IDs: docs/HARBOUR.md (Jolla's numbering).
#
# bash, not sh: .conf files are extglob patterns; other matchers accept what
# Harbour rejects.
set -u
shopt -s extglob

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
rules="$root/ci/harbour"
waivers="$rules/waivers.conf"

# Found, not named: names follow the package (rename spec, check follows).
spec=$(find "$root/rpm" -maxdepth 1 -name '*.spec' | sort | head -1)
if [[ -z "$spec" ]]; then
    echo "harbour-check: FAIL no .spec file in rpm/" >&2
    exit 1
fi

# Both arches: %ifarch changes file list; Harbour checks every RPM.
ARCHES="aarch64 armv7hl"

# rpmvalidation.conf's ICON_SIZES.
ICON_SIZES="86x86 108x108 128x128 172x172"

status=0
findings=0
skipped=0
declare -A waiver_used=()
declare -A skipped_id=()

# HARBOUR_CHECK_STRICT=1: missing tool fails. CI sets it.
strict=${HARBOUR_CHECK_STRICT:-0}

note() { echo "harbour-check: $*"; return 0; }

# skip "<ids>" <why>: every check id the missing tool stops.
skip() {
    local id=$1 why=$2 i
    for i in $id; do skipped_id[$i]=1; done
    if [[ "$strict" = 1 ]]; then
        echo "harbour-check: FAIL [$id] cannot run: $why (strict mode)" >&2
        status=1
    else
        echo "harbour-check: SKIP [$id] $why"
        skipped=$((skipped + 1))
    fi
    return 0
}

# Finding = (check id, subject, message). Waivers match the stable subject part.
fail() {
    local id=$1 subject=$2 message=$3
    local key
    if key=$(waived "$id" "$subject"); then
        waiver_used[$key]=1
        echo "harbour-check: WAIVED [$id] $subject -- $message"
        return
    fi
    echo "harbour-check: FAIL [$id] $subject -- $message" >&2
    findings=$((findings + 1))
    status=1
    return 0
}

# waivers.conf: `<check id> <subject pattern> # why`. Returns key for stale
# reports.
waived() {
    local id=$1 subject=$2 line wid wsubject
    [[ -f "$waivers" ]] || return 1
    while IFS= read -r line; do
        line=${line%%#*}
        # shellcheck disable=SC2086 # deliberate: split into id and pattern.
        set -- $line
        [[ $# -ge 2 ]] || continue
        wid=$1
        wsubject=$2
        # Glob, like upstream's allow-list.
        # shellcheck disable=SC2053
        if [[ "$id" = "$wid" ]] && [[ $subject == $wsubject ]]; then
            echo "$wid $wsubject"
            return 0
        fi
    done < "$waivers"
    return 1
}

# rpmvalidation.sh's check_contained_in: query vs .conf patterns.
contained_in() {
    local query=$1 pat
    shift
    while read -r pat; do
        [[ -z $pat || $pat == '#'* ]] && continue
        # shellcheck disable=SC2053
        [[ $query == $pat ]] && return 0
        # shellcheck disable=SC2053
        [[ $query == "$pat()(64bit)" ]] && return 0
    done < <(cat "$@")
    return 1
}

#
# 1.1 Naming
#
name=$(sed -n 's/^Name:[[:space:]]*//p' "$spec" | head -1 | tr -d '[:space:]')
version=$(sed -n 's/^Version:[[:space:]]*//p' "$spec" | head -1 | tr -d '[:space:]')
release=$(sed -n 's/^Release:[[:space:]]*//p' "$spec" | head -1 | tr -d '[:space:]')

if [[ -z "$name" ]]; then
    echo "harbour-check: FAIL $spec has no Name:" >&2
    exit 1
fi

# 1.3.3 wants Icon=<NAME>; 1.2.2 wants <NAME>.desktop: source named for package.
desktop="$root/$name.desktop"

if [[ $name =~ ^harbour-[-a-z0-9_.]+$ ]]; then
    note "[1.1.1] package name '$name' is a valid Harbour name"
else
    fail 1.1.1 "$name" \
        "package name must match '^harbour-[-a-z0-9_.]+\$' (lowercase, harbour- prefix)"
fi

if [[ $version =~ ^[0-9.]+$ ]]; then
    note "[1.1.3] Version '$version' is digits and periods"
else
    fail 1.1.3 "$version" "Version may contain only digits and periods"
fi

# Tree Release; built package gets rpm.yml's stamp (below).
if [[ $release =~ ^[0-9._]+$ ]]; then
    note "[1.1.4] Release '$release' is digits, underscores and periods"
else
    fail 1.1.4 "$release" \
        "Release may contain only digits, underscores and periods"
fi

# 1.1.2 (file name) and 1.1.5 (arch) follow from Name/Version/Release/arch,
# so decided by what the rpm workflow can be asked to build.
if [[ -f "$root/.github/workflows/rpm.yml" ]]; then
    while read -r arch; do
        case "$arch" in
            armv7hl|aarch64|i486|noarch)
                ;;
            *)
                fail 1.1.5 "$arch" \
                    "architecture must be armv7hl, aarch64, i486 or noarch"
                ;;
        esac
    done < <(sed -n 's/.*options:[[:space:]]*\[\(.*\)\].*/\1/p' \
        "$root/.github/workflows/rpm.yml" | tr ',' '\n' | tr -d '[:space:]')
    note "[1.1.5] the rpm workflow offers only Harbour architectures"

    # Stamped Release is the one reaching the RPM.
    stamped=$(sed -n 's/.*release="\(.*\)".*/\1/p' "$root/.github/workflows/rpm.yml" | head -1)
    if [[ -n "$stamped" ]]; then
        # Plausible value per expansion, then judge shape.
        # shellcheck disable=SC2016 # the patterns are the literal text.
        shape=$(echo "$stamped" | sed -e 's/\${GITHUB_RUN_NUMBER}/17/g' -e 's/\${short}/abc1234/g')
        if [[ $shape =~ ^[0-9._]+$ ]]; then
            note "[1.1.4] the rpm workflow stamps a Harbour-legal Release"
        else
            fail 1.1.4 "$stamped" \
                "the Release stamped by rpm.yml expands to '$shape', which is not digits, underscores and periods"
        fi
    fi
fi

#
# 1.2 File layout; 1.6.7/1.8.x: read off spec
#
if ! command -v rpmspec >/dev/null 2>&1; then
    skip "1.2.1 1.2.2 1.2.3 1.2.4 1.2.6 1.2.7 1.2.8 1.5.2 1.6.7 1.8.1 1.8.2 1.8.3 1.8.4 1.8.5 1.8.6 1.8.7" \
        "rpmspec not found (install rpm)"
else
    for arch in $ARCHES; do
        expanded=$(rpmspec -P --target "$arch" "$spec" 2>/dev/null)
        if [[ -z "$expanded" ]]; then
            fail 1.2.1 "$spec" "does not parse for $arch"
            continue
        fi

        # %files entries, minus directives and attributes.
        files=$(echo "$expanded" | sed -n '/^%files/,/^%[a-z]*$/p' |
            grep -v '^%files' | grep -v '^%defattr' | grep -v '^%changelog' |
            sed -e 's/^%dir[[:space:]]*//' -e 's/^%attr([^)]*)[[:space:]]*//' \
                -e 's/^%config([^)]*)[[:space:]]*//' -e 's/^%doc[[:space:]]*//' |
            grep '^/' || true)

        while read -r path; do
            [[ -n "$path" ]] || continue
            case "$path" in
                "/usr/bin/$name") ;;
                "/usr/share/applications/$name.desktop") ;;
                "/usr/share/icons/hicolor/"*"/apps/$name.png") ;;
                "/usr/share/$name"|"/usr/share/$name/"*) ;;
                /home/*)
                    fail 1.2.6 "$path" \
                        "nothing may be installed under /home ($arch)"
                    ;;
                /usr/lib/debug*|/usr/src/debug*)
                    fail 1.2.4 "$path" \
                        "debug symbols and sources must not be packaged ($arch)"
                    ;;
                *)
                    fail 1.2.1 "$path" \
                        "installation not allowed in this location; only /usr/bin/$name, /usr/share/applications/$name.desktop, /usr/share/icons/hicolor/<size>/apps/$name.png and /usr/share/$name/** may be packaged ($arch)"
                    ;;
            esac
        done <<< "$files"

        if echo "$files" | grep -qx "/usr/share/applications/$name.desktop"; then
            note "[1.2.2] $arch packages /usr/share/applications/$name.desktop"
        else
            fail 1.2.2 "/usr/share/applications/$name.desktop" \
                "the .desktop file is not in %files ($arch)"
        fi

        # 1.2.3: not sailfish-qml app (see 1.3.2): binary must exist.
        if echo "$files" | grep -qx "/usr/bin/$name"; then
            note "[1.2.3] $arch packages /usr/bin/$name"
        else
            fail 1.2.3 "/usr/bin/$name" \
                "a C++/QML app must install its binary as /usr/bin/$name ($arch)"
        fi

        # install(1) calls: mode and destination of every packaged file.
        # Continuation lines joined (split call = one src, one dst);
        # expanded %{buildroot} prefix stripped from destination.
        install_section=$(echo "$expanded" | sed -n '/^%install/,/^%files/p' |
            sed -e ':a' -e '/\\$/{N;s/\\\n[[:space:]]*/ /;ta' -e '}' |
            sed -E 's#[^ ]*/BUILDROOT/[^/ ]+##g')
        while read -r line; do
            mode=$(echo "$line" | sed -n 's/.*-D\{0,1\}m[[:space:]]*\([0-7]\{3,4\}\).*/\1/p')
            [[ -n "$mode" ]] || continue
            # Pad to 4 digits: setuid/setgid/sticky digit in fixed place.
            [[ ${#mode} -eq 3 ]] && mode="0$mode"
            dst=$(echo "$line" | awk '{print $NF}')
            case "${mode:0:1}" in
                0) ;;
                *) fail 1.2.8 "$dst" "setuid, setgid or sticky bit set (mode $mode)" ;;
            esac
            [[ ${mode:3:1} == [2367] ]] && fail 1.2.7 "$dst" "world-writable (mode $mode)"
            [[ ${mode:2:1} == [2367] ]] && fail 1.2.7 "$dst" "group-writable (mode $mode)"
        done <<< "$install_section"

        # 1.6.7: ELF only at /usr/bin/<NAME> or /usr/share/<NAME>/lib/. Read
        # from install(1) calls: where tree source pairs with landing path.
        while read -r line; do
            case "$line" in install*) ;; *) continue ;; esac
            src=$(echo "$line" | awk '{print $(NF-1)}')
            dst=$(echo "$line" | awk '{print $NF}')
            dst=${dst//\"/}
            src=${src//\"/}
            # App binary is built, not checked out (1.7 covers it). Other
            # tree files must exist, else check reads half a package;
            # `scripts/fetch-rpc-server.sh` places the bundled ones.
            if [[ ! -f "$root/$src" ]]; then
                # shellcheck disable=SC2016 # '$builddir' is literal text.
                case "$src" in
                    *'$builddir'*|*target/*) ;;
                    *) skip 1.6.7 "$src is not in the tree, so its file type is unknown ($arch)" ;;
                esac
                continue
            fi
            case "$(file -b "$root/$src" 2>/dev/null)" in
                ELF*) ;;
                *) continue ;;
            esac
            case "$dst" in
                "/usr/bin/$name") ;;
                "/usr/share/$name/lib/"*.so|"/usr/share/$name/lib/"*.so.*) ;;
                *)
                    fail 1.6.7 "$dst" \
                        "ELF binary in a location Harbour does not allow; only /usr/bin/$name and private shared libraries under /usr/share/$name/lib/ may be ELF ($arch)"
                    ;;
            esac
        done <<< "$install_section"

        # 1.5.2: all four icon sizes installed.
        for size in $ICON_SIZES; do
            if echo "$install_section" |
                grep -q "/usr/share/icons/hicolor/$size/apps/$name.png"; then
                continue
            fi
            fail 1.5.2 "/usr/share/icons/hicolor/$size/apps/$name.png" \
                "icon size $size is not installed ($arch)"
        done
    done
    note "[1.2.x] file layout checked for: $ARCHES"

    #
    # 1.8 RPM metadata: spec-stated tags only; rpm-generated Requires/Provides
    # are the real validator's (.github/workflows/rpm.yml).
    #
    expanded=$(rpmspec -P --target aarch64 "$spec" 2>/dev/null)

    if grep -qE '^Vendor:' <<< "$expanded"; then
        fail 1.8.1 "Vendor" "a Vendor: tag must not be set"
    else
        note "[1.8.1] no Vendor: tag"
    fi

    for tag in Provides Obsoletes Conflicts Recommends Suggests Supplements Enhances; do
        while read -r value; do
            [[ -n "$value" ]] || continue
            fail 1.8.2 "$tag: $value" "'$tag:' is not allowed in a Harbour RPM"
        done < <(sed -n "s/^$tag:[[:space:]]*//p" <<< "$expanded")
    done
    note "[1.8.2] no Provides:/Obsoletes:/Conflicts:/Recommends:/Suggests:/Supplements:/Enhances:"

    # BuildRequires not shipped: runtime Requires only.
    requires=$(sed -n 's/^Requires:[[:space:]]*//p' <<< "$expanded")
    while read -r req; do
        [[ -n "$req" ]] || continue
        # Versioned dep = three tokens; operator and version both rejected.
        if [[ $req == *[[:space:]]* ]]; then
            fail 1.8.4 "$req" \
                "Requires: must not be versioned -- Harbour derives its own compatibility range"
            req=${req%%[[:space:]]*}
        fi
        if contained_in "$req" "$rules/allowed_libraries.conf" "$rules/allowed_requires.conf"; then
            continue
        fi
        if contained_in "$req" "$rules/deprecated_libraries.conf" "$rules/deprecated_requires.conf"; then
            fail 1.8.3 "$req" "dependency is deprecated and will stop being accepted"
            continue
        fi
        if contained_in "$req" "$rules/dropped_libraries.conf" "$rules/dropped_requires.conf"; then
            fail 1.8.3 "$req" "dependency was dropped from the platform and is no longer accepted"
            continue
        fi
        fail 1.8.3 "$req" "dependency is not on Harbour's allowed list"
    done <<< "$requires"
    note "[1.8.3/1.8.4] Requires: checked against ci/harbour/{allowed,deprecated,dropped}_*.conf"

    for scriptlet in pre post preun postun pretrans posttrans verifyscript triggerin triggerun triggerpostun filetriggerin; do
        if grep -qE "^%$scriptlet\b" <<< "$expanded"; then
            fail 1.8.5 "%$scriptlet" "RPM scriptlets and triggers are not allowed"
        fi
    done
    note "[1.8.5] no RPM scriptlets or triggers"
fi

#
# 1.6 QML imports
#
uses_silica=0
uses_xmllistmodel=0
qml_files=0
while IFS= read -r qml; do
    qml_files=$((qml_files + 1))
    relative=${qml#"$root"/}
    while IFS= read -r line; do
        # One line, several statements: `import a 1.0; import b 1.0`.
        while IFS= read -r statement; do
            [[ -n "$statement" ]] || continue
            # As rpmvalidation.sh: drop `as Foo`, collapse spaces, keep
            # module+version.
            import=$(sed -e 's/^[[:space:]]*import/import/' -e 's/[[:space:]]\+/ /g' \
                -e 's/ as .*$//' -e 's/;$//' <<< "$statement" | cut -f2-3 -d' ')
            [[ -n "$import" ]] || continue

            [[ "$import" = "Sailfish.Silica 1.0" ]] && uses_silica=1
            [[ $import == QtQuick.XmlListModel* ]] && uses_xmllistmodel=1

            if contained_in "$import" "$rules/allowed_qmlimports.conf"; then
                continue
            fi
            if contained_in "$import" "$rules/deprecated_qmlimports.conf"; then
                fail 1.6.4 "$import" \
                    "QML import is deprecated and will stop being accepted ($relative)"
                continue
            fi
            # Named, not left to blocked-prefix patterns: those would say
            # "not at this version" about it.
            if contained_in "$import" "$rules/dropped_qmlimports.conf"; then
                fail 1.6.4 "$import" \
                    "QML import was dropped from the platform and is no longer accepted ($relative)"
                continue
            fi

            case "$import" in
                [\"\']*)
                    # Path import: strip quotes; cut(1)'s version is not path.
                    path=${import%%[[:space:]]*}
                    path=${path//[\"\']/}
                    case "$path" in
                        /*)
                            fail 1.6.5 "$import" \
                                "absolute path imports are forbidden ($relative)"
                            ;;
                        qrc:/*)
                            ;;
                        *)
                            # qml/ installs as /usr/share/<NAME>/qml: staying
                            # inside it keeps import in package. Script import
                            # names a file: its directory must be inside.
                            if [[ $path == *.js ]]; then
                                target=$(cd "$(dirname "$qml")" 2>/dev/null &&
                                    [[ -f "$path" ]] &&
                                    cd "$(dirname "$path")" 2>/dev/null && pwd)
                            else
                                target=$(cd "$(dirname "$qml")" 2>/dev/null &&
                                    cd "$path" 2>/dev/null && pwd)
                            fi
                            if [[ -z "$target" ]]; then
                                fail 1.6.6 "$import" \
                                    "relative import does not resolve to a directory ($relative)"
                            elif [[ "${target#"$root/qml"}" = "$target" ]]; then
                                fail 1.6.6 "$import" \
                                    "relative import resolves to '$target', outside the installed qml/ tree ($relative)"
                            fi
                            ;;
                    esac
                    ;;
                *)
                    # Unblocked = allowed: app registers its own QML module.
                    if contained_in "$import" "$rules/disallowed_qmlimport_patterns.conf"; then
                        fail 1.6.4 "$import" \
                            "QML import is not allowed at this version; see ci/harbour/allowed_qmlimports.conf ($relative)"
                    fi
                    ;;
            esac
        done < <(tr ';' '\n' <<< "$line")
    done < <(grep -e '^[[:space:]]*import[[:space:]]' "$qml" | sed -e 's/\x0D$//')
done < <(find "$root/qml" -name '*.qml' | sort)

if [[ "$qml_files" -eq 0 ]]; then
    fail 1.6.4 "qml/" "no .qml files were found -- did the tree move?"
else
    note "[1.6.x] imports checked in $qml_files .qml files"
fi

#
# 1.3 .desktop file, 1.4 [X-Sailjail]
#
uses_sailfish_qml=0
if [[ ! -f "$desktop" ]]; then
    fail 1.3.1 "$desktop" "the .desktop file is missing"
else
    if grep -qE '^Name=.+' "$desktop"; then
        note "[1.3.1] Name= is present and non-empty"
    else
        fail 1.3.1 "Name" "a non-empty Name= is required"
    fi

    # Launch form decides three later checks: /usr/bin/<NAME> required,
    # libsailfishapp-launcher required, what ExecDBus may say.
    if grep -qE "^Exec=sailfish-qml[[:space:]]+${name}[[:space:]]*$" "$desktop"; then
        uses_sailfish_qml=1
        note "[1.3.2] Exec=sailfish-qml $name (QML-only app)"
    elif grep -qE "^Exec=${name}[[:space:]]*$" "$desktop"; then
        note "[1.3.2] Exec=$name"
    else
        fail 1.3.2 "Exec" \
            "must be 'Exec=$name' or 'Exec=sailfish-qml $name'; found '$(grep -m1 '^Exec=' "$desktop")'"
    fi

    if grep -qE "^Icon=${name}[[:space:]]*$" "$desktop"; then
        note "[1.3.3] Icon=$name"
    else
        fail 1.3.3 "Icon" \
            "must be the bare name 'Icon=$name', with no path and no extension"
    fi

    if grep -qE '^Type=Application[[:space:]]*$' "$desktop"; then
        note "[1.3.4] Type=Application"
    else
        fail 1.3.4 "Type" "must be exactly 'Type=Application'"
    fi

    # Silica import needs silica-qt5, else booster won't start it.
    if grep -qE '^X-Nemo-Application-Type=silica-qt5[[:space:]]*$' "$desktop"; then
        note "[1.3.5] X-Nemo-Application-Type=silica-qt5"
    elif [[ "$uses_silica" = 1 ]]; then
        fail 1.3.5 "X-Nemo-Application-Type" \
            "must be silica-qt5: the app imports Sailfish.Silica"
    elif grep -qE '^X-Nemo-Application-Type=(no-invoker|generic|qtquick2|qt5)[[:space:]]*$' "$desktop"; then
        note "[1.3.5] X-Nemo-Application-Type is an accepted non-Silica value"
    else
        fail 1.3.5 "X-Nemo-Application-Type" \
            "must be declared; use silica-qt5 for a Silica app"
    fi

    if grep -qE '^\[Sailjail\][[:space:]]*$' "$desktop"; then
        fail 1.3.6 "[Sailjail]" "the section header must be [X-Sailjail]"
    fi

    if ! grep -qE '^\[X-Sailjail\][[:space:]]*$' "$desktop"; then
        fail 1.3.7 "[X-Sailjail]" "the section is missing"
    else
        sailjail=$(sed -n '/^\[X-Sailjail\]/,$p' "$desktop" |
            sed '1d;/^\[/,$d' | grep -vE '^[[:space:]]*(#|$)')
        if [[ -z "$sailjail" ]]; then
            fail 1.3.7 "[X-Sailjail]" "the section must not be empty"
        fi

        org=""
        app=""
        while IFS= read -r line; do
            [[ -n "$line" ]] || continue
            key=${line%%=*}
            value=${line#*=}
            if ! contained_in "$key" "$rules/allowed_sailjailkeys.conf"; then
                fail 1.4.7 "$key" "not an allowed [X-Sailjail] key"
                continue
            fi
            case "$key" in
                OrganizationName)
                    org=$value
                    if [[ ! $value =~ ^[0-9a-z._-]+$ ]]; then
                        fail 1.4.1 "$value" \
                            "OrganizationName must match '^[0-9a-z._-]+\$'"
                    fi
                    if [[ $value =~ (^|[.])[0-9] ]]; then
                        fail 1.4.2 "$value" \
                            "no dot-separated component of OrganizationName may start with a digit"
                    fi
                    if contained_in "$value" "$rules/disallowed_orgnames.conf"; then
                        fail 1.4.3 "$value" "OrganizationName is reserved"
                    fi
                    ;;
                ApplicationName)
                    app=$value
                    if [[ ! $value =~ ^[A-Za-z_-][A-Z0-9a-z_-]*$ ]]; then
                        fail 1.4.4 "$value" \
                            "ApplicationName must match '^[A-Za-z_-][A-Z0-9a-z_-]*\$'"
                    fi
                    ;;
                Permissions)
                    while IFS= read -r permission; do
                        [[ -n "$permission" ]] || continue
                        if ! contained_in "$permission" "$rules/allowed_permissions.conf"; then
                            fail 1.4.5 "$permission" "permission is not on Harbour's whitelist"
                        elif [[ "$permission" = Compatibility ]]; then
                            fail 1.4.5 "$permission" \
                                "the Compatibility permission exists for pre-sandboxing apps and invites QA scrutiny"
                        fi
                    done < <(tr ';' '\n' <<< "$value")
                    ;;
                ExecDBus)
                    if [[ "$uses_sailfish_qml" = 1 ]]; then
                        expect="^sailfish-qml[[:space:]]+$name([[:space:]]+[A-Za-z_-][A-Z0-9a-z_-]*)?$"
                    else
                        expect="^$name([[:space:]]+[A-Za-z_-][A-Z0-9a-z_-]*)?$"
                    fi
                    if [[ ! $value =~ $expect ]]; then
                        fail 1.4.6 "$value" \
                            "ExecDBus must be the Exec value, optionally plus one argument"
                    fi
                    ;;
                # Other allowed keys: nothing to check.
                *) ;;
            esac
        done <<< "$sailjail"
        note "[1.4.x] [X-Sailjail] keys, OrganizationName, ApplicationName and Permissions checked"

        # 2.5: sandbox grants $XDG_DATA_HOME/<Org>/<App>; app path must match.
        # One-side rename is silent until app is confined on device.
        if [[ -n "$org" ]] && [[ -n "$app" ]]; then
            if grep -rqF --exclude-dir=target "\"$org/$app/" "$root/rust"; then
                note "[2.5] the app's data path uses OrganizationName/ApplicationName ($org/$app)"
            else
                fail 2.5 "$org/$app" \
                    "no source file builds a data path under '$org/$app/', so the sandbox grant and the app disagree"
            fi
        fi
    fi
fi

if command -v desktop-file-validate >/dev/null 2>&1; then
    note "[1.3.x] desktop entry syntax is ci/packaging-lint.sh's"
fi

# 1.8.6/1.8.7: Requires derived from app; runs after QML and .desktop passes.
if [[ -n "${expanded:-}" ]]; then
    if [[ "$uses_xmllistmodel" = 1 ]] &&
        ! grep -qE '^Requires:[[:space:]]*qt5-qtdeclarative-import-xmllistmodel' <<< "$expanded"; then
        fail 1.8.6 "qt5-qtdeclarative-import-xmllistmodel" \
            "QtQuick.XmlListModel is imported but not required; it is not on devices by default"
    fi
    if grep -qE '^Requires:[[:space:]]*libsailfishapp-launcher' <<< "$expanded" &&
        [[ "$uses_sailfish_qml" = 0 ]]; then
        fail 1.8.7 "libsailfishapp-launcher" \
            "required but the .desktop file does not use the sailfish-qml launcher; drop the dependency"
    fi
    if [[ "$uses_sailfish_qml" = 1 ]] &&
        ! grep -qE '^Requires:[[:space:]]*libsailfishapp-launcher' <<< "$expanded"; then
        fail 1.8.7 "libsailfishapp-launcher" \
            "a sailfish-qml app must require the package that provides the launcher"
    fi
fi

#
# 1.5 Icons
#
if ! command -v file >/dev/null 2>&1; then
    skip 1.5.3 "file(1) not found"
else
    for size in $ICON_SIZES; do
        icon="$root/icons/$size/$name.png"
        if [[ ! -f "$icon" ]]; then
            fail 1.5.1 "icons/$size/$name.png" "icon is missing"
            continue
        fi
        described=$(file -b "$icon")
        case "$described" in
            "PNG image data, ${size%x*} x ${size#*x},"*)
                ;;
            PNG*)
                fail 1.5.4 "icons/$size/$name.png" \
                    "pixel dimensions must match the directory name; file(1) reads '$described'"
                ;;
            *)
                fail 1.5.3 "icons/$size/$name.png" \
                    "must be a PNG; file(1) reads '$described'"
                ;;
        esac
    done
    note "[1.5.x] icons checked for: $ICON_SIZES"
fi

#
# 1.7 The binary
#
# cargo bin named for package: %install copies it to /usr/bin/<NAME>.
cargo_bin=$(sed -n '/^\[\[bin\]\]/,/^\[/p' "$root/rust/piirit-app/Cargo.toml" |
    sed -n 's/^name = "\(.*\)"/\1/p' | head -1)
if [[ "$cargo_bin" = "$name" ]]; then
    note "[1.2.3] the cargo binary is named '$name'"
else
    fail 1.2.3 "$cargo_bin" \
        "piirit-app's [[bin]] name must be '$name', the name the spec installs to /usr/bin"
fi

binary=""
for candidate in "$root/rust/target/release/$name" "$root/rust/target/debug/$name"; do
    [[ -x "$candidate" ]] && { binary=$candidate; break; }
done

if [[ -z "$binary" ]]; then
    skip 1.7.3 "no built binary (run: cd rust && cargo build -p piirit-app)"
elif ! command -v readelf >/dev/null 2>&1; then
    skip 1.7.3 "readelf not found (install binutils)"
else
    if readelf --wide --syms "$binary" 2>/dev/null | grep -q 'UND __libc_start_main'; then
        note "[1.7.2] the binary links __libc_start_main"
    else
        fail 1.7.2 "$(basename "$binary")" \
            "the binary must link __libc_start_main"
    fi

    # --dyn-syms: booster dlopen()s, looks up main() dynamically; rpmbuild
    # strips .symtab, so only a dynamic symbol survives.
    if readelf --wide --dyn-syms "$binary" 2>/dev/null |
        awk '$4 == "FUNC" && $8 == "main"' | grep -q .; then
        note "[1.7.3] the binary exports main() as a dynamic symbol"
    else
        fail 1.7.3 "$(basename "$binary")" \
            "a Silica app must export main() for the mapplauncherd booster; a plain Rust 'fn main' is a local symbol that stripping removes"
    fi

    # 1.6.1: NEEDED vs allow-list. Host binary names host loader (device's is
    # listed too); Qt/C++ libs, the real question, same on both.
    if ! command -v objdump >/dev/null 2>&1; then
        skip 1.6.1 "objdump not found (install binutils)"
    else
        while read -r lib; do
            [[ -n "$lib" ]] || continue
            [[ $lib == ld-linux* ]] && continue
            if contained_in "$lib" "$rules/allowed_libraries.conf"; then
                continue
            fi
            if contained_in "$lib" "$rules/deprecated_libraries.conf"; then
                fail 1.6.1 "$lib" "linked library is deprecated"
                continue
            fi
            if contained_in "$lib" "$rules/dropped_libraries.conf"; then
                fail 1.6.1 "$lib" "linked library was dropped from the platform"
                continue
            fi
            fail 1.6.1 "$lib" \
                "cannot link to this shared library; it is not on Harbour's allowed list and the package does not ship it"
        done < <(objdump -x "$binary" 2>/dev/null |
            awk '/NEEDED/ {print $2}' | sort -u)
        note "[1.6.1] linked libraries checked against ci/harbour/allowed_libraries.conf"
    fi
fi

#
# 2.1 Hardcoded home directories
#
hits=$(grep -rn --include='*.rs' --include='*.qml' --include='*.js' \
    --exclude-dir=target -E '/home/(nemo|defaultuser)' "$root/rust" "$root/qml" 2>/dev/null |
    grep -v '/target/' || true)
if [[ -n "$hits" ]]; then
    while IFS= read -r hit; do
        fail 2.1 "${hit%%:*}" "hardcoded home directory: ${hit#*:}"
    done <<< "$hits"
else
    note "[2.1] no hardcoded /home/nemo or /home/defaultuser"
fi

#
# 2.6 Nothing the package installs is written at runtime
#
# Narrow (no variable tracking) but exact; broad check is §10 on-device run.
writes='create_dir_all|create_dir|File::create|fs::write|fs::copy|fs::rename|remove_file|remove_dir|OpenOptions'
if hits=$(grep -rnE "\"(/usr/(share|bin|libexec)/$name)" --include='*.rs' \
        --exclude-dir=target "$root/rust" |
    grep -E "$writes"); then
    while IFS= read -r hit; do
        fail 2.6 "${hit%%:*}" \
            "writes to a path the package installs, which the package manager owns and an upgrade overwrites: ${hit#*:}"
    done <<< "$hits"
else
    note "[2.6] nothing writes to an installed path"
fi

#
# Waiver matching nothing: stale. Skipped check matched nothing because it
# never looked: not stale.
#
if [[ -f "$waivers" ]]; then
    while IFS= read -r line; do
        line=${line%%#*}
        # shellcheck disable=SC2086
        set -- $line
        [[ $# -ge 2 ]] || continue
        [[ -n "${skipped_id[$1]:-}" ]] && continue
        if [[ -z "${waiver_used["$1 $2"]:-}" ]]; then
            echo "harbour-check: FAIL stale waiver '$1 $2' in ci/harbour/waivers.conf matches nothing; delete it" >&2
            status=1
        fi
    done < "$waivers"
fi

echo
if [[ "$status" -eq 0 ]]; then
    if [[ "$skipped" -gt 0 ]]; then
        note "ok, with $skipped check(s) skipped for want of a tool"
    else
        note "ok"
    fi
else
    if [[ "$findings" -gt 0 ]]; then
        note "FAILED: $findings finding(s)" >&2
    else
        note "FAILED" >&2
    fi
    note "the authority is 'sfdk check -s harbour' on a built RPM; see docs/HARBOUR.md" >&2
fi
exit "$status"
