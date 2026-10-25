# AGENTS.md

How to work on Piirit. What it is: `README.md`, `docs/`. Read `docs/BUILDING.md` before first change. User's words in chat beat this file.

## Ground rules

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
7. With RPM: outline briefly what merging this PR risks and how you mitigated it.

Feature reachable from several places (chat, contact/group page, cover, quick actions) → all of them.

User tests RPM on phone; that test judges feel, lag, transitions. Say exactly what to check on device.

## Local checks

- `git submodule update --init --depth 1` before first build (FFmpeg, x264). x264 comes from `code.videolan.org`; sandbox must allow it, else `piirit-shim` (all QML tests) doesn't build.
- Missing optional tool = SKIP locally, FAIL in CI. Say which steps skipped.
- Local failure → rerun on untouched `main` before calling it environment.
- `make check` gates catch device-only breakage (Rust 1.75, Cargo.lock v3, Qt 5.6 QML, shaders, lints, spec, Harbour); reasons in BUILDING.md. Gate red → fix code, never loosen gate.
- Tests wait on signals or conditions, not fixed sleeps or wall-clock asserts (those flake on loaded runners).
- New dependency must earn its keep: well maintained, widely used, more than someone's pet project, builds at Rust 1.75. Add row to BUILDING.md dependency table saying why.

## UI and words

- Silica stock components, built like existing pages (e.g. chat's `PageHeader`). Jolla app has same feature (Gallery, Camera) → match it. Delta Chat apps = reference for features.
- "Design first" / "no PR yet" → mockup on canvas, wait for approval, then code.
- Every `qsTr()` string: PROJECT.md "The words on screen". Changed strings → `scripts/update-translations.sh`, fill all catalogs.
- QML tests stub Silica, no layout: looks unproven until device test. Say so.
- Phone = battery: no always-running animations, timers, polling; stop work when app is in background.

## Comments and docs

- Caveman-full = drop articles, filler, pleasantries, hedging. Fragments OK, short words. Identifiers, paths, code, errors exact. E.g. "`rust/Cargo.lock` stays v3: SDK cargo 1.75 can't read v4."
- Code comments, `docs/*.md`, this file: caveman-full, rules in BUILDING.md "Comments".
- `README.md`, `CHANGELOG.md`, release notes, forum posts: plain prose for users, as short and to the point as possible.
- User-visible change → line in `CHANGELOG.md` under `## Unreleased` (create if missing), same PR.
- No plans, research notes, scratch files in repo.

## Git and PRs

- One branch, one PR per task. Stacked PR only when asked.
- Unrelated finding → tell user; separate PR or issue, not this PR.
- Commit and PR titles: Conventional Commits, plain language, say outcome: `fix(cover): faces no longer turn into white squares`. Types `feat` `fix` `perf` `refactor` `test` `docs` `ci` `build` `chore`; scope = app area or subsystem (`chat`, `calls`, `rpm`, …). Body says why, plain prose; brief, to the point.
- PR body: the problem in a sentence or two, then how you fixed it. End with the model and harness that did the work.
- Final diff: no debug logging, no leftovers.
- Flaky test → find and fix cause. Never add retries (nextest runs none by design: pass on second try = defect). Never re-run CI until green and call it done. Never skip test or loosen assert. One re-run OK only for infra failure before any test ran (runner lost, apt mirror timeout); say so.
- Merge only on user's explicit go. Merge commit, not squash/rebase.

## Releases

- Only on user's go. Latest `main` commit green first, Sonar included: check main, not just PR.
- Strict semver: nothing breaking → minor.
- Steps: BUILDING.md "Cutting a release". Harbour upload stays with user.

## Subagents

- Delegate only with tight, written scope.
- Models: only ones on cost/intelligence Pareto frontier ([Artificial Analysis](https://artificialanalysis.ai/), [Slopalytics](https://slopalytics.com/)). Dominated model = don't use. Cheapest frontier model that does the job well. If user names model for task, use that.
- Cheap-model output gets strong-model review: cheap models dropped facts, inverted conditions.

## Reporting

- Replies to user: caveman-full (see Comments and docs).
- Lead with result. Say what was verified, what wasn't, what is guess.
- Non-green step → name it and why. Never hide.
- Forum bug report → search GitHub issues first, link match.
