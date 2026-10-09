/*
 * The call page's bridge, run: `call_video.js` and `calls.js` together,
 * as the call host serves them, in front of a stand-in for the page and
 * the browser engine.
 *
 * What the engine does with a camera or a peer connection is a phone's
 * question. What is answered here is everything the bridge decides: what
 * the page is handed when it asks for the camera, which camera is opened
 * and with what limits, what goes on the peer connection's senders, which
 * of the page's switches is pressed, what is reported to the host, and
 * where the pictures are drawn.
 *
 * Run by `node --test tests/js/`, which rust/piirit-shim/tests/call_bridge_js.rs
 * does as part of the suite. Each test gets a fresh context, and runs the
 * two files in it under their own names, so coverage is measured on them.
 */
"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const SRC = path.join(__dirname, "..", "..", "rust", "piirit-shim", "src");
const FILES = ["call_video.js", "calls.js"].map((name) => {
    const file = path.join(SRC, name);
    return { file, code: fs.readFileSync(file, "utf8") };
});

/* ---- the engine's side ------------------------------------------------ */

class Track {
    constructor(kind, label, settings) {
        this.kind = kind;
        this.label = label;
        this.settings = settings || {};
        this.enabled = true;
        this.muted = false;
        this.readyState = "live";
        this.listeners = {};
        this.log = null;
    }
    stop() {
        this.readyState = "ended";
        if (this.log) {
            this.log.push("stop:" + this.label);
        }
    }
    addEventListener(name, fn) {
        (this.listeners[name] = this.listeners[name] || []).push(fn);
    }
    fire(name) {
        (this.listeners[name] || []).forEach((fn) => fn({ type: name }));
    }
}

class Stream {
    constructor(tracks) {
        this.tracks = tracks.slice();
    }
    getTracks() {
        return this.tracks.slice();
    }
    getVideoTracks() {
        return this.tracks.filter((t) => t.kind === "video");
    }
    getAudioTracks() {
        return this.tracks.filter((t) => t.kind === "audio");
    }
    addTrack(track) {
        if (this.tracks.indexOf(track) < 0) {
            this.tracks.push(track);
        }
    }
    removeTrack(track) {
        this.tracks = this.tracks.filter((t) => t !== track);
    }
}

class Element {
    constructor(tag) {
        this.tagName = tag.toUpperCase();
        this.children = [];
        this.parentElement = null;
        this.attributes = {};
        const props = {};
        this.style = {
            setProperty(name, value, priority) {
                props[name] = value + (priority ? " !" + priority : "");
            },
            props
        };
        this.textContent = "";
    }
    appendChild(child) {
        child.parentElement = this;
        this.children.push(child);
        return child;
    }
}

