# `maps.xdc` provenance

`maps.xdc` here = map app: chat's locations, and everyone's, on OpenStreetMap. **Unmodified upstream zip**, compiled into app (`rust/piirit-shim/src/maps.rs`), handed to core as webxdc integration (`set_webxdc_integration`), run like any webxdc app.

- **Project:** deltachat/maps, maps integration Delta Chat clients run.
- **Source:** https://github.com/deltachat/maps, files at commit `a1d6d76` (zip holds `index.html`, `index.js`, `leaflet.js`, `leaflet.css`, `manifest.toml`, two pin images, byte for byte).
- **File:** deltachat-desktop's bundled build, `static/xdcs/maps.xdc` at commit `bb52bd1d61e8458749745ae93509cd380432c719`. 52,457 bytes, sha256 `38881287752faf943d4c9e6b66bb2be353e169d523ca2ac1c31e0a08d3f41476`. Upstream has no releases; Desktop's copy = what its users run. Release v2.65.0's copy lacks `?poi=` and `?contactId=`, both used here.
- **Build:** upstream `create-xdc.sh` (zip of repo files). Zip timestamps make bytes unreproducible; files inside compare equal to source commit.
- **Fetched by:** `scripts/fetch-maps-xdc.sh`, pins commit + checksum. `--check` fetches again, requires byte-for-byte match. CI runs it (`ci/vendor-check.sh`).

## Licences

- **deltachat/maps:** Mozilla Public License 2.0 (its `LICENSE`; zip carries none). File-level copyleft; shipped unmodified, source above. GPL-3.0-compatible (MPL-2.0 section 3.3). File installed with package so recipients told where.
- **Bundled:** [Leaflet](https://leafletjs.com) 1.9.4, BSD 2-Clause, (c) 2010-2023 Vladimir Agafonkin, (c) 2010-2011 CloudMade. Notice in `leaflet.js` head.
- **Map data:** © OpenStreetMap contributors, ODbL; app shows attribution.

## What Piirit adds around it

Nothing inside zip. App asks `https://tile.openstreetmap.org/{z}/{x}/{y}.png`; host (`rust/piirit-shim/src/webxdc_host.rs`) rewrites that address in served scripts, for apps core grants internet only, to own `/tile/` path behind chat token, fetched by core (`get_http_response`: core's HTTP cache, core's user agent, core's proxy). Page never reaches network itself.

## Updating

Bump `COMMIT` and `SHA256` in `scripts/fetch-maps-xdc.sh` together, run it, set `MAPS_VERSION` in `maps.rs` to same commit, update this file. Read upstream `index.js` changes first: tile address and `?poi=`/`?contactId=` = whole contract with Piirit.
