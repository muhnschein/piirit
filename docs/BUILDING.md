# Building, testing, and packaging

Engineering standards and the build procedure. The reference for the former
is [clove](https://github.com/muhnschein/clove)'s §9: pinned toolchain,
pedantic lints, CI-parity `make` targets, tests that drive the real
binaries.

## Toolchains

`rust-toolchain.toml` pins 1.94.1 so lint results are reproducible; the
device floor is **1.75.0**, enforced by CI's `msrv` job with warnings
denied. It is a rustup mechanism, and the Sailfish SDK's cargo ignores it.

`rust/Cargo.lock` stays at **v3**: cargo learned v4 in 1.78 and the SDK's
cargo 1.75 cannot read it, while a `cargo update` on a modern host rewrites
it silently. `ci/check-lockfile.sh` catches that.

## Lints

Workspace-level, so a bare `cargo clippy` fails the way CI does:
`clippy::all` and `pedantic` at deny, `unwrap_used`/`expect_used` denied
outside tests, `missing_docs` and `unsafe_code` denied.

`unsafe_code` is deny rather than forbid because the Qt harness tests need
`env::set_var` before Qt initialises, and because two things the app does
have no safe binding: installing a `QTranslator`, and recording a voice
message through `QAudioRecorder`, neither of which qmetaobject wraps.
Those are the two `cpp!` files in the tree, `piirit-app/src/translations.rs`
and `piirit-shim/src/recorder.rs`, and the C++ build step in each
crate's `build.rs` exists for them alone. Every exception is at the
narrowest scope and says why, and every block is short enough to be
checked by reading.

The recorder is also why the host build needs `qtmultimedia5-dev` (the
`Makefile` lists the packages) and the spec `pkgconfig(Qt5Multimedia)`:
the shim links `libQt5Multimedia`, which Harbour allows.

`rust/clippy.toml` bans two methods that have already caused device-only
failures: `tokio::runtime::Runtime::new` (must go through `CoreRuntime`) and
`qmetaobject::single_shot` (truncates sub-second `Duration`s).

## Testing

Piirit parses almost nothing — protocol and crypto are the core's, and
the subprocess we talk to is one we spawned. The failure mode is misreading
the core's JSON, or calling it wrongly, with nothing noticing until the app
is on a phone. The tests aim at that.

`make check` runs all of it from a clean checkout: no phone, account or
Sailfish SDK. Not quite offline — `msrv` fetches the 1.75 toolchain the
first time, and `deny` wants the advisory database.

**How the suite is run.** Through `cargo nextest`, which puts each test in
its own process rather than running one test binary at a time. That matters
here more than in most workspaces: there are over a hundred test binaries,
almost every one starts a Qt engine and then waits on real timers, so under
`cargo test` the suite spent about ten minutes mostly idle and serialised.
The same 207 tests take about two and a half minutes on four cores. Nothing
is shared between them — the webxdc host binds port 0 and lets the kernel
choose, and the QML probes copy the tree into a directory named for their
own pid — which is what makes running them at once safe.

```
cargo install --locked cargo-nextest
```

Without it `make test` still runs everything, the slow way, because
`make check` has to work on a laptop that has installed nothing extra.
`rust/.config/nextest.toml` carries the rest: a 30-second slow warning and
a two-minute kill, so a Qt test waiting on a signal that never arrives is
reported against its own name instead of hanging until the job's timeout;
and no retries, because a test that passes on the second attempt is a
defect this project wants to see.

nextest does not run doctests. `cargo test --doc` runs beside it in both
the Makefile and `ci.yml`, and `ci/packaging-lint.sh` fails a tree where
one is there without the other — there are no doctests today, so nothing
would notice the first one silently never running.

1. **Transport unit tests** against a fake stdio server.
2. **Protocol-contract tests** against a recording double that journals
   every request, pinning the call sequence of each onboarding action.
3. **Qt event-loop tests** under `QT_QPA_PLATFORM=offscreen`.
4. **QML load tests** against stub Silica components (`tests/silica-stubs/`):
   the real page files, driven by `objectName`. The stubs imitate no layout,
   so nothing here says a page *looks* right. Silica's `EnterKey` attached
   property cannot be stubbed — QML forbids capitalised property names and
   `qmetaobject` cannot register attached types — so pages using it cannot
   be loaded. Put what such a page shows in a component that can be, and lay
   that component out with bindings rather than a `Column`: a positioner
   sizes itself in a polish pass, which never runs without a window, so its
   geometry reads as zero.
5. **Static QML tests** (`tests/qml_syntax.rs`) for what no host-Qt run
   can see: Qt 5.6 rules that Qt 5.15 accepts silently, and the rules the
   tree holds itself to -- every string the other end chose is drawn as
   plain text (Silica's own headers cannot be, so `ConversationHeader`
   exists), file URLs are encoded a segment at a time, only the picker
   pages import `Sailfish.Pickers`, and every `model.<role>` a delegate
   binds is one its model has.
6. **Real-core integration** (`real_server`, `real_core`), gated on
   `DELTACHAT_RPC_SERVER`, offline. `real_core.rs` distinguishes a request
   the real core could not decode from one it could not deliver.
7. **Packaging checks** (`ci/packaging-lint.sh`): spec parses, desktop
   entry validates, shell scripts clean, every translation catalog
   current and compiling cleanly with `lrelease`, every `docs/*.md` a
   comment points at exists. Locally a missing tool is
   a SKIP; CI sets `PACKAGING_LINT_STRICT=1` so it is a failure there, as
   `HARBOUR_CHECK_STRICT=1` already does for the Harbour check.

8. **Report-script tests** (`ci/sonar-report-selftest.sh`): the one script
   here that talks to a service outside GitHub, run against a stub server
   that answers SonarQube's four endpoints. It cannot be tested any other
   way -- the analysis it reads does not exist when the tests run, and this
   project's CI cannot reach `sonarcloud.io`, which is the reason the script
   exists at all.

9. **Runner-setup tests** (`ci/apt-install-selftest.sh`): the rule deciding
   which apt sources every CI job keeps, proved on a directory of the
   test's own. Getting it backwards deletes the archive the jobs install
   from, which fails everything.

Aspiration, tracked not gated: test volume exceeds source volume.

## Static analysis

SonarQube Cloud reads the tree on every pull request
(`.github/workflows/build.yml`, configured by `sonar-project.properties`).
It is a **report, not a gate**: `ci.yml` decides what is allowed in, and
nothing Sonar says can turn a red build green or a green build red. Keeping
that boundary is why it is a separate workflow -- folding it into the gate
would make a hosted service part of the rule that a green `make check` on a
laptop is a green CI.

The scanner **imports** coverage; it does not measure it. `make
sonar-reports` writes `rust/target/sonar/lcov.info` with `cargo llvm-cov`
over the whole workspace, and the workflow runs it before the scan. Without
it the reading is a confident 0.0% rather than "no data". The target needs
`cargo-llvm-cov`, so it is opt-in rather than part of `make check`. It runs
the suite under `cargo-nextest` when that is installed too, for the reason
`make test` does: cargo-llvm-cov's own runner is `cargo test`, one binary at
a time, which instrumented costs the scan job about ten minutes. Without
nextest the report is still written, the slow way.

```
rustup component add llvm-tools-preview
cargo install --locked cargo-llvm-cov cargo-nextest
make sonar-reports
```

Clippy findings are **not** handed over, and Sonar's own Clippy pass is off
(`sonar.rust.clippy.enabled=false`), for two different reasons. Sonar's pass
invokes cargo where it finds the project, and this workspace is under
`rust/`, not at the root, so it would run a different clippy from the one
that gates this project -- or none. And a report of our own would be empty:
`make lint` denies warnings, so a warning in this project's code fails the
gate and never reaches a branch Sonar analyses, leaving only findings in
`third_party/qmetaobject`, which is excluded anyway. Producing one costs a
`cargo clean` and a full recompile inside the scan job.

`sonar.tests` separates the fixtures from the application, so coverage and
duplication are measured on what ships. That matters more here than in most
trees, because test volume exceeds source volume: indexed as main sources,
the fixtures would be most of what every ratio was computed over.
`sonar.exclusions` drops the vendored crates, the patched
qmetaobject, the rendered icons, and `translations/` -- a Qt catalog is
named `.ts`, so the scanner reads thirty-nine of them as TypeScript.
`sonar.coverage.exclusions` keeps `qml/` out of the coverage arithmetic
alone: nothing there can produce a report, and one changed line of QML
JavaScript otherwise reads as 0% coverage on new code and fails the gate.
`sonar.issue.ignore.multicriteria` closes the findings the project has
decided against, one rule on one path each, with the reason beside it:
qmetaobject's glob import, cognitive complexity in tests and in the fake
servers, and a character-class name Sonar took for a repeated literal.

The scanner uploads a report and exits; the server processes it afterwards,
so the run that produced an analysis finishes knowing nothing about its
result. `scripts/sonar-report.sh` asks the server from the runner that just
fed it and prints the quality gate, the measures and the open issues into
the job log and the step summary. It reports and never gates: the step is
`continue-on-error`, so a Sonar outage costs a warning, not a build.

## CI

`ci.yml` is the gate and runs what `make check` runs. Three things about
the runners are worth knowing.

**Packages come through `ci/apt-install.sh`**, not a bare `apt-get`.
`apt-get update` exits non-zero when *any* configured repository fails, and
the runner image ships several this project never installs from -- so one
of them serving a bad index kills every job before a test runs, with
nothing in this repository having changed. The script drops the
third-party lists first, keeping Ubuntu's wherever the image puts them --
a list survives only if something in it names an `ubuntu.com` host, which
is what stops it deleting the archive it is about to install from.

**The Rust jobs cache their `target/`** (`Swatinem/rust-cache`, scoped to
the `rust` workspace); without it every job compiles the whole dependency
graph from nothing on every push. `msrv` carries a cache key of its own
because it builds with `+1.75.0` while the action keys on the default
toolchain, and without it the two would share a slot and neither would
ever hit. `CARGO_INCREMENTAL: 0` because a runner compiles once and throws
the machine away, so incremental state is written, cached and never read.

Caching is worth less than it looks: with a warm cache clippy compiles the
workspace in about twenty seconds, but the `test` job barely moves, because
compilation is not its cost -- most of its time is the suite waiting on
timers, which is what nextest addresses above.

**The test job installs `cargo-nextest`** and runs the suite under the
`ci` profile, which differs from a laptop's in two ways: `fail-fast` is
off, because CI is asked once and should report everything it knows; and
failures are printed where they happen and again at the end, because in a
two-hundred-line log the summary is what anyone reads.

## Translations

The strings are the `qsTr()` calls in `qml/`; `translations/piirit.ts`
is the untranslated source catalog and `translations/piirit-<lang>.ts`
one catalog per language Sailfish ships in. `scripts/update-translations.sh`
regenerates all of them from the source in one `lupdate` run, so a new
string turns up as `unfinished` in every language at once, and
`ci/packaging-lint.sh` fails when a committed catalog differs from what
that run produces. `tests/translation_catalogs.rs` fails when a string in
any language is left untranslated, so a new string is not done until every
catalog has it.

The app loads `piirit-<lang>.qm`, which `scripts/release-translations.sh`
compiles with `lrelease` -- in the RPM's `%build`, and locally with
`make translations`, which leaves them beside the `.ts` files where a
source-tree run finds them. `lupdate` and `lrelease` are Debian's
`qttools5-dev-tools`, and the SDK's `qt5-qttools-linguist`; the app's own
test compiles the German catalog, so the package is a test dependency too.

`<lang>` is what `QTranslator` matches against the reader's locale from
the most specific form down: `de` serves every German locale, `pt_BR`
only Brazil, and a language with no catalog gets the English one -- the
strings are English already, and that catalog holds their plural forms.
To add one, write the three-line header `update-translations.sh` documents to
`translations/piirit-<lang>.ts` and run the script; `lupdate` fills in
every string with as many plural forms as that language has.

## Dependencies

Few, and each for a reason. `cargo tree` on the app is the list; this is
why each entry is there, so that a proposal to drop one starts from what
it would cost.

| Crate | What it is for | Why it stays |
|---|---|---|
| `tokio` | the server subprocess, its pipes, the event loop | the transport is async; `process` is what reaps the child |
| `serde`, `serde_json` | the JSON on the wire | the contract with the core is JSON-RPC |
| `qmetaobject` (vendored), `qttypes`, `cpp`, `cpp_build` | Qt from Rust | the whole UI hangs off them; `default-features = false` keeps its `log` bridge out |
| `chrono` | the viewer's timezone, for the day headings | `std` has none, and the alternative is `localtime_r`, which `unsafe_code` denies |
| `qrcode` | an invite drawn as a code | one crate, no dependencies |
| `rqrr` (+ `g2p`, `lru`) | a code read off the camera | a QR decoder is not a small thing to vendor |
| `mp3lame-encoder` (+ `mp3lame-sys`, `autotools`) | a voice message as MP3 (`voice.rs`) | the phone's recorder offers no encoder the iOS client plays as a voice message; LAME is what the desktop client encodes with, and the pure-Rust encoders want a Rust past the 1.75 floor or are ports of shine, which has no psychoacoustic model |

`mp3lame-sys` carries LAME 3.100's C source and builds it with LAME's own
`configure` and `make`, so a host build needs both, which a machine that
builds the C++ above already has. It is the one C dependency in the tree
that is not Qt, and the reason `rust/deny.toml` allows LGPL-3.0 for its
two crates and nothing else: LGPL code may be conveyed as part of a GPLv3
work.

`tokio`'s `net` feature is what the webxdc host binds its loopback socket
with, and it brings `socket2` -- tokio's own platform layer for sockets,
and the only crate the whole feature adds. The alternative was a zip
reader and an inflate implementation, to unpack an app the core can
already read.

## Comments

One sentence where one will do. A comment states what is true now and why.
It is not a changelog, a bug report, or a story about how the code got here
— that belongs in git history. Delete a comment rather than update it into a
history of its own subject.

## Packaging: the supported path

`.github/workflows/rpm.yml` builds a device RPM unattended on an
`ubuntu-latest` runner, from a `docker run` of the Sailfish SDK. Dispatch
it from the Actions tab (the SDK version and cargo's job count are inputs)
or push a `v*` tag. It builds **aarch64**, which is the only architecture
this project targets and the only one it has ever built.

```sh
./scripts/fetch-rpc-server.sh                        # bundled server binaries
mb2 -t SailfishOS-<ver>-<arch> -X build-init
mb2 -t SailfishOS-<ver>-<arch> -X build --no-check
```

- `-X` (`--no-fix-version`) uses the spec's `Version:` rather than deriving
  one from git tags. It is needed **by `build-init`** too: without it that
  step gives up at version-fixing and never writes `.mb2/spec`, so `build`
  fails identically and the flag looks innocent.
- `build-init` must precede `build`, which queries `.mb2/spec` within a
  second of starting.
- `--no-check`: the tests are host-oriented, and the spec has no `%check`.

Environment requirements, each of which cost an attempt:

- **Mount the tree inside the SDK user's home** (`/home/mersdk/<name>`), not
  `/build` or `/share`. rpm runs under scratchbox2, which redirects
  unrecognised absolute paths into the target rootfs; with the tree
  elsewhere `mb2` writes `.mb2/spec` outside and rpm reads inside. A file
  that exists and cannot be opened is the signature. The directory must keep
  the package's name — `mb2` derives the package from it.
- **The i686 rustlib at the SDK's own `/usr/lib/rustlib`.** `mb2` installs
  rust into the *target*, but build-script links run in sb2's host mode
  where `/usr` maps to the SDK filesystem. Copy it from the tooling.
  `ci/build-sdk-image.sh` does this once, into the image, so a build no
  longer carries it; a build against upstream's image still has to.
- **Not root.** `sdk-manage` refuses ("Cannot determine Mer SDK user") and
  the target snapshot never initialises. Chown the checkout to the
  container's `mersdk` uid — read it from the image, don't assume it — and
  hand it back so the artifact upload can read the result.

`scripts/build-rpm.sh` wraps the ordinary developer path, `sfdk build`.

## Cutting a release

A release is `rpm.yml` run for a version, on `main`:

1. Put the version in `rpm/harbour-piirit.spec` (`Version:`) and in the
   three crates' `Cargo.toml`, refresh `Cargo.lock` (`cargo check`), and
   write the version's section in `CHANGELOG.md`. Merge that.
2. Either dispatch `rpm.yml` on `main` from the Actions tab with the
   version in its `release` input, which tags the commit `v<version>`
   itself; or tag the merge commit `v<version>` by hand and push the tag,
   which runs the same workflow.

The workflow builds the package with the spec's own `Release: 1` rather
than the run-number stamp an ordinary build gets, refuses a version that
is not what the spec says (and a dispatch that is not on `main`), and then
publishes a GitHub release named `v<version>`: the device RPM, a
`SHA256SUMS`, and that version's section of `CHANGELOG.md` as the text
(`scripts/release-notes.sh`, which fails the run if the section is
missing). The debug and source RPMs stay on the run's artifact.

Harbour intake is by hand: the RPM on the release page is the one to
upload, and `docs/HARBOUR.md` says what the validator will say about it.

## What a device build costs

| Step | One job | Four jobs |
|---|---|---|
| Pull the SDK image | 105 s | 80 s |
| Build the RPM |142 s | 82 s |
| Validate against Harbour | 10 s | 10 s |
| **The whole run** | **284 s** | **190 s** |

`rpm.yml` runs one job. Four is faster when it finishes, but at two or
four cargo stalls inside scratchbox2 often enough that the waiting costs
more than the parallelism saves (see Spec constraints below).

Two changes, in the order they pay:

- **The SDK image is derived, not upstream's.** `ci/build-sdk-image.sh` takes
`coderus/sailfishos-platform-sdk` by digest and produces an image with one
architecture instead of three, this package's `BuildRequires` already
installed, and the i686 rustlib already at `/usr/lib/rustlib`. It has to
flatten the result rather than layer it, because files deleted in a new
layer still weigh what they weighed. 5.04 GB of pull becomes about 2.3 GB,
and `zypper` leaves the critical path.
- `rust/target` and the crates are carried between runs.

## What a package weighs

The aarch64 package, built against SDK 5.2.0.15. "In the package" is what
a file costs inside the zstd payload, which is the number that matters for
a download; raw is what it costs on the phone.

Taken apart at 1.0.0 (`rpm/harbour-piirit.spec` 1.0.0-1, run 152):

| raw | in the package | what |
|---|---|---|
| 20.42 MB | 9.38 MB | `%{_libexecdir}/%{name}/deltachat-rpc-server` |
| 5.83 MB | 1.46 MB | `%{_bindir}/%{name}` |
| 3.05 MB | 3.01 MB | `qml/art/intro-*.png` |
| 1.42 MB | ~0.1 MB | the 41 `.qm` catalogs |
| ~0.7 MB | ~0.6 MB | the rest of `qml/`, the icons, `LICENSE`, `SOURCE.md` |

The bundled server is a fixed cost: upstream's stripped static-musl build,
and Harbour leaves nowhere else to put the core (`HARBOUR.md`). The other
rows are this tree's to answer for, and two of them were answered:

- **The pictures are painted at the size a phone draws them.** `IntroPage`
shows one at `min(width * 0.42, height * 0.30)` -- 453 px on the tallest
phone, 614 px on the Jolla Tablet -- and PNG is already compressed, so a
1254 px master was carried whole into the download and scaled away on the
device. At 640 px the five of them are 0.75 MB instead of 3.05 MB.
`the_intro_pictures_are_the_shape_the_shader_reads` pins both ends of that
range now, so they cannot regrow without a test saying so.
- **The binary is stripped by `[profile.release]`, not by rpmbuild.**
rpmbuild here does not strip what it packages -- the validator said `file
is not stripped!` of every build up to 1.0.0, which carried 2.08 MB of
symbol tables. `strip = "symbols"` removes them; `main` stays reachable
because `build.rs` exports it into `.dynsym` (`HARBOUR.md`). `lto = "thin"`
and `codegen-units = 1` are in the same block.

Measured either side of those three settings, on `main` and on the branch
that introduced them:

| | before (run 160) | after (run 161) |
|---|---|---|
| Download | 15,027,710 B (14.33 MiB) | 12,254,749 B (11.69 MiB) |
| Installed | 65126 blocks (31.80 MiB) | 55270 blocks (26.99 MiB) |

The build pays about a minute for the LTO: five and a half minutes became
six and a half at one cargo job.

## Spec constraints

Constraints encoded in `rpm/harbour-piirit.spec`:

- **The cargo job count under sb2 is a define, and it is one.** At `-j2`
  or `-j4` cargo stalls under sb2 -- it has been seen to futex-wait
  forever on an unreaped child while qmetaobject's C++ glue compiles --
  so `%{jobs}` defaults to **1**, and `rpm.yml` passes
  `mb2 build --define "jobs 1"` on every run rather than offering a
  choice. It applies only inside sb2; a native OBS worker lets cargo
  pick. The same spec also keeps the build's temporaries in the build
  directory, because a parallel link through the shared `/tmp` under sb2
  can lose an object file it has just written. `--define "jobs N"` is
  still there for a local build that wants to try more.
- **No `--target` for cargo.** Jolla's cargo pins build scripts to the
  tooling's host triple; `--target` on top makes cargo treat the whole build
  as a cross build. `SB2_RUST_TARGET_TRIPLE` already tells the accelerated
  rustc what to emit. Whisperfish's spec passes none either.
- **`CARGO_TARGET_<HOST>_LINKER=host-gcc` inside sb2.** rustc links build
  scripts by calling plain `cc`, which sb2 rewrites to the *cross* compiler
  (`aarch64-meego-linux-gnu-cc: unrecognized option '-m32'`). scratchbox2
  exposes the native compiler as `host-gcc`. Pointing at the tooling's gcc
  by absolute path is not enough — sb2 still rewrites the `ld` that gcc
  invokes, giving `cannot find /lib/libgcc_s.so.1`.
- **`QT_INCLUDE_PATH`/`QT_LIBRARY_PATH` exported in `%build`**: qttypes
  cannot exec the target `qmake` under sb2. `QT_LIBRARY_PATH` uses
  `%{_libdir}` — Qt is in `/usr/lib64` on aarch64, not `/usr/lib`.
- **`%{_target_cpu}`, not `%{_arch}`**, for the bundled server path.
- **`Exec=harbour-piirit`** in the desktop file: the invoker does not
  honour an `Exec=env FOO=bar` wrapper, so the bundled server path is a
  fallback inside the binary.
- **Harbour constrains the name, the paths and every `Requires:`.**
  `ci/harbour-check.sh` fails a build that breaks one; `HARBOUR.md` is
  the map, including the two rules this package still breaks.
- **No bare `%` in a spec comment.** rpm expands macros inside comments, and
  on the SDK's older rpm a comment mentioning `%build` expands to a preamble
  starting `LANG=C`, which rpm reads as a tag. Host rpm 4.18 leaves comments
  alone, so such a spec parses locally and fails only on the SDK.
  `ci/packaging-lint.sh` checks for this directly.