/* Everything a test reads afterwards. */
function makeWorld(search, options) {
    const opts = options || {};
    const log = [];
    const requests = [];
    const timers = [];
    const videos = [];
    const buttons = {};
    const classes = new Set();
    const connections = [];
    const head = new Element("head");
    const world = {
        log, requests, timers, videos, buttons, classes, connections, head,
        streams: [],
        cameraFails: opts.cameraFails || (() => false),
        cameraDelay: null
    };

    function camera(facing) {
        const track = new Track("video", "camera-" + facing, { facingMode: facing });
        track.log = log;
        return track;
    }

    const mediaDevices = {
        getUserMedia(asked) {
            log.push("gum:" + JSON.stringify(asked));
            if (asked.video) {
                const facing = asked.video.facingMode ? asked.video.facingMode.ideal : "user";
                if (world.cameraFails(facing)) {
                    return Promise.reject(new Error("NotReadableError"));
                }
                const given = new Stream([camera(facing)]);
                if (world.cameraDelay) {
                    return new Promise((resolve) => {
                        world.cameraDelay = () => resolve(given);
                    });
                }
                return Promise.resolve(given);
            }
            const mic = new Track("audio", "mic");
            mic.log = log;
            const given = new Stream([mic]);
            world.streams.push(given);
            return Promise.resolve(given);
        }
    };

    const document = {
        head,
        documentElement: {
            classList: {
                add: (name) => classes.add(name),
                remove: (name) => classes.delete(name)
            }
        },
        createElement(tag) {
            const element = new Element(tag);
            if (tag === "canvas") {
                element.getContext = () => ({ fillRect() {} });
                if (!opts.noCaptureStream) {
                    element.captureStream = (rate) => {
                        log.push("captureStream:" + rate);
                        return new Stream([new Track("video", "blank")]);
                    };
                }
            }
            return element;
        },
        querySelector(selector) {
            const found = /aria-label="([^"]+)"/.exec(selector);
            return found && buttons[found[1]] ? buttons[found[1]] : null;
        },
        getElementsByTagName(tag) {
            return tag === "video" ? videos : [];
        }
    };

    class Sender {
        constructor(track) {
            this.track = track;
            this.parameters = { encodings: [{}] };
        }
        replaceTrack(track) {
            log.push("replace:" + (track ? track.label : "null"));
            this.track = track;
            return Promise.resolve();
        }
        getParameters() {
            return JSON.parse(JSON.stringify(this.parameters));
        }
        setParameters(parameters) {
            log.push("set:" + JSON.stringify(parameters.encodings));
            this.parameters = parameters;
            return Promise.resolve();
        }
    }

    class Channel {
        constructor(label) {
            this.label = label;
            this.listeners = [];
        }
        addEventListener(name, fn) {
            if (name === "message") {
                this.listeners.push(fn);
            }
        }
        say(data) {
            this.listeners.forEach((fn) => fn({ data }));
        }
    }

    class PeerConnection {
        constructor(configuration) {
            this.configuration = configuration;
            this.transceivers = [];
            this.listeners = {};
            this.channels = {};
            this.signalingState = "have-local-offer";
            connections.push(this);
        }
        setConfiguration() {}
        addEventListener(name, fn) {
            (this.listeners[name] = this.listeners[name] || []).push(fn);
        }
        fire(name, event) {
            (this.listeners[name] || []).forEach((fn) => fn(event || {}));
        }
        addTrack(track) {
            const transceiver = {
                sender: new Sender(track),
                receiver: { track: new Track(track.kind, "remote-" + track.kind) },
                stopped: false
            };
            this.transceivers.push(transceiver);
            return transceiver.sender;
        }
        getTransceivers() {
            return this.transceivers;
        }
        getSenders() {
            return this.transceivers.map((t) => t.sender);
        }
        createDataChannel(label) {
            const channel = new Channel(label);
            this.channels[label] = channel;
            return channel;
        }
    }

    class XMLHttpRequest {
        open(method, url) {
            this.method = method;
            this.url = url;
        }
        setRequestHeader() {}
        send(body) {
            this.body = body;
            requests.push(this);
        }
    }

    const window = {
        location: { search, hash: "" },
        navigator: { mediaDevices },
        document,
        XMLHttpRequest,
        RTCPeerConnection: PeerConnection,
        Promise,
        JSON,
        setTimeout(fn) {
            timers.push(fn);
            return timers.length;
        }
    };
    window.window = window;
    world.window = window;
    world.context = vm.createContext(window);
    for (const { file, code } of FILES) {
        vm.runInContext(code, world.context, { filename: file });
    }
    return world;
}

/* Let promises settle. */
function settle() {
    return new Promise((resolve) => setImmediate(resolve));
}

async function settleAll() {
    for (let i = 0; i < 5; i++) {
        await settle();
    }
}

/* Run what is waiting on a timer, once round. */
function runTimers(world) {
    const due = world.timers.splice(0);
    due.forEach((fn) => fn());
}

/* What the bridge posted to `path`, parsed. */
function posts(world, route) {
    return world.requests
        .filter((r) => r.method === "POST" && r.url.endsWith(route))
        .map((r) => (route === "/video" ? JSON.parse(r.body) : r.body));
}

function lastVideo(world) {
    const all = posts(world, "/video");
    return all[all.length - 1];
}

/* Hand the bridge commands, as the host answers its long poll. */
async function command(world, ...commands) {
    const waiting = world.requests.filter(
        (r) => r.method === "GET" && r.url.endsWith("/commands") && !r.answered
    );
    assert.ok(waiting.length > 0, "the bridge is not listening for commands");
    const xhr = waiting[waiting.length - 1];
    xhr.answered = true;
    xhr.status = 200;
    xhr.responseText = JSON.stringify(commands);
    xhr.onload();
    await settleAll();
}

