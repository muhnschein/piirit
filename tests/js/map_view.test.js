/*
 * Map integration's opening view (`map_view.js`, served after the webxdc API): the area
 * `maps.rs` asks for becomes the view the maps app restores at start.
 *
 * Run by `node --test tests/js/`, via rust/piirit-shim/tests/call_bridge_js.rs.
 */
"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const FILE = path.join(__dirname, "..", "..", "rust", "piirit-shim", "src", "map_view.js");
const CODE = fs.readFileSync(FILE, "utf8");

/** View stored after the script ran in a window `width` x `height` at `search`. */
function view(search, width, height, storage) {
    const stored = {};
    const window = {
        location: { search },
        innerWidth: width,
        innerHeight: height,
        localStorage: storage || {
            setItem(key, value) {
                stored[key] = String(value);
            },
        },
    };
    vm.runInNewContext(CODE, { window, URLSearchParams }, { filename: FILE });
    return stored;
}

test("no area: app keeps its own start", () => {
    assert.deepEqual(view("", 400, 800), {});
    assert.deepEqual(view("?poi=1,2", 400, 800), {});
    assert.deepEqual(view("?bounds=1,2,3", 400, 800), {});
    assert.deepEqual(view("?bounds=1,x,3,4", 400, 800), {});
});

test("one point: closest zoom, on it", () => {
    assert.deepEqual(view("?bounds=60.1,24.9,60.1,24.9", 400, 800), {
        "map.lat": "60.1",
        "map.lng": "24.9",
        "map.zoom": "18",
    });
});

test("Helsinki and Tampere: both in sight, as close as fits", () => {
    const stored = view("?bounds=60.16952,23.7574,61.5018,24.93545", 400, 800);
    // Zoom 8: ~214 px wide of 320, ~500 px high (Mercator stretch) of 640; zoom 9 overflows.
    assert.equal(stored["map.zoom"], "8");
    assert.ok(Math.abs(Number(stored["map.lat"]) - 60.84) < 0.01, stored["map.lat"]);
    assert.ok(Math.abs(Number(stored["map.lng"]) - 24.3464) < 0.001, stored["map.lng"]);
    // Narrower window: further out.
    assert.equal(view("?bounds=60.16952,23.7574,61.5018,24.93545", 150, 800)["map.zoom"], "7");
    assert.equal(view("?bounds=60.16952,23.7574,61.5018,24.93545", 60, 800)["map.zoom"], "5");
});

test("two points a step apart: no closer than the app's own closest zoom", () => {
    assert.equal(view("?bounds=60.1,24.9,60.10001,24.90001", 400, 800)["map.zoom"], "18");
});

test("tall area: middle on the map, not halfway in degrees", () => {
    // Equator to 80 N: Mercator's middle is ~57.0 N; halfway in degrees, 40 N, sits low.
    const stored = view("?bounds=0,10,80,20", 400, 800);
    assert.ok(Math.abs(Number(stored["map.lat"]) - 57.05) < 0.1, stored["map.lat"]);
});

test("whole world: never past zoom 1, poles clamped", () => {
    const stored = view("?bounds=-90,-180,90,180", 400, 800);
    assert.equal(stored["map.zoom"], "1");
    assert.equal(stored["map.lat"], "0");
});

test("unsized window and storage refused: no error", () => {
    // Phone-sized guess, 320 x 480.
    assert.equal(view("?bounds=60,24,61,25", 0, 0)["map.zoom"], "8");
    const refusing = {
        setItem() {
            throw new Error("denied");
        },
    };
    assert.doesNotThrow(() => view("?bounds=60,24,61,25", 400, 800, refusing));
});

/** File `name` from the zip at `file` (local headers; stored or deflated). */
function unzip(file, name) {
    const zip = fs.readFileSync(file);
    const zlib = require("node:zlib");
    let at = 0;
    while (zip.readUInt32LE(at) === 0x04034b50) {
        const method = zip.readUInt16LE(at + 8);
        const size = zip.readUInt32LE(at + 18);
        const nameLength = zip.readUInt16LE(at + 26);
        const extraLength = zip.readUInt16LE(at + 28);
        const start = at + 30 + nameLength + extraLength;
        const data = zip.subarray(start, start + size);
        if (zip.toString("utf8", at + 30, at + 30 + nameLength) === name) {
            return (method === 8 ? zlib.inflateRawSync(data) : data).toString("utf8");
        }
        at = start + size;
    }
    throw new Error(name + " not in " + file);
}

test("bundled maps app restores this view, after its own poi", () => {
    const xdc = path.join(__dirname, "..", "..", "vendor", "maps", "maps.xdc");
    const script = unzip(xdc, "index.js");
    const poi = script.indexOf("urlParams.get('poi')");
    const restore = script.indexOf(
        "map.setView([localStorage.getItem('map.lat'), localStorage.getItem('map.lng')], " +
            "localStorage.getItem('map.zoom'))"
    );
    assert.ok(poi >= 0 && restore > poi, "maps.xdc no longer restores map.lat/lng/zoom after poi");
});
