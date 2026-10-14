#!/usr/bin/env bash
# Prove scripts/sonar-report.sh reports what it claims. Real SonarQube Cloud is
# unreachable here, so this runs against a stub server answering the four
# endpoints. Each case asserts one past bug. Script reads everything from the
# scanner's report-task.txt, so stub + hand-written task file is the harness.
set -u

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
script="$root/scripts/sonar-report.sh"
work=$(mktemp -d)

server_pid=""
# Inline, not cleanup fn: shellcheck calls trap body unreachable (SC2317).
trap 'if [[ -n "$server_pid" ]]; then kill "$server_pid" 2>/dev/null; wait "$server_pid" 2>/dev/null; fi; rm -rf "$work"' EXIT

if [[ ! -x "$script" ]]; then
    echo "selftest: FAIL $script is missing or not executable" >&2
    exit 1
fi
for tool in python3 jq curl; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "selftest: SKIP $tool is not installed" >&2
        exit 0
    fi
done

# ---- the stub
#
# Answers four endpoints, records paths, can demand credentials. Refuses with
# 404, not 403, as SonarQube Cloud does (script must retry through it).
cat >"$work/stub.py" <<'PY'
import json, os, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

STATUS = os.environ["STUB_TASK_STATUS"]
AUTH = os.environ["STUB_AUTH_REQUIRED"] == "1"
SEEN = os.environ["STUB_SEEN"]

GATE = {
    "projectStatus": {
        "status": "OK",
        "conditions": [
            {
                "status": "OK",
                "metricKey": "new_coverage",
                "comparator": "LT",
                "errorThreshold": "80",
                "actualValue": "91.4",
            },
            {
                "status": "ERROR",
                "metricKey": "new_violations",
                "comparator": "GT",
                "errorThreshold": "0",
                "actualValue": "2",
            },
        ],
    }
}

MEASURES = {
    "component": {
        "measures": [
            {"metric": "ncloc", "value": "14540"},
            {"metric": "coverage", "value": "73.1"},
            {"metric": "security_rating", "value": "1.0"},
            {"metric": "sqale_rating", "value": "2.0"},
            {"metric": "new_coverage", "period": {"value": "91.4"}},
            {"metric": "new_lines_to_cover", "periods": [{"index": 1, "value": "1"}]},
        ]
    }
}

ISSUES = {
    "total": 2,
    "issues": [
        {
            "severity": "MAJOR",
            "rule": "rust:S1234",
            "component": "muhnschein_piirit:rust/piirit-shim/src/lib.rs",
            "line": 42,
            "message": "Remove this unused import.",
        },
        {
            "rule": "shell:S5678",
            "component": "muhnschein_piirit:ci/qml-lint.sh",
            "impacts": [{"severity": "LOW"}],
            "message": "Quote this expansion.",
        },
    ],
}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_GET(self):
        with open(SEEN, "a", encoding="utf-8") as seen:
            seen.write(self.path + "\n")

        if AUTH and not self.headers.get("Authorization"):
            self.send(404, {"errors": [{"msg": "Component key not found"}]})
            return

        if self.path.startswith("/api/ce/task"):
            self.send(200, {"task": {"status": STATUS, "analysisId": "AN1"}})
        elif self.path.startswith("/api/qualitygates/project_status"):
            self.send(200, GATE)
        elif self.path.startswith("/api/measures/component"):
            self.send(200, MEASURES)
        elif self.path.startswith("/api/issues/search"):
            self.send(200, ISSUES)
        else:
            self.send(404, {"errors": [{"msg": "no such endpoint"}]})

    def send(self, code, payload):
        body = json.dumps(payload).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


httpd = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
with open(os.environ["STUB_PORT_FILE"], "w", encoding="utf-8") as handle:
    handle.write(str(httpd.server_port))
sys.stderr.write("stub listening on %d\n" % httpd.server_port)
httpd.serve_forever()
PY

status=0
cases=0

