# Sailfish/OBS packaging for Piirit (template: Whisperfish spec; docs/PROJECT.md).
# Sources: rust/, qml/, separately fetched deltachat-rpc-server (scripts/fetch-rpc-server.sh).
# C libs built from source by build scripts with target compiler: LAME (in mp3lame-sys crate),
# FFmpeg and x264 (third_party/ submodules, so Source0 must carry them; docs/BUILDING.md,
# "Making a video smaller").
#
# `sfdk build` fetches crates; `sfdk build -- --with vendor` uses vendor.tar.xz (OBS/Chum, no
# network; scripts/vendor-crates.sh).
#
# Harbour rules constrain name, paths, Requires; ci/harbour-check.sh fails a build breaking one.
# docs/HARBOUR.md maps them, incl. the two rules this package still breaks.

%bcond_with vendor

# Reproducible package (docs/VERIFYING.md). With SOURCE_DATE_EPOCH set
# (build-in-container.sh: commit time): build time = it, file dates clamped to
# it. Unset: rpm's defaults. Host name fixed either way (rpm records it).
%define use_source_date_epoch_as_buildtime 1
%define clamp_mtime_to_source_date_epoch 1
%define _buildhost piirit-build

# No i486 server (no musl build upstream): i486 needs PIIRIT_RPC_SERVER. Device arches bundle it.
%ifarch %ix86
%define bundle_rpc_server 0
%else
%define bundle_rpc_server 1
%endif

# Harbour: `harbour-` prefix, lowercase. Users see .desktop Name=, not this.
Name:       harbour-piirit
Summary:    Native SailfishOS client for Delta Chat
Version:    2.2.0
Release:    1
# Licences: GPL-3.0-or-later app; MPL-2.0 server; calls-webapp GPL-3.0 (+MIT, Apache-2.0);
# emoji CC-BY-4.0, MIT, Unicode-3.0; FFmpeg LGPL-2.1+, x264 GPL-2.0+. Tag = package contents.
%if 0%{?bundle_rpc_server}
License:    GPL-3.0-or-later AND MPL-2.0 AND MIT AND Apache-2.0 AND CC-BY-4.0 AND Unicode-3.0 AND LGPL-2.1-or-later AND GPL-2.0-or-later
%else
License:    GPL-3.0-or-later AND MIT AND Apache-2.0 AND CC-BY-4.0 AND Unicode-3.0 AND LGPL-2.1-or-later AND GPL-2.0-or-later
%endif
Group:      Qt/Qt
URL:        https://github.com/muhnschein/piirit
Source0:    %{name}-%{version}.tar.gz
%if %{with vendor}
Source1:    vendor.tar.xz
Source2:    vendor.toml
%endif

# Unversioned; only allowed_requires.conf names. Validator splits on whitespace, so operator
# and version are rejected; Harbour derives its own range. OS floor 4.5.0 (for [X-Sailjail])
# goes in submission form's "From OS version".
#
# No libsailfishapp-launcher: it provides `sailfish-qml`, used only by QML-only apps;
# requiring it unused is an error itself.
Requires:   sailfishsilica-qt5
# One package per QML module beyond Silica (QtMultimedia, QtSensors, Nemo.Thumbnailer,
# Nemo.Notifications): all allowed, on every image. Declared, not relied on by luck.
Requires:   qt5-qtdeclarative-import-multimedia
Requires:   qt5-qtdeclarative-import-sensors
Requires:   nemo-qml-plugin-notifications-qt5
Requires:   nemo-qml-plugin-thumbnailer-qt5
# Nemo.DBus: name a tapped notification calls back to.
Requires:   nemo-qml-plugin-dbus-qt5
# Nemo.Configuration: profile-less settings in dconf, so a change reaches every open page.
Requires:   nemo-qml-plugin-configuration-qt5
# Nemo.KeepAlive: phone awake during a call, screen on while ringing
# (qml/components/CallCenter.qml). Package carries QML module beside the library.
Requires:   libkeepalive
# Sailfish.WebView: webxdc apps and store only (qml/pages/Webxdc*Page.qml); a device without
# engine loses those alone.
# Silica's WebView.qml imports Sailfish.WebView.Popups/.Pickers; unresolved, that page never loads.
Requires:   sailfish-components-webview-qt5
Requires:   sailfish-components-webview-qt5-popups
Requires:   sailfish-components-webview-qt5-pickers

# Harbour allows no Provides:, and rpm generates one from any packaged shared library (e.g.
# libpiirit.so under %%{appdatadir}/lib). Exclude both private dirs.
%define __provides_exclude_from ^(%{_datadir}/%{name}|%{_libexecdir}/%{name})/.*$