/*
 * The page, as far as the bridge sees it: it asks for the camera and the
 * microphone, puts its stream on a peer connection and on its own picture,
 * and draws the camera switch, which acts on its stream's video tracks as
 * the page's own does.
 */
async function page(world) {
    const connection = new world.window.RTCPeerConnection({ iceServers: [] });
    connection.createDataChannel("mutedState", { negotiated: true, id: 3 });
    const stream = await world.window.navigator.mediaDevices.getUserMedia({
        video: true,
        audio: true
    });
    stream.getTracks().forEach((track) => connection.addTrack(track, stream));

    const frame = new Element("div");
    const box = frame.appendChild(new Element("div"));
    const picture = box.appendChild(new Element("video"));
    picture.srcObject = stream;
    world.videos.push(picture);

    const cameraOn = world.window.location.search.indexOf("noOutgoingVideoInitially") < 0;
    const pageSwitch = { on: cameraOn };
    function draw() {
        delete world.buttons["Start camera"];
        delete world.buttons["Stop camera"];
        // The page draws its switch only over a stream with video.
        if (stream.getVideoTracks().length === 0) {
            return;
        }
        const label = pageSwitch.on ? "Stop camera" : "Start camera";
        world.buttons[label] = {
            click() {
                world.log.push("press:" + label);
                pageSwitch.on = !pageSwitch.on;
                stream.getVideoTracks().forEach((t) => (t.enabled = pageSwitch.on));
                draw();
            }
        };
    }
    draw();
    stream.getVideoTracks().forEach((t) => (t.enabled = pageSwitch.on));
    // A moment later: what the bridge looked for before the page had
    // drawn it, it looks for again.
    runTimers(world);
    await settleAll();
    return { connection, stream, picture, box, frame, pageSwitch };
}

function videoSender(connection) {
    return connection.transceivers.find((t) => t.receiver.track.kind === "video").sender;
}

const VOICE = "?noOutgoingVideoInitially&key=ab12";
const VIDEO = "?&key=ab12";

/* ---- the tests -------------------------------------------------------- */

test("a voice call opens no camera, and the page still has a video track", async () => {
    const world = makeWorld(VOICE);
    const { stream } = await page(world);

    const asked = world.log.filter((line) => line.startsWith("gum:"));
    assert.deepEqual(asked, ['gum:{"audio":true}'],
        "a voice call asked the engine for a camera");
    const video = stream.getVideoTracks();
    assert.equal(video.length, 1, "the page was given no video track to switch");
    assert.equal(video[0].label, "blank");
    assert.equal(video[0].enabled, false, "the blank track is not off");
    assert.ok(world.log.includes("captureStream:0"),
        "the blank track is a canvas that may draw frames");
    assert.deepEqual(lastVideo(world),
        { local: false, remote: false, front: true, failed: false });
});

test("a video call opens the front camera, held to VGA at 24 frames", async () => {
    const world = makeWorld(VIDEO);
    const { stream } = await page(world);

    const asked = world.log.filter((line) => line.startsWith("gum:"));
    assert.equal(asked[0], 'gum:{"audio":true}');
    const video = JSON.parse(asked[1].slice(4)).video;
    assert.deepEqual(video.facingMode, { ideal: "user" });
    assert.deepEqual(video.width, { ideal: 640 });
    assert.deepEqual(video.height, { ideal: 480 });
    assert.deepEqual(video.frameRate, { ideal: 24, max: 24 });
    assert.deepEqual(stream.getVideoTracks().map((t) => t.label), ["camera-user"]);
    assert.equal(lastVideo(world).local, true);
});

test("switched on in a voice call, the camera replaces the blank track everywhere", async () => {
    const world = makeWorld(VOICE);
    const { connection, stream, pageSwitch } = await page(world);

    await command(world, "camera-on");

    assert.deepEqual(stream.getVideoTracks().map((t) => t.label), ["camera-user"],
        "the page's stream does not carry the camera");
    assert.equal(videoSender(connection).track.label, "camera-user",
        "the sender does not carry the camera");
    assert.ok(world.log.includes("press:Start camera"),
        "the page was not told the camera is on");
    assert.equal(pageSwitch.on, true);
    assert.equal(stream.getVideoTracks()[0].enabled, true);
    assert.equal(lastVideo(world).local, true);
});

