# Harbour

Piirit is packaged for [Jolla's Harbour store](https://harbour.jolla.com).
Harbour's rules are not advice: a validator failure is a guaranteed
rejection, and several of them constrain things — the package name, the
install paths, the linker flags — that are expensive to change once code
is built around them.

So they are a CI gate. `ci/harbour-check.sh` runs on every pull request
and is mandatory.

## The two checks

| | `ci/harbour-check.sh` | `sfdk check -s harbour` |
|---|---|---|
| Runs | every pull request (`ci.yml`) | when an RPM is built (`rpm.yml`) |
| Reads | the source tree | the built package |
| Needs | rpm, file, binutils, a host build | the Sailfish SDK, minutes of runner time |
| Authority | no | **yes** |

The second is Jolla's own `rpmvalidation.sh`, fetched and run against the
RPM the SDK produces by `ci/harbour-validate-rpm.sh`. It is the one that
decides. The first exists because the second cannot run on every push, and
because a rule broken in a pull request is cheaper to fix than one
discovered at intake.

It runs *after* the artifact upload, and the blockers below do not fail it:
a package Harbour would reject is still one worth putting on a phone, and a
workflow that is red for a reason nobody intends to fix this week is a
workflow people stop reading. Anything not in `ci/harbour/waivers.conf`
fails it.

They are kept honest against each other: `ci/harbour/` holds the
validator's own allow-lists, copied verbatim by
`scripts/update-harbour-rules.sh`, and the `rpm.yml` step warns if those
copies have fallen behind upstream. `ci/harbour-check.sh` reimplements the
logic around them, from `rpmvalidation.sh`.

## What the source check covers

Check IDs follow Jolla's own numbering.

**Naming** (1.1) — the `harbour-` prefix and lowercase package name;
Version digits and periods only; Release digits, underscores and periods,
*including the Release `rpm.yml` stamps onto each build*; only Harbour
architectures offered.

**Layout** (1.2) — every path in `%files`, for each device architecture,
against the four locations Harbour permits; the .desktop file and the
binary present; nothing under `/home`; no debug directories; no
world- or group-writable, setuid or setgid install modes.

**The .desktop file** (1.3) — a non-empty `Name=`; `Exec=` and `Icon=`
exactly the package name; `Type=Application`;
`X-Nemo-Application-Type=silica-qt5`; `[X-Sailjail]`, never `[Sailjail]`,
and never empty.

**Sailjail** (1.4) — only the four allowed keys; `OrganizationName` and
`ApplicationName` against their regexes and the reserved-name list; every
permission on the whitelist; `ExecDBus` agreeing with `Exec`.

**Icons** (1.5) — all four sizes present, real PNGs, pixel dimensions
matching their directory names.

**QML** (1.6) — every import against the allow-list and the blocked-prefix
patterns; no absolute-path imports; relative imports resolving inside the
installed tree; no ELF file outside the two places one may live.

**The binary** (1.6.1, 1.7) — every linked library against the allowed
list; that it links `__libc_start_main`; and that it **exports `main()` as
a dynamic symbol**. See below. The source check cannot see the *version* of
that symbol, which is what the SDK decides; only the built RPM shows it.

**RPM metadata** (1.8) — no `Vendor:`; no `Provides:`, `Obsoletes:`,
`Conflicts:`, `Recommends:`, `Suggests:`, `Supplements:` or `Enhances:`;
every `Requires:` unversioned and on the allowed list; no scriptlets or
triggers; `libsailfishapp-launcher` required if and only if the
`sailfish-qml` launcher is used; `qt5-qtdeclarative-import-xmllistmodel`
required if `QtQuick.XmlListModel` is imported.

**Runtime policy** (2.1, 2.5, 2.6) — no hardcoded `/home/nemo` or
`/home/defaultuser`; the data path the app builds spelled the same way as
the sandbox grant it depends on; nothing written to a path the package
installs.

`ci/harbour-check-selftest.sh` breaks each of these in a throwaway copy of
the tree and asserts the check names it. A gate that only ever prints
"ok" is indistinguishable from one that has stopped looking.

## The SDK version is a Harbour rule

Harbour requires the binary to link `__libc_start_main@GLIBC_2.34`, and the
version is the point: 2.34 is where glibc merged libpthread and libdl into
libc and re-versioned the symbol. A binary built against an older glibc
references `@GLIBC_2.17` on aarch64 and is rejected outright.

Only a 5.x SDK provides it. The first real validator run, against a package
built with 4.6.0.13, failed on exactly this while every other finding was
one of the two known blockers:

    FAIL /usr/bin/harbour-piirit -- Binary does not link to __libc_start_main@GLIBC_2.34.

`rpm.yml` therefore defaults to **5.2.0.15**, the Jolla Phone's baseline.
That is a deliberate floor, not a compromise: a binary from a newer SDK can
call symbols an older phone lacks, and this project does not support phones
older than the current one (`PROJECT.md`).

## Exporting `main()`

Harbour rejects a Silica app whose binary does not export `main()`: the
`silica-qt5` booster in mapplauncherd `dlopen()`s the binary and looks the
symbol up dynamically. C++ apps mark it `Q_DECL_EXPORT`.

Rust has no equivalent. `fn main` becomes an ordinary global symbol, which
lives only in `.symtab` — and `.symtab` is stripped on the way into the
package, so by the time Harbour looks there is nothing there.
`rust/piirit-app/build.rs` passes `--dynamic-list` at link time to put
`main` in `.dynsym`, where stripping cannot reach it.

Who does the stripping is worth being exact about, because this tree
assumed rpmbuild did and it does not: every package built before
`rust/Cargo.toml` grew a `[profile.release]` shipped a binary the
validator called out as `file is not stripped!`, carrying 2.08 MB of
symbol tables a phone never reads. `strip = "symbols"` in that profile is
what removes them now. It is `--strip-all` under the hood and `.dynsym`
survives it, so the booster still finds `main` — which is the whole
reason the export is made there rather than left in `.symtab`.

`--dynamic-list` rather than `--export-dynamic-symbol`, which needs
binutils 2.35 and so may not exist in the SDK, or `--export-dynamic`,
which would export every symbol in the binary.

## Waivers

`ci/harbour/waivers.conf` records rules this package knowingly breaks, one
line each with a reason. Nothing belongs there that can be fixed: every
entry is a submission blocker.

A line is `<check id> <subject glob> <message glob>`. The source check
matches its findings on the id and the subject; the RPM check matches the
real validator's subject *and* message, so a waiver excuses one known
error about a path and never every error the validator might raise about
it -- a setuid bit or a dynamic link on the bundled server would still
fail.

A waiver that stops matching anything fails the check, so the file cannot
outlive what it excuses. Its entries cover the two blockers below.

## QtWidgets, and the vendored qmetaobject

`qmetaobject-rs` builds its QML engine on `QApplication`, which comes from
QtWidgets — a library Harbour does not allow, since a Silica app is
expected to use QtGui's `QGuiApplication`. Upstream carries that
unconditionally, on the released crate and on master, with no feature to
turn it off.

Nothing here needs QtWidgets. The application object is used only for
`exec()` and `quit()`, both of which `QGuiApplication` provides. So
`third_party/qmetaobject` is upstream 0.2.10 plus
`third_party/qmetaobject.patch`: three lines, swapping the include, the
member type and the constructor.

`qttypes` separately passes `-lQt5Widgets` unconditionally, which would
record the dependency even with nothing using it. Rather than fork a
second crate for one line, `rust/piirit-app/build.rs` links with
`--as-needed`, which drops any library no symbol refers to. That only
works *because* the patch removed the last reference — with `QApplication`
still in use the library is genuinely needed and `--as-needed` keeps it.

Carrying someone else's crate in-tree is only safe while the difference is
visible, so `ci/vendor-check.sh` fetches the crates.io tarball, applies the
patch, and requires the result to match the vendored tree exactly. A stray
edit fails it; so does a patch that stops describing the tree.

The crate's own `tests/` are not vendored, and the check drops them from
both sides. Cargo never builds a dependency's tests, so they are 1,200
lines that cannot run, and CodeQL would scan them anyway and report
high-severity findings in code this repository does not compile.

The same scanner reports the same kind of finding on our own code. **Every
`#[derive(QObject)]` a pull request adds draws a high-severity "Access of
invalid pointer" alert**, on the `#[derive]` line itself: the macro
generates the dispatcher and destructor Qt calls through raw pointers, and
CodeQL attributes expanded code to the macro. The alert follows the derive
rather than the file, and the derives already throughout `src/` do not
report only because CodeQL comments on lines a pull request changed.

Nothing in the code answers it. Test scaffolding that stands in for a
`pageStack` has to be a QObject, because a page loaded on its own reads
`pageStack` from the QML context and nothing else can be put there.
Rewriting a test to avoid the derive means putting it somewhere it does
not belong, which costs more than the alert does. Dismiss it in the
Security tab, or -- for a lasting answer -- move the repository from
CodeQL's default setup to an advanced one whose configuration can exclude
`rust/piirit-shim/tests/`.

The real fix is upstream: a feature flag choosing between `QApplication`
and `QGuiApplication` would serve every Sailfish app built on qmetaobject,
all of which hit this. Until then the fork is three lines and rebases
cleanly.

## The open blockers

**Piirit cannot be submitted to Harbour today.** One rule is broken,
and it is structural: it cannot be fixed by editing anything in this
repository.

### The bundled core

`deltachat-rpc-server` is an ELF executable, bundled at
`/usr/libexec/harbour-piirit/`, spawned as a subprocess and spoken to
over JSON-RPC on stdio. Harbour allows ELF files in exactly two places:
`/usr/bin/<NAME>`, and `*.so` under `/usr/share/<NAME>/lib/`. Neither fits
a second executable, so this is three validator errors — the path, the
binary in it, and its executable bit.

There are three ways to hold the core, and each is blocked differently:

| | Current API? | Buildable here? |
|---|---|---|
| `libdeltachat.so` (C interface) | **no — being deprecated** | yes: a C ABI spans the compiler gap |
| `deltachat-jsonrpc` crate, in-process | yes | **no**: needs Rust 1.89, the SDK ships 1.75, and Rust will not mix compiler versions |
| `deltachat-rpc-server` subprocess *(today)* | **yes — what upstream recommends** | yes |

Upstream is explicit that `libdeltachat` "is going to be deprecated and
only exists because Android, iOS and Ubuntu Touch are still using it", and
that new projects should use the JSON-RPC API. Migrating onto it to satisfy
a packaging rule would mean adopting a dying interface. The Rust-native
replacement cannot be compiled by the Rust the SDK ships, and Rust refuses
to link output from two compiler versions.

So the remaining move is not technical. Harbour's one-executable rule
predates upstreams shipping self-contained helper binaries as the
recommended integration, and Delta Chat's is reproducibly built,
checksum-pinned (`scripts/fetch-rpc-server.sh`) and confined by the same
sandbox as the app. That is a case to put to Jolla, on the forum their
validator's README points at, rather than to engineer around.

Renaming the binary to `.so` would pass the validator. It is also
precisely what that README calls circumvention, and it says such apps are
removed from the store even after approval. Not an option.