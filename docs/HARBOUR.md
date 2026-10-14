# Harbour

Target: [Jolla's Harbour store](https://harbour.jolla.com). Validator fail = rejection; name, install paths, linker flags costly to change later. So `ci/harbour-check.sh` = mandatory gate, every PR.

## The two checks

| | `ci/harbour-check.sh` | `sfdk check -s harbour` |
|---|---|---|
| Runs | every PR (`ci.yml`) | RPM built (`rpm.yml`) |
| Reads | source tree | built package |
| Needs | rpm, file, binutils, host build | Sailfish SDK, minutes |
| Authority | no | **yes** |

- `sfdk check` = Jolla's `rpmvalidation.sh` on RPM via `ci/harbour-validate-rpm.sh`; decides. Source check: sfdk can't run every push; PR fix cheaper than intake.
- `sfdk check` runs after artifact upload; waived blockers don't fail it (rejected package still worth phone-testing; workflow red for known reason stops being read). Anything not in `ci/harbour/waivers.conf` fails.
- `ci/harbour/` = validator's allow-lists, verbatim via `scripts/update-harbour-rules.sh`; `rpm.yml` warns if behind upstream. `ci/harbour-check.sh` reimplements logic around them from `rpmvalidation.sh`.

## What the source check covers

Check IDs = Jolla's.

- **Naming** (1.1): `harbour-` prefix, lowercase. Version: digits, periods. Release: digits, `_`, periods, incl. `rpm.yml` stamp. Harbour arches only.
- **Layout** (1.2): `%files` per device arch in 4 permitted locations. `.desktop`, binary present. Nothing under `/home`, no debug dirs, no world/group-writable, setuid, setgid.
- **.desktop** (1.3): non-empty `Name=`. `Exec=`, `Icon=` = package name. `Type=Application`. `X-Nemo-Application-Type=silica-qt5`. `[X-Sailjail]`, never `[Sailjail]` or empty.
- **Sailjail** (1.4): 4 allowed keys only. `OrganizationName`, `ApplicationName` vs regex, reserved names. Permissions whitelisted. `ExecDBus` = `Exec`.
- **Icons** (1.5): 4 sizes, real PNGs, pixel dims = dir names.
- **QML** (1.6): imports vs allow-list, blocked prefixes. No absolute imports. Relative imports resolve in installed tree. No ELF outside 2 allowed places.
- **Binary** (1.6.1, 1.7): linked libs allowed. Links `__libc_start_main`. **Exports `main()` dynamic** (below). Symbol *version* (SDK's) visible only in built RPM.
- **RPM metadata** (1.8): no `Vendor:`, `Provides:`, `Obsoletes:`, `Conflicts:`, `Recommends:`, `Suggests:`, `Supplements:`, `Enhances:`. `Requires:` unversioned, allowed. No scriptlets/triggers. `libsailfishapp-launcher` iff `sailfish-qml` launcher. `qt5-qtdeclarative-import-xmllistmodel` if `QtQuick.XmlListModel` imported.
- **Runtime** (2.1, 2.5, 2.6): no hardcoded `/home/nemo`, `/home/defaultuser`. Data path spelled as sandbox grant. Nothing written to installed path.

`ci/harbour-check-selftest.sh` breaks each rule in throwaway copy, asserts check names it: "ok"-only gate looks like one that stopped looking.

## The SDK version is a Harbour rule

- Binary must link `__libc_start_main@GLIBC_2.34` (glibc 2.34 merged libpthread, libdl into libc, re-versioned symbol). Older glibc → `@GLIBC_2.17` on aarch64 → rejected. Only 5.x SDK provides it; first real run (4.6.0.13 build) failed on this, rest = known blockers:

        FAIL /usr/bin/harbour-piirit -- Binary does not link to __libc_start_main@GLIBC_2.34.

- `rpm.yml` default **5.2.0.15** (Jolla Phone baseline): deliberate floor (`PROJECT.md`).

## Exporting `main()`

- Silica binary must export `main()`: `silica-qt5` booster (mapplauncherd) `dlopen()`s it, looks up dynamically. C++: `Q_DECL_EXPORT`. Rust: none; `fn main` only in `.symtab`, stripped for package. `rust/piirit-app/build.rs` passes `--dynamic-list` → `main` in `.dynsym`.
- Stripping = `strip = "symbols"` (`--strip-all`) in `rust/Cargo.toml` `[profile.release]`; rpmbuild doesn't (BUILDING.md, "What a package weighs"). `.dynsym` survives: why export goes there.
- `--dynamic-list`, not `--export-dynamic-symbol` (binutils 2.35, maybe not in SDK) or `--export-dynamic` (exports all).

## The media plugin

- `libpiirit.so` = H.264 encoder for video calls (docs/CALLS.md), at `/usr/share/harbour-piirit/lib/gmp-piirit/1.0/` beside `piirit.info` Gecko finds it by. `/usr/share/<name>/lib/` = only ELF place besides binary; links only libc, libm, libstdc++, libgcc_s; x264 built in. No waiver.
- Validator warns "...then the rpath must be set accordingly": binary doesn't link it, no rpath. Gecko loads full path in own `plugin-container` (in app sandbox), dir from `MOZ_GMP_PATH` set before Gecko starts (`prepare_media_plugins`, `rust/piirit-shim/src/call_host.rs`). Env vars: Harbour silent.

## Waivers

- `ci/harbour/waivers.conf`: knowingly broken rules, one line + reason each. Every entry = submission blocker, so nothing fixable there.
- Line: `<check id> <subject glob> <message glob>`. Source check matches id + subject; RPM check subject *and* message: one known error on one path, not all (setuid or dynamic link on bundled server still fails).
- Waiver matching nothing fails check: can't outlive what it excuses.

## QtWidgets, and the vendored qmetaobject

- `qmetaobject-rs` builds QML engine on `QApplication` (QtWidgets, Harbour-banned; Silica uses `QGuiApplication`); no upstream off switch. App needs only `exec()`, `quit()`: `QGuiApplication` has both.
- `third_party/qmetaobject` = upstream 0.2.10 + `third_party/qmetaobject.patch` (3 lines: include, member type, constructor). Upstream fix: feature flag picking app class, would serve every Sailfish qmetaobject app; until then fork rebases clean.
- `qttypes` always passes `-lQt5Widgets`; no second fork: `rust/piirit-app/build.rs` links `--as-needed` (drops unreferenced libs), works only since patch removed last reference.
- `ci/vendor-check.sh`: crates.io tarball + patch must equal vendored tree exactly. Crate `tests/` not vendored, dropped both sides: 1,200 lines cargo never builds; CodeQL would flag high-severity in uncompiled code.
- **Each `#[derive(QObject)]` a PR adds = high-severity CodeQL "Access of invalid pointer"** on derive line: macro's dispatcher, destructor call Qt via raw pointers, attributed to macro. Existing `src/` derives silent (CodeQL comments changed lines only). No code fix: test `pageStack` stand-in must be QObject (lone page reads `pageStack` from QML context); moving test costs more than alert. Dismiss in Security tab, or CodeQL advanced setup excluding `rust/piirit-shim/tests/`.

## The open blockers

**Piirit cannot be submitted to Harbour today.** One structural rule broken; unfixable in this repo.

### The bundled core

- `deltachat-rpc-server` = ELF at `/usr/libexec/harbour-piirit/`, subprocess, JSON-RPC over stdio. Harbour allows ELF only at `/usr/bin/<NAME>`, `*.so` under `/usr/share/<NAME>/lib/`: 3 errors (path, binary, executable bit).

| Core option | Current API? | Buildable here? |
|---|---|---|
| `libdeltachat.so` (C interface) | **no, being deprecated** | yes: C ABI spans compiler gap |
| `deltachat-jsonrpc` crate, in-process | yes | **no**: needs Rust 1.89, SDK 1.75; Rust won't mix compilers |
| `deltachat-rpc-server` subprocess *(today)* | **yes, upstream recommends** | yes |

- Upstream: `libdeltachat` "is going to be deprecated and only exists because Android, iOS and Ubuntu Touch are still using it"; new projects: JSON-RPC.
- Remaining move not technical: rule predates upstreams shipping self-contained helpers; ours reproducible, checksum-pinned (`scripts/fetch-rpc-server.sh`), in app's sandbox. Case for Jolla, on forum validator README links; don't engineer around.
- `.so` rename passes validator, but README calls it circumvention; such apps removed even after approval. Not an option.
