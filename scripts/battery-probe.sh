#!/bin/sh
# Measure how fast a Sailfish phone discharges over a long idle stretch, so
# the cost of leaving Postivene running in the background can be read as a
# number rather than argued about.
#
# Run it twice under conditions that differ in exactly one thing -- the app
# minimised on one night, swiped away on the other -- and compare. The
# conditions that have to match are listed in docs/POWER.md and recorded in
# every log this writes, so a pair that did not in fact match can be caught
# afterwards instead of being believed.
#
#     ./battery-probe.sh --label with-app        # night one
#     ./battery-probe.sh --label without-app     # night two
#     ./battery-probe.sh --compare ~/postivene-power/with-app-*.log \
#                                  ~/postivene-power/without-app-*.log
#
# Nothing here is installed on the phone by the package: copy this one file
# across (`scp scripts/battery-probe.sh nemo@phone:`) and run it from a
# terminal or over ssh. It needs no root; with root (run it under
# `devel-su`) it also reads the kernel's wakeup counters, which say what
# kept the chip awake rather than only how much it cost.

set -eu

LABEL=""
HOURS=8
INTERVAL=300
OUTDIR="${HOME}/postivene-power"
COMPARE_A=""
COMPARE_B=""

usage() {
    cat <<'USAGE'
usage: battery-probe.sh --label NAME [--hours N] [--interval SECS] [--out DIR]
       battery-probe.sh --compare FIRST.log SECOND.log

  --label NAME      what this run is, e.g. with-app / without-app
  --hours N         how long to sample for (default 8)
  --interval SECS   seconds between samples (default 300)
  --out DIR         where logs go (default ~/postivene-power)
  --compare A B     print two finished runs side by side
USAGE
}

while [ $# -gt 0 ]; do
    case "$1" in
        --label) LABEL="${2:?--label needs a name}"; shift 2 ;;
        --hours) HOURS="${2:?--hours needs a number}"; shift 2 ;;
        --interval) INTERVAL="${2:?--interval needs a number}"; shift 2 ;;
        --out) OUTDIR="${2:?--out needs a directory}"; shift 2 ;;
        --compare)
            COMPARE_A="${2:?--compare needs two logs}"
            COMPARE_B="${3:?--compare needs two logs}"
            shift 3 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
done

# ---------------------------------------------------------------- reading

# The battery's sysfs node. Phones disagree about where it is, so the first
# one that has a charge or capacity reading wins.
find_battery() {
    for node in /sys/class/power_supply/battery \
                /sys/class/power_supply/bms \
                /sys/class/power_supply/qcom-battery \
                /sys/class/power_supply/*; do
        [ -d "$node" ] || continue
        case "$(cat "$node/type" 2>/dev/null || echo)" in
            Battery) ;;
            *) continue ;;
        esac
        if [ -r "$node/capacity" ] || [ -r "$node/charge_now" ]; then
            echo "$node"
            return 0
        fi
    done
    return 1
}

read_or() {
    # read_or FILE FALLBACK
    if [ -r "$1" ]; then cat "$1" 2>/dev/null || echo "$2"; else echo "$2"; fi
}

# Anything this big is microamps; a phone does not draw 100 A.
to_ua() {
    value="$1"
    [ "$value" = "" ] && { echo ""; return; }
    abs=${value#-}
    if [ "$abs" -lt 100000 ] 2>/dev/null; then
        echo $((value * 1000))   # reported in milliamps
    else
        echo "$value"
    fi
}

charger_online() {
    for node in /sys/class/power_supply/*/online; do
        [ -r "$node" ] || continue
        [ "$(cat "$node" 2>/dev/null || echo 0)" = "1" ] && { echo yes; return; }
    done
    echo no
}

# ------------------------------------------------------------- conditions

