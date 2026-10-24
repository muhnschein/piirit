# AGENTS.md

How to work on Piirit. Facts live in `docs/`; this file = workflow + pointers. Read `docs/PROJECT.md` (scope) and `docs/BUILDING.md` (standards) before first change. User's words in chat beat this file.

## Ground rules

- Piirit = Silica UI, platform glue, packaging on `deltachat-rpc-server` (JSON-RPC over stdio). No protocol, crypto, mail, storage code; no hand-written C FFI. Core lacks API → say so, don't build around it.
- Harbour (Jolla Store) only. No OpenRepos/Chum, nothing needing dev mode. Platform gap that blocks → report it, propose raising with Jolla; no workaround.
- One target: SFOS 5.2 SDK, aarch64, Jolla Phone 2026. No effort for older releases.
- Don't know how to proceed → say "I don't know", stop, ask. Mark guesses as guesses. Unknown beats confident wrong.
- Bugs: find root cause, state it. Symptom fix ≠ fix.
- Good return on cost. No gold-plating, no unasked refactors.
- Options → lay them out, recommend one with reasons. Implement nothing until user clearly picks.

## Done means

In this order; later step only once earlier ones hold:

1. Change implemented.
2. Tests added for it (regression test per bug).
3. Tests proven to bite: each fails with change reverted or mutated. Report how (e.g. "22/22 mutations caught"); name what headless tests can't prove.
4. `make check` green locally.
5. PR CI green, SonarQube included (quality gate, new-code coverage).
6. Device RPM: dispatch `.github/workflows/rpm.yml` on PR branch, give user the artifact link.

Feature reachable from several places (chat, contact/group page, cover, quick actions) → all of them.

User tests RPM on phone; that test judges feel, lag, transitions. Say exactly what to check on device.

## Commands

```sh
git submodule update --init --depth 1   # FFmpeg, x264; before any build
make check                  # all CI gates; run before every push
make test                   # tests; nextest if installed (cargo install --locked cargo-nextest)
make lint fmt msrv          # clippy -D warnings, rustfmt --check, build at Rust 1.75
scripts/update-translations.sh   # after any qsTr() change
make translations           # .qm catalogs for local runs
```

- Missing optional tool = SKIP locally, FAIL in CI. Say which steps skipped.
- Local failure → rerun on untouched `main` before calling it environment. Known: `qml_return_place` fails in some containers, green in CI.
- Tests wait on signals or conditions, not fixed sleeps or wall-clock asserts (those flake on loaded runners).
- x264 submodule comes from `code.videolan.org`; sandbox must allow it. Without it `piirit-shim` (all QML tests) doesn't build.

## Where things are

- `rust/piirit-shim`: JSON-RPC client, Qt models; tests in `rust/piirit-shim/tests`
- `rust/piirit-app`: binary. `rust/piirit-video`: FFmpeg/x264 build, recode. `rust/deltachat-jsonrpc`: core types
- `qml/`: pages, components, cover. `tests/silica-stubs/`: Silica stand-ins for QML tests
- `ci/`: gate scripts. `scripts/`: build, release, translations. `rpm/harbour-piirit.spec`
- Docs: `docs/PROJECT.md` scope, non-goals, string rules; `docs/BUILDING.md` toolchain, lints, test layers, CI, packaging, release; `docs/HARBOUR.md` store rules; `docs/VERIFYING.md` reproducible builds

## Constraints that bite

Details: `docs/BUILDING.md`.

- Rust floor 1.75 on device; nothing newer from std or syntax. `rust-toolchain.toml` pins lint toolchain only.
- `rust/Cargo.lock` stays v3; modern `cargo update` rewrites to v4 silently.
- Qt 5.6 on device; host Qt 5.15 accepts more. `tests/qml_syntax.rs` holds 5.6 rules.
- No `ShaderEffect`, `layer`, QtGraphicalEffects: Qt 5.6 render loop mixes up shader programs. Guard tests fail.
- Denied: `unwrap_used`/`expect_used` outside tests, `missing_docs`, `unsafe_code`, clippy pedantic. `unsafe`/`cpp!` only in the four files BUILDING.md names.
- `rust/clippy.toml` bans `Runtime::new` (use `CoreRuntime`) and `qmetaobject::single_shot`.
- Spec: no bare `%` in comments; Harbour limits name, paths, `Requires:` (`ci/harbour-check.sh`).
- New crate: rare; add row to BUILDING.md dependency table.

## UI and words

- Silica stock components, built like existing pages (e.g. chat's `PageHeader`). Jolla app has same feature (Gallery, Camera) → match it. Delta Chat apps = reference for features.
- "Design first" / "no PR yet" → mockup on canvas, wait for approval, then code.
- Every `qsTr()` string follows PROJECT.md "The words on screen": one plain finished sentence, literal verbs, Delta Chat's own string verbatim where one exists. New or changed source string → fill all catalogs; `tests/translation_catalogs.rs` fails otherwise.
- QML tests stub Silica, no layout: looks unproven until device test. Say so.
- Phone = battery: no always-running animations, timers, polling; stop work when app is in background.

## Comments and docs

- Code comments, `docs/*.md`, this file: caveman-full. Terse fragments; what is true now + why. No history, story, changelog (git's job). Identifiers, paths exact.
- `README.md`, `CHANGELOG.md`, release notes, forum posts: plain prose for users.
- User-visible change → line in `CHANGELOG.md` under `## Unreleased` (create if missing), same PR.
- No plans, research notes, scratch files in repo.

## Git and PRs

- One branch, one PR per task. Stacked PR only when asked.
- Unrelated finding → tell user; separate PR or issue, not this PR.
- Commit: imperative subject; body says why, plain prose.
- Dependabot: fold bumps into own branch (bot PRs get no Sonar token); don't merge bot PRs.
- Main moved and conflicts → merge `main` in, rerun checks.
- Final diff: no debug logging, no leftovers.
- Flaky test → fix cause. Never retry, skip, loosen to green.
- Merge only on user's explicit go. Merge commit, not squash/rebase.

## Releases

- Only on user's go. Latest `main` commit green first, Sonar included: check main, not just PR.
- Strict semver: nothing breaking → minor.
- Steps: BUILDING.md "Cutting a release". Harbour upload stays with user.

## Subagents

- Delegate only with tight, written scope.
- Models: only ones on cost/intelligence Pareto frontier (Artificial Analysis, Slopalytics). Dominated model = don't use. Cheapest frontier model that does the job well. User names model for task → use that.
- Cheap-model output gets strong-model review: cheap models dropped facts, inverted conditions.

## Reporting

- Replies to user: caveman-full. User-facing text (changelog, release notes, forum) stays prose.
- Lead with result. Say what was verified, what wasn't, what is guess.
- Non-green step → name it and why. Never hide.
- Forum bug report → search GitHub issues first, link match.