# start_stub <task status> <auth required>
start_stub() {
    if [[ -n "$server_pid" ]]; then
        kill "$server_pid" 2>/dev/null
        wait "$server_pid" 2>/dev/null
        server_pid=""
    fi
    rm -f "$work/port" "$work/seen"
    : >"$work/seen"

    STUB_TASK_STATUS=$1 \
    STUB_AUTH_REQUIRED=$2 \
    STUB_PORT_FILE="$work/port" \
    STUB_SEEN="$work/seen" \
        python3 "$work/stub.py" 2>/dev/null &
    server_pid=$!

    local waited=0
    while [[ ! -s "$work/port" ]]; do
        sleep 0.1
        waited=$((waited + 1))
        if [[ "$waited" -ge 100 ]]; then
            echo "selftest: FAIL the stub server never came up" >&2
            return 1
        fi
    done
}

# write_task <scope query, may be empty>
write_task() {
    local scope=$1
    local port
    port=$(cat "$work/port")
    local dash="http://127.0.0.1:$port/dashboard?id=muhnschein_piirit"
    if [[ -n "$scope" ]]; then
        dash="$dash&$scope"
    fi
    cat >"$work/report-task.txt" <<EOF
projectKey=muhnschein_piirit
serverUrl=http://127.0.0.1:$port
serverVersion=8.0
dashboardUrl=$dash
ceTaskId=TASK1
ceTaskUrl=http://127.0.0.1:$port/api/ce/task?id=TASK1
EOF
    return 0
}

# expect <description> <text that must appear> [file to search]
expect() {
    local what=$1 needle=$2 where=$3
    cases=$((cases + 1))
    # `--`: a needle starting with a dash would be a grep option.
    if grep -qF -- "$needle" "$where"; then
        echo "selftest: ok   $what"
    else
        echo "selftest: FAIL $what -- no '$needle' in $where" >&2
        sed -n '1,60p' "$where" >&2
        status=1
    fi
    return 0
}

# reject <description> <text that must NOT appear> <file>
reject() {
    local what=$1 needle=$2 where=$3
    cases=$((cases + 1))
    if grep -qF -- "$needle" "$where"; then
        echo "selftest: FAIL $what -- found '$needle' in $where" >&2
        status=1
    else
        echo "selftest: ok   $what"
    fi
    return 0
}

# ---- everything, anonymous
#
# Public project, no token. Also catches jq bug (`|` binds looser than `,`;
# died on "Cannot index string with string"): heading + condition line present
# = fixed.
start_stub SUCCESS 0 || exit 1
write_task ""
env -u SONAR_TOKEN -u GITHUB_STEP_SUMMARY \
    "$script" "$work/report-task.txt" >"$work/out" 2>"$work/err"
rc=$?

cases=$((cases + 1))
if [[ "$rc" -eq 0 ]]; then
    echo "selftest: ok   a complete analysis is reported"
else
    echo "selftest: FAIL a complete analysis should exit 0, got $rc" >&2
    cat "$work/err" >&2
    status=1
fi

expect "the quality gate is rendered" "### Quality gate: OK" "$work/out"
expect "a passing condition is listed" "OK  new_coverage LT 80 (actual: 91.4)" "$work/out"
expect "a failing condition is listed" "ERROR  new_violations GT 0 (actual: 2)" "$work/out"
expect "measures are rendered" "- coverage: 73.1" "$work/out"
expect "a new-code measure reads its period value" "- new_coverage: 91.4" "$work/out"
# Cloud answers `periods` (array), Server answers `period`. First real run
# printed "-" for every new_* measure.
expect "and one in Cloud's periods shape too" "- new_lines_to_cover: 1" "$work/out"
expect "issues are counted" "### Open issues: 2" "$work/out"
expect "an issue names its file and line" "rust/piirit-shim/src/lib.rs:42" "$work/out"
expect "an issue with only impacts still has a severity" "LOW  shell:S5678" "$work/out"
expect "the dashboard link is printed" "/dashboard?id=muhnschein_piirit" "$work/out"

# Real run had 115 issues; page cap 100 gave "15 more not listed".
expect "the issue list asks for a whole page" "resolved=false&ps=500" "$work/seen"