conditions() {
    echo "# --- conditions ---"
    echo "# host              $(uname -n)"
    echo "# kernel            $(uname -r)"
    echo "# release           $(. /etc/os-release 2>/dev/null && echo "${PRETTY_NAME:-unknown}")"
    echo "# battery node      $BATTERY"
    echo "# charger connected $(charger_online)"

    # connman is the phone's own account of its radios: what is powered and
    # what is actually connected. Both matter -- a powered-but-idle cellular
    # modem still scans.
    if command -v connmanctl >/dev/null 2>&1; then
        connmanctl technologies 2>/dev/null \
            | awk '/^\//{t=$0} /Type = /{ty=$3} /Powered = /{p=$3} /Connected = /{print "# technology       " ty " powered=" p " connected=" $3}'
        echo "# connman state     $(connmanctl state 2>/dev/null | awk '/State/{print $3}')"
        echo "# vpn connections   $(connmanctl vpnconnections 2>/dev/null | wc -l)"
    else
        echo "# technology       connmanctl not available"
    fi

    # A tunnel of any kind is a second always-on socket and changes the
    # answer, so it is recorded rather than assumed absent.
    if command -v ip >/dev/null 2>&1; then
        echo "# default route     $(ip route show default 2>/dev/null | head -n 1)"
        echo "# tunnel devices    $(ip -o link show 2>/dev/null | awk -F': ' '$2 ~ /^(tun|tap|wg|ppp)/{printf "%s ", $2} END{print ""}')"
    fi

    if command -v mcetool >/dev/null 2>&1; then
        echo "# display state     $(mcetool -N 2>/dev/null | head -n 1)"
        echo "# radio states      $(mcetool 2>/dev/null | awk -F: '/Radio states/{print $2}' | tr -d ' ')"
    else
        echo "# display state     mcetool not available (install mce-tools)"
    fi

    echo "# postivene running $(pgrep -f harbour-postivene >/dev/null 2>&1 && echo yes || echo no)"
    echo "# rpc server running $(pgrep -f deltachat-rpc-server >/dev/null 2>&1 && echo yes || echo no)"
    echo "# other harbour apps $(pgrep -l -f 'harbour-' 2>/dev/null | grep -v harbour-postivene | awk '{printf "%s ", $2} END{print ""}')"
    echo "# uptime            $(cut -d' ' -f1 /proc/uptime)"
    echo "# --- conditions end ---"
}

# Kernel wakeup counters, so a run can say what woke the chip and not only
# what it cost. Root only; a run without them is still a valid run.
wakeups() {
    for path in /sys/kernel/debug/wakeup_sources /sys/class/wakeup; do
        [ -r "$path" ] || continue
        if [ "$path" = /sys/kernel/debug/wakeup_sources ]; then
            awk 'NR>1 && $2+0 > 0 {print $1"\t"$2}' "$path" 2>/dev/null && return 0
        fi
    done
    return 1
}

suspend_success() {
    for path in /sys/power/suspend_stats/success \
                /sys/kernel/debug/suspend_stats; do
        [ -r "$path" ] || continue
        if [ -d "$path" ]; then
            read_or "$path/success" ""
        else
            cat "$path" 2>/dev/null
        fi
        return 0
    done
    echo ""
}

# ---------------------------------------------------------------- compare

summarise() {
    # summarise LOGFILE -- prints the numbers a run is worth reading for.
    awk -F'\t' '
        /^#/ { next }
        $1 == "t" { next }
        {
            if (first == "") { first = $1; cap0 = $2; chg0 = $3 }
            last = $1; cap1 = $2; chg1 = $3
            if ($4 != "") { cur += ($4 < 0 ? -$4 : $4); n++ }
        }
        END {
            if (first == "") { print "  no samples"; exit }
            hours = (last - first) / 3600.0
            if (hours <= 0) { print "  run too short"; exit }
            printf "  duration          %.2f h (%d samples)\n", hours, n
            printf "  capacity          %s%% -> %s%%  (%.2f %%/h)\n", cap0, cap1, (cap0 - cap1) / hours
            if (chg0 != "" && chg1 != "" && chg0 + 0 > 0)
                printf "  charge            %.0f -> %.0f uAh  (%.1f mAh/h)\n", chg0, chg1, (chg0 - chg1) / 1000.0 / hours
            if (n > 0)
                printf "  mean draw         %.1f mA (from current_now)\n", cur / n / 1000.0
        }
    ' "$1"
}

