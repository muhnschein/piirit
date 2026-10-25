# piirit developer targets.
#
# `make check` = CI minus SDK/phone needs. Not fully offline (msrv, deny).
#
# Qt5 packages (Debian/Ubuntu):
#   apt install qtbase5-dev qtdeclarative5-dev qtmultimedia5-dev \
#               qtdeclarative5-dev-tools qml-module-qtquick2 \
#               qml-module-qtquick-window2 qttools5-dev-tools \
#               libqt5svg5 libpulse-dev pulseaudio pulseaudio-utils
#
# qml-module-qtquick2: QtQuick runtime (-dev omits it). -window2: QtQuick.Window
# (tests). qttools5-dev-tools: lupdate/lrelease. libqt5svg5: SVG image plugin
# (emoji pictures). Rest: multimedia, pulse (mic, test server).

.PHONY: check test lint fmt qml-lint packaging-lint lockfile-lint doc-lint \
        msrv deny workflow-lint leak-check integration harbour vendor-check fetch-server \
        sonar-report-test apt-install-test sonar-reports translations \
        cover-icons clean

CARGO ?= cargo
# Shim tests need a Qt event-loop platform plugin.
export QT_QPA_PLATFORM = offscreen

## CI's steps, same order; keep in step with ci.yml. `msrv` fetches toolchain
## first time, `deny` needs advisory DB.
check: fmt lint test doc-lint msrv qml-lint lockfile-lint packaging-lint harbour \
       sonar-report-test apt-install-test vendor-check deny workflow-lint

## Unit, integration, Qt event-loop tests.
##
## nextest if installed (one process per test: ~10 min down to ~2.5; see
## rust/.config/nextest.toml), else `cargo test` (check must work on a bare
## laptop).
##   cargo install --locked cargo-nextest
## nextest skips doctests, so `--doc` runs beside it; none exist yet, so
## packaging-lint.sh fails a tree dropping it.
test:
	@command -v cargo-nextest >/dev/null 2>&1 || \
		echo "test: cargo-nextest is not installed; running the slow way \
(cargo install --locked cargo-nextest)"
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		cd rust && $(CARGO) nextest run --workspace; \
	else \
		cd rust && $(CARGO) test --workspace; \
	fi
	cd rust && $(CARGO) test --workspace --doc

## Clippy, workspace lints, tests and bins too.
lint:
	cd rust && $(CARGO) clippy --workspace --all-targets -- -D warnings

fmt:
	cd rust && $(CARGO) fmt --all --check

## Parse every .qml; Qt 5.6 rules are a Rust test.
qml-lint:
	./ci/qml-lint.sh

## Spec, desktop entry, shell, docs refs. Missing tool: SKIP here, FAIL in CI.
packaging-lint:
	./ci/packaging-lint.sh

## Cargo.lock must stay v3: Sailfish's cargo 1.75 can't read v4.
lockfile-lint:
	./ci/check-lockfile.sh

## Broken intra-doc links are errors.
doc-lint:
	cd rust && RUSTDOCFLAGS="-D warnings" $(CARGO) doc --workspace --no-deps

## Build at Sailfish's 1.75 floor; part of `check`. Modern clippy misses newer
## std methods/syntax: only a real 1.75 build proves device builds.
msrv:
	rustup toolchain install 1.75.0 --profile minimal
	cd rust && $(CARGO) +1.75.0 check --workspace --all-targets

## Harbour rejects, from sources (authority: `sfdk check -s harbour`,
## docs/HARBOUR.md).
##
## Last checks need `make fetch-server` and built binary; else says so.
harbour:
	./ci/harbour-check.sh
	./ci/harbour-check-selftest.sh

## Proves third_party/qmetaobject = upstream + one three-line patch. Needs
## network (fetches crates.io tarball).
vendor-check:
	./ci/vendor-check.sh

## Licences and advisories (CI `deny`). Needs cargo-deny, advisory DB.
##
## Missing tool = skip; finding = fail. `if`, not `||`: SKIP can't print over
## advisory. One shell line: `exit 0` ends only its own line.
deny:
	@if command -v cargo-deny >/dev/null 2>&1; then \
		cd rust && $(CARGO) deny check; \
	else \
		echo "deny: SKIP (cargo-deny not installed)"; \
	fi