test("switched off, the camera is let go and the blank track goes back", async () => {
    const world = makeWorld(VIDEO);
    const { connection, stream, box } = await page(world);
    const live = stream.getVideoTracks()[0];

    await command(world, "camera-off");

    assert.ok(world.log.includes("press:Stop camera"),
        "the page was not told the camera is off");
    assert.equal(live.readyState, "ended", "the camera was left open");
    assert.equal(videoSender(connection).track.label, "blank");
    assert.deepEqual(stream.getVideoTracks().map((t) => t.label), ["blank"]);
    assert.equal(lastVideo(world).local, false);
    assert.equal(box.style.display, "none", "an empty picture is still drawn");
});

test("turned, the open camera is let go before the other is opened", async () => {
    const world = makeWorld(VIDEO);
    const { connection, picture } = await page(world);
    assert.equal(picture.style.transform, "scaleX(-1)",
        "the front camera's picture is not mirrored");
    world.log.length = 0;

    await command(world, "facing-environment");

    const stop = world.log.indexOf("stop:camera-user");
    const open = world.log.findIndex((line) => line.includes('"environment"'));
    assert.ok(stop >= 0 && open > stop,
        "the second camera was asked for while the first was open: " + world.log);
    assert.equal(videoSender(connection).track.label, "camera-environment");
    assert.equal(picture.style.transform, "none", "the back camera's picture is mirrored");
    assert.equal(lastVideo(world).front, false);

    await command(world, "facing-user");
    assert.equal(videoSender(connection).track.label, "camera-user");
    assert.equal(picture.style.transform, "scaleX(-1)");
    assert.equal(lastVideo(world).front, true);
});

test("a camera that will not turn falls back to the one it had", async () => {
    const world = makeWorld(VIDEO, { cameraFails: (facing) => facing === "environment" });
    const { connection } = await page(world);

    await command(world, "facing-environment");

    assert.equal(videoSender(connection).track.label, "camera-user");
    assert.deepEqual(lastVideo(world),
        { local: true, remote: false, front: true, failed: false });
});

test("a camera that will not open is reported, and the page told it is off", async () => {
    const world = makeWorld(VIDEO, { cameraFails: () => true });
    const { stream, pageSwitch } = await page(world);

    assert.deepEqual(stream.getVideoTracks().map((t) => t.label), ["blank"],
        "the call has no video track to switch on later");
    assert.equal(lastVideo(world).failed, true, "the failure was not reported");
    assert.equal(lastVideo(world).local, false);
    assert.ok(world.log.includes("press:Stop camera"));
    assert.equal(pageSwitch.on, false);
});

test("a camera that will not open says so every time it is asked for", async () => {
    const world = makeWorld(VOICE, { cameraFails: () => true });
    await page(world);

    await command(world, "camera-on");
    assert.equal(lastVideo(world).failed, true, "the first failure was not reported");
    const before = posts(world, "/video").length;
    await command(world, "camera-on");
    const after = posts(world, "/video").slice(before);
    assert.ok(after.some((said) => said.failed === false)
              && after[after.length - 1].failed === true,
        "a second failure is not news to the app: " + JSON.stringify(after));
});

test("a camera switched off while it opens is let go when it does", async () => {
    const world = makeWorld(VOICE);
    const { connection } = await page(world);
    world.cameraDelay = true;

    await command(world, "camera-on");
    const finish = world.cameraDelay;
    await command(world, "camera-off");
    world.cameraDelay = null;
    finish();
    await settleAll();

    assert.ok(world.log.includes("stop:camera-user"), "the late camera was left open");
    assert.equal(videoSender(connection).track.label, "blank");
    assert.equal(lastVideo(world).local, false);
});

test("the other end's picture is reported while it comes and they have it on", async () => {
    const world = makeWorld(VOICE);
    const { connection } = await page(world);
    const theirs = new Track("video", "theirs");
    connection.fire("track", { track: theirs, streams: [new Stream([theirs])] });
    assert.equal(lastVideo(world).remote, true, "their picture was not reported");

    connection.channels.mutedState.say(JSON.stringify({ audioEnabled: true, videoEnabled: false }));
    assert.equal(lastVideo(world).remote, false, "a camera they switched off still counts");

    connection.channels.mutedState.say(JSON.stringify({ audioEnabled: true, videoEnabled: true }));
    assert.equal(lastVideo(world).remote, true);

    theirs.muted = true;
    theirs.fire("mute");
    assert.equal(lastVideo(world).remote, false, "a picture with no frames still counts");

    connection.channels.mutedState.say("not json");
    assert.equal(lastVideo(world).remote, false);
});