if [ -n "$COMPARE_A" ]; then
    for log in "$COMPARE_A" "$COMPARE_B"; do
        [ -r "$log" ] || { echo "cannot read $log" >&2; exit 1; }
        echo "=== $log"
        grep '^# ' "$log" | sed 's/^# /  /'
        summarise "$log"
        echo
    done
    echo "Conditions above must match line for line apart from the app itself."
    echo "A pair that differs in signal strength, radios, tunnels or other"
    echo "running apps is not a measurement of Postivene."
    exit 0
fi

# ------------------------------------------------------------------- run

[ -n "$LABEL" ] || { echo "--label is required" >&2; usage >&2; exit 2; }

BATTERY=$(find_battery) || { echo "no battery node under /sys/class/power_supply" >&2; exit 1; }

if [ "$(charger_online)" = "yes" ]; then
    echo "The charger is connected. A discharge measurement needs it unplugged." >&2
    exit 1
fi

mkdir -p "$OUTDIR"
LOG="$OUTDIR/${LABEL}-$(date +%Y%m%d-%H%M%S).log"

{
    echo "# label             $LABEL"
    echo "# started           $(date -Iseconds 2>/dev/null || date)"
    echo "# planned hours     $HOURS"
    echo "# sample interval   ${INTERVAL}s"
    conditions
    echo "# wakeup counters   $(wakeups >/dev/null 2>&1 && echo available || echo "not readable (run under devel-su for these)")"
    echo "# suspend successes at start: $(suspend_success)"
    printf 't\tcapacity\tcharge_uah\tcurrent_ua\tvoltage_uv\tuptime\n'
} > "$LOG"

if wakeups > "$LOG.wakeups.start" 2>/dev/null; then :; else rm -f "$LOG.wakeups.start"; fi

echo "Logging to $LOG"
echo "Leave the phone alone now: screen off, face down, do not pick it up."

END=$(( $(date +%s) + HOURS * 3600 ))
while [ "$(date +%s)" -lt "$END" ]; do
    now=$(date +%s)
    cap=$(read_or "$BATTERY/capacity" "")
    chg=$(read_or "$BATTERY/charge_now" "")
    cur=$(to_ua "$(read_or "$BATTERY/current_now" "")")
    vol=$(read_or "$BATTERY/voltage_now" "")
    up=$(cut -d' ' -f1 /proc/uptime)
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$now" "$cap" "$chg" "$cur" "$vol" "$up" >> "$LOG"

    # A charger plugged in mid-run ends the run: everything after it is not
    # a discharge measurement, and a half-good log invites a bad comparison.
    if [ "$(charger_online)" = "yes" ]; then
        echo "# stopped early: charger connected at $(date -Iseconds 2>/dev/null || date)" >> "$LOG"
        break
    fi
    sleep "$INTERVAL"
done

{
    echo "# finished          $(date -Iseconds 2>/dev/null || date)"
    echo "# suspend successes at end: $(suspend_success)"
    conditions
} >> "$LOG"

if wakeups > "$LOG.wakeups.end" 2>/dev/null; then :; else rm -f "$LOG.wakeups.end"; fi

if [ -r "$LOG.wakeups.start" ] && [ -r "$LOG.wakeups.end" ]; then
    echo "# --- wakeups gained over the run ---" >> "$LOG"
    awk -F'\t' 'NR==FNR{was[$1]=$2; next}
                {gained = $2 - (($1 in was) ? was[$1] : 0)
                 if (gained > 0) printf "# %-40s %d\n", $1, gained}' \
        "$LOG.wakeups.start" "$LOG.wakeups.end" | sort -k3 -rn >> "$LOG"
    rm -f "$LOG.wakeups.start" "$LOG.wakeups.end"
fi

echo
echo "=== $LABEL"
summarise "$LOG"
echo
echo "Log: $LOG"
echo "Run the other condition on another night, then:"
echo "  $0 --compare FIRST.log SECOND.log"
