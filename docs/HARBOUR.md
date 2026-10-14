# Harbour

Piirit targets [Jolla's Harbour store](https://harbour.jolla.com). Rules not advice: validator fail = guaranteed rejection. Name, paths, linker flags: costly to change later.

CI gate: `ci/harbour-check.sh` every PR. Mandatory.

## The two checks

| | `ci/harbour-check.sh` | `sfdk check -s harbour` |
|---|---|---|
| Runs | every PR (`ci.yml`) | RPM built (`rpm.yml`) |
| Reads | source tree | built package |
| Needs | rpm, file, binutils, host build | Sailfish SDK, minutes runner time |
| Authority | no | **yes** |

- `sfdk check` = Jolla's `rpmvalidation.sh` on RPM via `ci/harbour-validate-rpm.sh`. Decides.
- Source check exists: sfdk can't run every push; fixing in PR cheaper than at intake.
- `sfdk check` runs *after* artifact upload. Waived blockers below don't fail it: rejected package still worth putting on phone; workflow red for reason nobody fixes this week stops being read. Anything not in `ci/harbour/waivers.conf` fails it.
- `ci/harbour/` = allow-lists copied verbatim by `scripts/update-harbour-rules.sh`. `rpm.yml` warns if behind upstream. `ci/harbour-check.sh` reimplements logic around them from `rpmvalidation.sh`.

## What the source check covers

Check IDs = Jolla numbering.

- **Naming** (1.1): `harbour-` prefix, lowercase. Version: digits, periods. Release: digits, `_`, periods, incl. `rpm.yml` stamp. Harbour arches only.
- **Layout** (1.2): `%files` paths per device arch in 4 permitted locations. `.desktop` + binary present. Nothing under `/home`, no debug dirs, no world/group-writable, setuid, setgid.
- **.desktop** (1.3): non-empty `Name=`. `Exec=`, `Icon=` = package name. `Type=Application`. `X-Nemo-Application-Type=silica-qt5`. `[X-Sailjail]`, never `[Sailjail]` or empty.
- **Sailjail** (1.4): 4 keys only. `OrganizationName`, `ApplicationName` vs regex, reserved names. Permissions on whitelist. `ExecDBus` = `Exec`.
- **Icons** (1.5): 4 sizes, real PNGs, pixel dims = dir names.
- **QML** (1.6): imports vs allow-list, blocked prefixes. No absolute-path imports. Relative imports resolve in installed tree. No ELF outside 2 allowed places.
- **Binary** (1.6.1, 1.7): linked libs vs allowed list. Links `__libc_start_main`. **Exports `main()` dynamic** (below). Symbol *version* (SDK decides) visible only in built RPM.
- **RPM metadata** (1.8): no `Vendor:`, `Provides:`, `Obsoletes:`, `Conflicts:`, `Recommends:`, `Suggests:`, `Supplements:`, `Enhances:`. `Requires:` unversioned, allowed list. No scriptlets/triggers. `libsailfishapp-launcher` iff `sailfish-qml` launcher used. `qt5-qtdeclarative-import-xmllistmodel` if `QtQuick.XmlListModel` imported.
- **Runtime** (2.1, 2.5, 2.6): no hardcoded `/home/nemo`, `/home/defaultuser`. Data path = sandbox grant spelling. Nothing written to installed path.

`ci/harbour-check-selftest.sh` breaks each rule in throwaway copy, asserts check names it. "ok"-only gate looks identical to one that stopped looking.

## The SDK version is a Harbour rule

- Binary must link `__libc_start_main@GLIBC_2.34`. 2.34 = glibc merged libpthread/libdl into libc, re-versioned symbol. Older glibc binary refs `@GLIBC_2.17` on aarch64 = rejected.
- Only 5.x SDK provides it. First validator run (4.6.0.13 package) failed on this; other findings = two known blockers. Error:

        FAIL /usr/bin/harbour-piirit -- Binary does not link to __libc_start_main@GLIBC_2.34.

- `rpm.yml` default **5.2.0.15** (Jolla Phone baseline). Deliberate floor: newer SDK binary may call symbols older phones lack. Older phones unsupported (`PROJECT.md`).

## Exporting `main()`

- Harbour rejects Silica app without exported `main()`. `silica-qt5` booster (mapplauncherd) `dlopen()`s binary, looks up dynamically. C++: `Q_DECL_EXPORT`.
- Rust: no equivalent. `fn main` = ordinary global, only in `.symtab`, stripped on way into package. `rust/piirit-app/build.rs` passes `--dynamic-list` at link -> `main` in `.dynsym`.
- rpmbuild does NOT strip (tree wrongly assumed it did). Every package before `rust/Cargo.toml` `[profile.release]`: validator `file is not stripped!`, 2.08 MB symbol tables phone never reads.
- `strip = "symbols"` in `rust/Cargo.toml` `[profile.release]` = `--strip-all`; `.dynsym` survives, booster still finds `main`. Whole reason export goes in `.dynsym`, not `.symtab`.
- Use `--dynamic-list`. Not `--export-dynamic-symbol` (needs binutils 2.35, maybe not in SDK). Not `--export-dynamic` (exports all).

## The media plugin

- `libpiirit.so` = H.264 encoder for calls (docs/CALLS.md). Path `/usr/share/harbour-piirit/lib/gmp-piirit/1.0/`, beside `piirit.info` Gecko finds it by.
- `/usr/share/<name>/lib/` = only place Harbour allows ELF besides binary. Links only allowed-list libs: libc, libm, libstdc++, libgcc_s. x264 built in, as in binary. Neither check needs waiver.
- Validator warns it can't tell if binary needs it ("...then the rpath must be set accordingly"). Binary doesn't link it: no rpath.
- Gecko loads by full path in plugin process; dir from `MOZ_GMP_PATH`, set before Gecko starts (`prepare_media_plugins` in `rust/piirit-shim/src/call_host.rs`). Harbour silent on env vars. Plugin process = Gecko `plugin-container`, in app sandbox.

## Waivers

- `ci/harbour/waivers.conf` = known-broken rules, 1 line + reason each. Fixable things never here: every entry blocks submission.
- Line: `<check id> <subject glob> <message glob>`.
- Source check: id + subject. RPM check: subject *and* message. Excuses one error on one path, not all (setuid or dynamic link on bundled server still fails).
- Matches nothing = check fails. Can't outlive what it excuses.

## QtWidgets, and the vendored qmetaobject

- `qmetaobject-rs` builds QML engine on `QApplication` (QtWidgets). Harbour disallows QtWidgets; Silica uses `QGuiApplication`. No upstream off switch.
- Nothing needs QtWidgets: app object only for `exec()`, `quit()`, both on `QGuiApplication`.
- `third_party/qmetaobject` = upstream 0.2.10 + `third_party/qmetaobject.patch` (3 lines: include, member type, constructor).
- `qttypes` passes `-lQt5Widgets` unconditionally. No second crate fork: `rust/piirit-app/build.rs` links `--as-needed` (drops libs no symbol refers to). Works only since patch removed last reference; with `QApplication` used, lib kept.
- `ci/vendor-check.sh`: fetch crates.io tarball, apply patch, exact match vs vendored tree. Stray edit or stale patch fails.
- Crate `tests/` not vendored; check drops them both sides. 1,200 lines that can't run (cargo never builds dep tests); CodeQL would still scan them, flag high-severity in uncompiled code.
- **Every `#[derive(QObject)]` a PR adds = high-severity "Access of invalid pointer" alert** on `#[derive]` line. Macro generates dispatcher + destructor Qt calls via raw pointers; CodeQL attributes expanded code to macro. Alert follows derive, not file. Existing `src/` derives silent (CodeQL comments changed lines only).
- No code fix: `pageStack` test stand-in must be QObject (page alone reads `pageStack` from QML context). Rewrite to avoid derive puts test where it doesn't belong; costs more than alert.
- Options: dismiss in Security tab. Lasting: CodeQL default -> advanced setup, excluding `rust/piirit-shim/tests/`.
- Real fix upstream: feature flag `QApplication` vs `QGuiApplication`; would serve every Sailfish app on qmetaobject (all hit this). Until then fork = 3 lines, rebases clean.

## The open blockers

**Piirit cannot be submitted to Harbour today.** One structural rule broken; not fixable in this repo.

### The bundled core

- `deltachat-rpc-server` = ELF at `/usr/libexec/harbour-piirit/`, subprocess JSON-RPC over stdio. Harbour allows ELF only at `/usr/bin/<NAME>`, `*.so` under `/usr/share/<NAME>/lib/`. Neither fits second executable: 3 validator errors (path, binary, executable bit).

| Core option | Current API? | Buildable here? |
|---|---|---|
| `libdeltachat.so` (C interface) | **no, being deprecated** | yes: C ABI spans compiler gap |
| `deltachat-jsonrpc` crate, in-process | yes | **no**: needs Rust 1.89, SDK has 1.75; Rust won't mix compiler versions |
| `deltachat-rpc-server` subprocess *(today)* | **yes, upstream recommends** | yes |

- Upstream: `libdeltachat` "is going to be deprecated and only exists because Android, iOS and Ubuntu Touch are still using it". New projects: JSON-RPC. Migrating = adopting dying interface.
- Rust-native replacement: SDK's Rust can't compile it.
- Not technical now. Harbour rule predates upstream self-contained helpers. Delta Chat's: reproducible, checksum-pinned (`scripts/fetch-rpc-server.sh`), confined by app's sandbox. Case to Jolla on forum linked from validator README. Don't engineer around.
- `.so` rename passes validator. README calls it circumvention; apps removed even after approval. Not an option.