test("negotiated, every video sender is held to the bitrate and frame caps", async () => {
    const world = makeWorld(VIDEO);
    const { connection } = await page(world);
    world.log.length = 0;
    connection.signalingState = "stable";
    connection.fire("signalingstatechange");

    const sets = world.log.filter((line) => line.startsWith("set:"));
    assert.deepEqual(sets, ['set:[{"maxBitrate":700000,"maxFramerate":24}]'],
        "the video sender, and only it, was not capped: " + sets);
});

test("this end's picture fills the screen until theirs comes, then a corner", async () => {
    const world = makeWorld(VIDEO);
    const { connection, box, frame } = await page(world);
    assert.equal(box.style.width, "100%");
    assert.equal(box.style.top, "0");
    assert.equal(box.style.display, "block");
    assert.equal(frame.style.props.display, "block !important",
        "the picture is not shown before the call is up");

    const theirs = new Track("video", "theirs");
    connection.fire("track", { track: theirs, streams: [new Stream([theirs])] });
    assert.equal(box.style.width, "27vw");
    assert.equal(box.style.top, "10vh");
    assert.equal(box.style.right, "4vw");
});

test("in the background the pictures are not drawn", async () => {
    const world = makeWorld(VOICE);
    await page(world);
    await command(world, "hidden");
    assert.ok(world.classes.has("piirit-hidden"));
    await command(world, "shown");
    assert.ok(!world.classes.has("piirit-hidden"));
    const sheet = world.head.children.find((child) => child.tagName === "STYLE");
    assert.ok(sheet && /html\.piirit-hidden video/.test(sheet.textContent));
    assert.ok(/body \* \{ visibility: hidden !important; \}/.test(sheet.textContent),
        "the page's own controls are drawn");
});

test("the page's other commands still reach it, and the microphone its switch", async () => {
    const world = makeWorld(VOICE);
    await page(world);
    let pressed = 0;
    world.buttons["Mute microphone"] = { click: () => (pressed += 1) };

    await command(world, "camera-on", "onAnswer=abc", "mute");

    assert.equal(world.window.location.hash, "onAnswer=abc",
        "a hash command was taken for the camera's");
    assert.equal(pressed, 1, "the microphone's switch was not pressed");
});

test("a page drawn late still gets its switch pressed and its picture placed", async () => {
    const world = makeWorld(VOICE);
    const connection = new world.window.RTCPeerConnection({});
    const stream = await world.window.navigator.mediaDevices.getUserMedia({
        video: true, audio: true
    });
    stream.getTracks().forEach((track) => connection.addTrack(track, stream));
    await settleAll();
    await command(world, "camera-on");
    assert.ok(world.timers.length > 0, "nothing looks again for the switch");

    world.buttons["Start camera"] = { click: () => world.log.push("press:late") };
    const box = new Element("div");
    const picture = box.appendChild(new Element("video"));
    picture.srcObject = stream;
    world.videos.push(picture);
    runTimers(world);

    assert.ok(world.log.includes("press:late"), "the late switch was never pressed");
    assert.equal(box.style.display, "block", "the late picture was never placed");
});

test("without a canvas to stand in, a voice call goes ahead without video", async () => {
    const world = makeWorld(VOICE, { noCaptureStream: true });
    const { stream } = await page(world);
    assert.equal(stream.getVideoTracks().length, 0);
    assert.deepEqual(lastVideo(world),
        { local: false, remote: false, front: true, failed: false });
    // No switch drawn, and none ever will be: nothing keeps looking.
    world.timers.length = 0;
    await command(world, "camera-off");
    assert.equal(world.timers.length, 0, "the bridge looks forever for a switch");
    // Not asked for the camera: the page's own audio-only retry.
    const audio = await world.window.navigator.mediaDevices.getUserMedia({ audio: true });
    assert.equal(audio.getVideoTracks().length, 0);
});
