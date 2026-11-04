# `deltachat-rpc-server` provenance

Binaries bundled by Piirit's RPM (installed to `/usr/libexec/harbour-piirit/deltachat-rpc-server`) = **unmodified upstream builds**:

- **Project:** Delta Chat core (chatmail core library)
- **Source:** https://github.com/chatmail/core
- **Version / tag:** `v2.67.0`
- **License:** MPL-2.0. Piirit GPL-3.0-or-later; the two sit side by side in RPM as separate works. This file installed with package for MPL-2.0 §3.2(a): recipients told how to get Source Code Form.
- **Build:** upstream release builds from `.github/workflows/deltachat-rpc-server.yml` nix builds, **statically linked against musl libc** ("to avoid problems with glibc version incompatibility", per that workflow). Triples: `aarch64-unknown-linux-musl`, `armv7-unknown-linux-musleabihf` (hard-float, matches Sailfish `armv7hl`), `x86_64-unknown-linux-musl`.
- **Channel:** PyPI `deltachat-rpc-server==2.67.0` wheels = same binaries as upstream GitHub release (both nix output). Fetched, checksum-verified, placed by `scripts/fetch-rpc-server.sh`, which pins sha256 of every wheel *and* every extracted binary.

## Layout

```
vendor/deltachat-rpc-server/
  aarch64/deltachat-rpc-server   sha256 38066a1cd9270ced33e32a0ce3f1fc526ba68b0eae3a401e207fa8bcb5166491
  armv7hl/deltachat-rpc-server   sha256 83d3235ee62da181fb78a18567867e55a19e2fa32df674cc6867c2fb8f3a27ba
  x86_64/deltachat-rpc-server    sha256 40e0df8174027d3048c330b2b5869c995bce7b0bb17365e6e6133005a00d38e3
```

Dir names = Sailfish arch names (`%{_target_cpu}`, not `%{_arch}`, which maps every armv7h* to `arm`). `rpm/harbour-piirit.spec` `%install` keys on them.

Binaries not committed (see `.gitignore`). Run fetch script to populate.

## Verified so far / still open

- [x] All three binaries statically linked and stripped (`file`, `ldd`).
- [x] x86_64 binary runs, answers over stdio. Gated test `rust/deltachat-jsonrpc/tests/real_server.rs` (`DELTACHAT_RPC_SERVER=<path> cargo test -p deltachat-jsonrpc`) passes: `get_system_info` health check, account creation, config round trip, transport added against stub mail server on 127.0.0.1, real event delivery, chat/message list wire shapes UI depends on.
- [ ] `aarch64`/`armv7hl` binaries on real Sailfish device or emulator. Static musl = only kernel ABI matters, so expected to run as-is. Only provable on hardware.

## Updating

Bump `VERSION` and all sha256 pins in `scripts/fetch-rpc-server.sh` together, re-run, update this file. Bundled version and version stated here must never drift apart.