## zizmor on .github/ workflows (template injection, leftover tokens, caches a
## release trusts). Offline, as CI gate.
##   cargo install --locked zizmor      (or: pipx install zizmor)
workflow-lint:
	@if command -v zizmor >/dev/null 2>&1; then \
		zizmor --offline .github/workflows/; \
	else \
		echo "workflow-lint: SKIP (zizmor not installed)"; \
	fi

## Every test under ASan and LSan, FFmpeg and C glue included. Slow; not in
## `check` (weekly in scheduled.yml). Needs/limits: ci/leak-check.sh.
leak-check:
	./ci/leak-check.sh

## Fetch the pinned upstream deltachat-rpc-server binaries (network).
fetch-server:
	./scripts/fetch-rpc-server.sh

## Catalogs to translations/*.qm. RPM does the same in %%build.
translations:
	./scripts/release-translations.sh

## Cover icons (icons/cover/*.svg) to qml/art/cover/*.png. Needs rsvg-convert.
## Output committed: build and CI don't run it.
cover-icons:
	./scripts/render-cover-icons.sh

## sonar-report.sh vs stub server (real service unreachable from CI).
sonar-report-test:
	./ci/sonar-report-selftest.sh

## ci/apt-install.sh keeps Ubuntu's apt sources; wrong = archive deleted.
apt-install-test:
	./ci/apt-install-selftest.sh

# sonar-reports: workspace minus third_party/, vendor/
# (sonar-project.properties).
SONAR_COV_ARGS = --workspace \
	--ignore-filename-regex '(^|/)(third_party|vendor)/' \
	--lcov --output-path target/sonar/lcov.info

## sonar-reports: coverage for SonarQube Cloud (lcov.info, js-lcov.info).
## Without it coverage reads 0.0%.
##
## nextest if installed, as `test` (cargo-llvm-cov otherwise runs one binary at
## a time). No `--doc`: cargo-llvm-cov skips doctests. `test` runs them.
##
## Also writes compile DB (piirit-video build.rs) for scanner's C analysis.
##
## Clippy not handed over: `make lint` already denies warnings, and gathering
## costs `cargo clean` + full rebuild. Sonar's own clippy off for another
## reason (sonar-project.properties).
##
## Opt-in (not in `check`):
##   rustup component add llvm-tools-preview
##   cargo install --locked cargo-llvm-cov
##   cargo install --locked cargo-nextest    (optional; serial without it)
sonar-reports:
	@echo "== coverage for SonarQube Cloud =="
	@command -v cargo-llvm-cov >/dev/null 2>&1 || { \
		echo "cargo-llvm-cov is not installed. Install it with:" >&2; \
		echo "    rustup component add llvm-tools-preview" >&2; \
		echo "    cargo install --locked cargo-llvm-cov" >&2; \
		exit 1; \
	}
	@command -v cargo-nextest >/dev/null 2>&1 || \
		echo "sonar-reports: cargo-nextest is not installed; running the slow way \
(cargo install --locked cargo-nextest)"
	@mkdir -p rust/target/sonar
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		cd rust && $(CARGO) llvm-cov nextest $(SONAR_COV_ARGS); \
	else \
		cd rust && $(CARGO) llvm-cov $(SONAR_COV_ARGS); \
	fi
	@echo "== wrote rust/target/sonar/lcov.info =="
	@db=$$(find rust/target -path '*/build/piirit-video-*/out/compile_commands.json' | head -n 1); \
	if [ -z "$$db" ]; then \
		echo "sonar-reports: piirit-video wrote no compile_commands.json" >&2; exit 1; \
	fi; \
	cp "$$db" rust/target/sonar/compile_commands.json
	@echo "== wrote rust/target/sonar/compile_commands.json =="
	@echo "== JavaScript coverage: the call page's bridge, under Node =="
	node --test --experimental-test-coverage \
		--test-reporter=lcov --test-reporter-destination=rust/target/sonar/js-lcov.info \
		--test-reporter=dot --test-reporter-destination=stdout \
		tests/js/*.test.js
	@echo "== wrote rust/target/sonar/js-lcov.info =="

## Real-core tests, offline. Needs `make fetch-server`.
integration:
	cd rust && DELTACHAT_RPC_SERVER=vendor/deltachat-rpc-server/x86_64/deltachat-rpc-server \
		$(CARGO) test -p deltachat-jsonrpc --test real_server -- --nocapture

clean:
	cd rust && $(CARGO) clean