# Sailfish Rust 1.75.0. rust/Cargo.lock v3 (Cargo reads v4 from 1.78).
# rust-std-static: native std. Cross std is rust-std-static-%%{rusttarget}; if "can't find crate
# for `std`", install into tooling:
#   sfdk tools exec <tooling> zypper in rust-std-static-%%{rusttarget}
BuildRequires:  rust >= 1.75
BuildRequires:  rust-std-static >= 1.75
BuildRequires:  cargo >= 1.75
# qmetaobject-rs: C++ glue against Qt.
BuildRequires:  gcc-c++
BuildRequires:  git
BuildRequires:  desktop-file-utils
BuildRequires:  pkgconfig(Qt5Core)
BuildRequires:  pkgconfig(Qt5Qml)
BuildRequires:  pkgconfig(Qt5Quick)
# QAudioRecorder for recorder (piirit-shim/src/recorder.rs). Runtime qt5-qtmultimedia: rpm
# derives it from binary; Harbour allows it.
BuildRequires:  pkgconfig(Qt5Multimedia)
# libpulse for mic watch (piirit-shim/src/mic_watch.rs); libpulse.so.0 Harbour-allowed.
BuildRequires:  pkgconfig(libpulse)
# lrelease: translation catalogs in %%build.
BuildRequires:  qt5-qttools-linguist

%ifarch %arm
%define rusttarget armv7-unknown-linux-gnueabihf
%endif
%ifarch aarch64
%define rusttarget aarch64-unknown-linux-gnu
%endif
%ifarch %ix86
# No i486 server build: use x86_64 emulator.
%define rusttarget i686-unknown-linux-gnu
%endif
%ifarch x86_64
%define rusttarget x86_64-unknown-linux-gnu
%endif

# cargo jobs under sb2 only (native OBS worker: cargo picks). One: four finishes faster
# (142 s to 82 s), but at 2-4 cargo stalls under sb2 (futex-wait on unreaped child during
# qmetaobject C++ glue) often enough to cost more than it saves. `--define "jobs N"`
# overrides; rpm.yml passes one every run (numbers: docs/BUILDING.md).
%{!?jobs: %global jobs 1}

# Binary dir: under sb2 target/<triple>/release (SB2_RUST_TARGET_TRIPLE, %%build); native
# target/release. Checked at install.
%define builddir rust/target/%{rusttarget}/release
%define nativedir rust/target/release
%define appdatadir %{_datadir}/%{name}
%define appexecdir %{_libexecdir}/%{name}

%description
Piirit is a Silica/QML SailfishOS client for Delta Chat. It contributes
only the UI, Sailfish platform integration, and packaging; all
IMAP/SMTP/MIME/encryption logic is delegated to a bundled
deltachat-rpc-server binary, spoken to over JSON-RPC on stdio. See
docs/PROJECT.md in the source tree for the full project scope and, just as
importantly, its non-goals.

%prep
%setup -q -n %{name}-%{version}
%if %{with vendor}
# vendor.tar.xz = rust/vendor/; vendor.toml = [source] stanza.
%setup -q -T -D -a 1 -n %{name}-%{version}
mkdir -p rust/.cargo
install -m 644 %{SOURCE2} rust/.cargo/config.toml
%endif

%build
rustc --version
cargo --version

# sb2 cross-compile needs Docker build engine (docs/BUILDING.md). Accelerated rustc emits x86
# unless given the real target (as Whisperfish, xulrunner-qt5.spec).
export SB2_RUST_TARGET_TRIPLE=%{rusttarget}

# qttypes runs `qmake -query`; build scripts under sb2 can't reliably exec target qmake. Both
# vars set: qttypes skips qmake, reads qtcoreversion.h (per crate build.rs). Target-rootfs paths
# as seen in sb2.
export QT_INCLUDE_PATH=%{_includedir}/qt5
export QT_LIBRARY_PATH=%{_libdir}

# SBOX_SESSION_DIR: set by sb2 in sessions; marks cross build. Gates job count and linker override.
# Build scripts/proc-macros link with `cc`, which sb2 maps to the cross compiler ("-m32" fails).
# Point host linker at `host-gcc` (SBOX_HOST_GCC_NAME). Outside sb2: no override.
if [ -n "${SBOX_SESSION_DIR:-}" ]; then
    host_triple=$(rustc -vV | sed -n 's/^host: //p')
    export "CARGO_TARGET_$(echo "$host_triple" | tr 'a-z-' 'A-Z_')_LINKER"=host-gcc
fi

# Parallel link under sb2 can lose objects via shared /tmp: temporaries in build dir.
export TMPDIR=${TMPDIR:-"$PWD/.tmp"}
mkdir -p "$TMPDIR"