# Ratings are 1..5 on the wire: unreadable as numbers, and look like a score out
# of five with polarity reversed.
expect "a rating is a letter" "- security_rating: A" "$work/out"
expect "the other rating is a letter too" "- sqale_rating: B" "$work/out"
reject "no raw rating survives" "security_rating: 1.0" "$work/out"

# ---- the scope is passed
#
# Slice from dashboard URL must reach measures and issues queries, or they
# answer for the default branch (wrong, but plausible-looking).
start_stub SUCCESS 0 || exit 1
write_task "pullRequest=45"
env -u SONAR_TOKEN -u GITHUB_STEP_SUMMARY \
    "$script" "$work/report-task.txt" >"$work/out" 2>"$work/err"

expect "the scope is named in the report" "pullRequest=45" "$work/out"
expect "the measures query carries the scope" \
    "/api/measures/component?component=muhnschein_piirit&pullRequest=45&" \
    "$work/seen"
expect "the issues query carries the scope" \
    "/api/issues/search?componentKeys=muhnschein_piirit&pullRequest=45&" \
    "$work/seen"

# ---- the token first
#
# Cloud answers 404, not 403, for unreadable things; asking anonymously first
# read that 404 as "no such thing" and reported nothing.
start_stub SUCCESS 1 || exit 1
write_task ""
SONAR_TOKEN=squ_stub \
    env -u GITHUB_STEP_SUMMARY \
    "$script" "$work/report-task.txt" >"$work/out" 2>"$work/err"

expect "a project that needs the token is still reported" \
    "### Quality gate: OK" "$work/out"
expect "and its measures with it" "- coverage: 73.1" "$work/out"

# No token: fail loudly, not an empty report that looks clean.
start_stub SUCCESS 1 || exit 1
write_task ""
env -u SONAR_TOKEN -u GITHUB_STEP_SUMMARY \
    "$script" "$work/report-task.txt" >"$work/out" 2>"$work/err"
rc=$?

cases=$((cases + 1))
if [[ "$rc" -ne 0 ]]; then
    echo "selftest: ok   an unreadable project fails instead of reporting nothing"
else
    echo "selftest: FAIL an unreadable project should not exit 0" >&2
    status=1
fi
reject "and prints no gate" "Quality gate" "$work/out"

# ---- the step summary
#
# Log scrolls; step summary is what reviewer opens. Same text in both.
start_stub SUCCESS 0 || exit 1
write_task ""
GITHUB_STEP_SUMMARY="$work/summary" \
    env -u SONAR_TOKEN \
    "$script" "$work/report-task.txt" >"$work/out" 2>"$work/err"

expect "the step summary gets the report" "### Quality gate: OK" "$work/summary"
expect "the step summary gets the measures" "- coverage: 73.1" "$work/summary"

# ---- what went wrong
#
# Report server never finished processing must not be rendered: early measures
# are previous analysis's numbers, which look right.
start_stub FAILED 0 || exit 1
write_task ""
env -u SONAR_TOKEN -u GITHUB_STEP_SUMMARY \
    "$script" "$work/report-task.txt" >"$work/out" 2>"$work/err"
rc=$?

cases=$((cases + 1))
if [[ "$rc" -ne 0 ]]; then
    echo "selftest: ok   a failed compute task is not reported as an analysis"
else
    echo "selftest: FAIL a failed compute task should not exit 0" >&2
    status=1
fi
expect "and says so" "status: FAILED" "$work/err"
reject "and prints no measures" "### Measures" "$work/out"

# No upload = nothing to report; step is continue-on-error, so failing here
# warns.
cases=$((cases + 1))
if "$script" "$work/nothing-here.txt" >"$work/out" 2>"$work/err"; then
    echo "selftest: FAIL a missing report-task.txt should not exit 0" >&2
    status=1
else
    echo "selftest: ok   a missing report-task.txt is refused"
fi
expect "and names the file" "nothing-here.txt" "$work/err"

echo "selftest: $cases case(s)"
if [[ "$status" -ne 0 ]]; then
    echo "selftest: FAILED" >&2
fi
exit "$status"