# No `--target`: sb2 would look for a cross std. SB2_RUST_TARGET_TRIPLE does the job;
# cargo writes target/<triple>/release.
# Run from rust/: cargo reads .cargo/config.toml from cwd and parents, not manifest dir. %%prep
# installs vendor stanza at rust/.cargo/; from root --offline fails. target/ same either way.
(
cd rust
cargo build \
    ${SBOX_SESSION_DIR:+-j%{jobs}} \
    --release \
    --locked \
%if %{with vendor}
    --offline \
%endif
    --manifest-path Cargo.toml \
    --package piirit-app
)

# translations/piirit-<lang>.ts -> .qm at build time (not checked in).
./scripts/release-translations.sh

%install
rm -rf %{buildroot}

builddir=%{builddir}
[ -x "$builddir/%{name}" ] || builddir=%{nativedir}
install -Dm 755 "$builddir/%{name}" \
    %{buildroot}%{_bindir}/%{name}

# H.264 for video calls: media plugin for Gecko, built from x264 by rust/piirit-video/build.rs.
# Lives in %%{appdatadir}/lib (Harbour's one allowed private lib); piirit-app points Gecko at it.
install -Dm 755 "$builddir/gmp-piirit/1.0/libpiirit.so" \
    %{buildroot}%{appdatadir}/lib/gmp-piirit/1.0/libpiirit.so
install -Dm 644 "$builddir/gmp-piirit/1.0/piirit.info" \
    %{buildroot}%{appdatadir}/lib/gmp-piirit/1.0/piirit.info

# QML under app data dir: piirit-app's qml_dir() finds it (PIIRIT_QML_DIR, then this path, then
# source-tree fallback). Only engine-read files: stray backups would ship and draw Harbour's eye.
# qml/art/ .png files: first-screen mark and intro pictures.
(cd qml && find . -type f \( -name '*.qml' -o -name '*.js' -o -name '*.png' -o -name qmldir \) -exec \
    install -Dm 644 "{}" "%{buildroot}%{appdatadir}/qml/{}" \; )

# Compiled .qm beside QML, where piirit-app looks; .ts stay in tree.
(cd translations && find . -type f -name 'piirit-*.qm' -exec \
    install -Dm 644 "{}" "%{buildroot}%{appdatadir}/translations/{}" \; )

# Bundled server in private libexec, not %%{_bindir}: nothing searching PATH picks it up as a
# generic deltachat-rpc-server. piirit-app defaults to this exact path.
#
# %%{_target_cpu}, not %%{_arch}: rpm maps armv7h* to "arm"; fetch script writes armv7hl/.
%if 0%{?bundle_rpc_server}
install -Dm 755 vendor/deltachat-rpc-server/%{_target_cpu}/deltachat-rpc-server \
    %{buildroot}%{appexecdir}/deltachat-rpc-server
%endif

# SOURCE.md: MPL-2.0 3.2(a) for bundled server (upstream repo, tag, sha256s).
install -Dm 644 vendor/deltachat-rpc-server/SOURCE.md \
    %{buildroot}%{appdatadir}/vendor/deltachat-rpc-server/SOURCE.md
# Same for call page (rust/piirit-shim/src/call_host.rs): GPL-3.0 source and notices.
install -Dm 644 vendor/calls-webapp/SOURCE.md \
    %{buildroot}%{appdatadir}/vendor/calls-webapp/SOURCE.md
# Emoji attribution (CC-BY-4.0) travels with them.
install -Dm 644 vendor/emoji/SOURCE.md \
    %{buildroot}%{appdatadir}/vendor/emoji/SOURCE.md
install -Dm 644 LICENSE \
    %{buildroot}%{appdatadir}/LICENSE

desktop-file-install \
    --dir %{buildroot}%{_datadir}/applications \
    %{name}.desktop

install -Dm 644 icons/86x86/%{name}.png \
    %{buildroot}%{_datadir}/icons/hicolor/86x86/apps/%{name}.png
install -Dm 644 icons/108x108/%{name}.png \
    %{buildroot}%{_datadir}/icons/hicolor/108x108/apps/%{name}.png
install -Dm 644 icons/128x128/%{name}.png \
    %{buildroot}%{_datadir}/icons/hicolor/128x128/apps/%{name}.png
install -Dm 644 icons/172x172/%{name}.png \
    %{buildroot}%{_datadir}/icons/hicolor/172x172/apps/%{name}.png
install -Dm 644 icons/256x256/%{name}.png \
    %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/%{name}.png

%files
%defattr(-,root,root,-)
%{_bindir}/%{name}
%{appdatadir}
%if 0%{?bundle_rpc_server}
# Own the directory too: listing only the file leaves /usr/libexec/%%{name} on uninstall.
%dir %{appexecdir}
%{appexecdir}/deltachat-rpc-server
%endif
%{_datadir}/applications/%{name}.desktop
%{_datadir}/icons/hicolor/*/apps/%{name}.png
