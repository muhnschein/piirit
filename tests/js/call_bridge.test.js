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
    getSettings() {
        return this.settings;
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
        this.listeners = {};
    }
    appendChild(child) {
        child.parentElement = this;
        this.children.push(child);
        return child;
    }
    addEventListener(name, fn, options) {
        (this.listeners[name] = this.listeners[name] || []).push({ fn, once: Boolean(options && options.once) });
    }
    fire(name) {
        const all = this.listeners[name] || [];
        this.listeners[name] = all.filter((one) => !one.once);
        all.forEach((one) => one.fn({ type: name }));
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
        windowListeners: {},
        documentListeners: {},
        cameraFails: opts.cameraFails || (() => false),
        cameraDelay: null
    };

    // The phone's cameras, as the engine lists them. By default as
    // Gecko on a phone was seen to: names that say nothing of which way
    // a camera faces, a facingMode weighed at nothing (the first camera
    // opens whatever is asked), and no facingMode in a track's settings.
    const cameras = opts.cameras || [
        { deviceId: "cam-a", label: "camera 0", facing: "user" },
        { deviceId: "cam-b", label: "camera 1", facing: "environment" },
    ];
    function camera(one) {
        const settings = { deviceId: one.deviceId, width: 640, height: 480, frameRate: 30 };
        if (opts.reportsFacing) {
            settings.facingMode = one.facing;
        }
        const track = new Track("video", "camera-" + one.facing, settings);
        track.log = log;
        return track;
    }
    function pick(video) {
        if (typeof video === "object" && video.deviceId && video.deviceId.exact) {
            return cameras.find((one) => one.deviceId === video.deviceId.exact) || null;
        }
        const wanted = typeof video === "object" && video.facingMode ? video.facingMode.ideal : "";
        const facing = opts.honoursFacing && cameras.find((one) => one.facing === wanted);
        return facing || cameras[0];
    }

    // What a phone camera can do, as Gecko weighs it: a mode's size, and
    // the most frames it runs at. A required value -- min, max, exact --
    // that no mode meets opens nothing.
    const MODES = [
        { width: 640, height: 480, frameRate: 30 },
        { width: 1280, height: 720, frameRate: 30 },
    ];
    function meets(mode, video) {
        return ["width", "height", "frameRate"].every((key) => {
            const asked = video[key];
            if (typeof asked !== "object" || asked === null) {
                return true;
            }
            const have = mode[key];
            return !(asked.min > have || asked.max < have
                     || (asked.exact !== undefined && asked.exact !== have));
        });
    }
    function refusal(name, message) {
        const err = new Error(message);
        err.name = name;
        return err;
    }

    const mediaDevices = {
        getUserMedia(asked) {
            log.push("gum:" + JSON.stringify(asked));
            if (asked.video) {
                const one = pick(asked.video);
                if (!one || (typeof asked.video === "object"
                    && !MODES.some((mode) => meets(mode, asked.video)))) {
                    return Promise.reject(refusal("OverconstrainedError", "Constraints could be not satisfied."));
                }
                if (world.cameraFails(one.facing)) {
                    return Promise.reject(refusal("NotReadableError", "Failed to allocate videosource"));
                }
                const given = new Stream([camera(one)]);
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
        },
        enumerateDevices() {
            log.push("enumerate");
            return Promise.resolve([{ kind: "audioinput", deviceId: "mic", label: "mic" }]
                .concat(cameras.map((one) => ({
                    kind: "videoinput", deviceId: one.deviceId, label: one.label
                }))));
        }
    };

    const document = {
        head,
        visibilityState: opts.visibilityState,
        addEventListener(name, fn, capture) {
            (world.documentListeners[name] = world.documentListeners[name] || []).push({ fn, capture });
        },
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
            // A POST is answered at once, unless the test holds the host
            // back; a GET waits for the test to answer it.
            if (this.method === "POST" && !world.holdPosts) {
                this.done();
            }
        }
        done() {
            this.status = 204;
            if (this.onloadend) {
                this.onloadend();
            }
        }
    }

    // An engine's media element, whose srcObject is an accessor on its
    // prototype, as Gecko's is.
    function HTMLMediaElement() {}
    Object.defineProperty(HTMLMediaElement.prototype, "srcObject", {
        configurable: true,
        enumerable: true,
        get() { return this.held; },
        set(value) { this.held = value; }
    });
    world.newVideo = () => {
        const element = new Element("video");
        Object.setPrototypeOf(element, Object.assign(Object.create(HTMLMediaElement.prototype), {
            appendChild: Element.prototype.appendChild
        }));
        return element;
    };

    const window = {
        location: { search, hash: "" },
        navigator: { mediaDevices },
        document,
        XMLHttpRequest,
        RTCPeerConnection: PeerConnection,
        HTMLMediaElement: opts.mediaElement ? HTMLMediaElement : undefined,
        Promise,
        JSON,
        console: {
            log(...words) { world.console.push(["log", ...words]); },
            warn(...words) { world.console.push(["warn", ...words]); },
            error(...words) { world.console.push(["error", ...words]); }
        },
        setTimeout(fn, delay) {
            // Run when the test says, whatever the delay; which is kept.
            const timer = () => fn();
            timer.delay = delay;
            timers.push(timer);
            return timers.length;
        },
        addEventListener(name, fn) {
            (world.windowListeners[name] = world.windowListeners[name] || []).push(fn);
        }
    };
    window.window = window;
    world.window = window;
    world.console = [];
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
        { local: false, remote: false, localPainted: false, remotePainted: false, front: true, failed: false });
});

test("a video call opens the front camera, asked for VGA at 24 frames and nothing it must meet", async () => {
    const world = makeWorld(VIDEO);
    const { stream } = await page(world);

    const asked = world.log.filter((line) => line.startsWith("gum:"));
    assert.equal(asked[0], 'gum:{"audio":true}');
    const video = JSON.parse(asked[1].slice(4)).video;
    assert.deepEqual(video.facingMode, { ideal: "user" });
    assert.deepEqual(video.width, { ideal: 640 });
    assert.deepEqual(video.height, { ideal: 480 });
    assert.deepEqual(video.frameRate, { ideal: 24 });
    assert.deepEqual(stream.getVideoTracks().map((t) => t.label), ["camera-user"],
        "a camera that runs at 30 frames did not open");
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

test("turned, the open camera is let go before the other is opened, by its id", async () => {
    const world = makeWorld(VIDEO);
    const { connection, picture } = await page(world);
    assert.equal(picture.style.transform, "scaleX(-1)",
        "the front camera's picture is not mirrored");
    world.log.length = 0;

    await command(world, "facing-environment");

    const stop = world.log.indexOf("stop:camera-user");
    const open = world.log.findIndex((line) => line.startsWith("gum:") && line.includes('"cam-b"'));
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

test("through a turn the picture keeps its mirroring until the other camera is in place", async () => {
    const world = makeWorld(VIDEO);
    const { picture } = await page(world);
    world.cameraDelay = true;

    await command(world, "facing-environment");
    assert.equal(picture.style.transform, "scaleX(-1)",
        "the front camera's last frame flipped round before the turn");
    assert.equal(lastVideo(world).front, false, "the app was not told of the turn at once");

    const finish = world.cameraDelay;
    world.cameraDelay = null;
    finish();
    await settleAll();
    assert.equal(picture.style.transform, "none", "the back camera's picture is mirrored");
});

test("a camera that will not turn falls back to the one it had", async () => {
    const world = makeWorld(VIDEO, { cameraFails: (facing) => facing === "environment" });
    const { connection } = await page(world);

    await command(world, "facing-environment");

    assert.equal(videoSender(connection).track.label, "camera-user");
    assert.deepEqual(lastVideo(world),
        { local: true, remote: false, localPainted: false, remotePainted: false, front: true, failed: false });
});

test("a camera that will not open is reported, and the page told it is off", async () => {
    const world = makeWorld(VIDEO, { cameraFails: () => true });
    const { stream, pageSwitch } = await page(world);

    assert.deepEqual(stream.getVideoTracks().map((t) => t.label), ["blank"],
        "the call has no video track to switch on later");
    assert.equal(lastVideo(world).failed, true, "the failure was not reported");
    assert.equal(lastVideo(world).error, "NotReadableError: Failed to allocate videosource",
        "the app is not told why the camera would not open");
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

    // Half a word, which the page itself rejects: not taken here either.
    connection.channels.mutedState.say(JSON.stringify({ videoEnabled: false }));
    assert.equal(lastVideo(world).remote, true, "a word the page rejects was taken");
    assert.deepEqual(posts(world, "/log").filter((line) => line.startsWith("their camera:")),
        ["their camera: off, they say", "their camera: on, they say"]);

    theirs.muted = true;
    theirs.fire("mute");
    assert.equal(lastVideo(world).remote, false, "a picture with no frames still counts");

    connection.channels.mutedState.say("not json");
    assert.equal(lastVideo(world).remote, false);
});

test("what the page says of the pictures and its commands is logged, and nothing else", async () => {
    const world = makeWorld(VIDEO);
    const { connection } = await page(world);
    const said = () => posts(world, "/log").filter((line) => line.startsWith("page"));
    const { console } = world.window;
    console.log("incStream.videoTracks() muted states:", [true]);
    console.log("sending ICE candidate to remote peer", { candidate: "candidate:1 1 udp 1 192.0.2.1 9 typ host" });
    console.log("a candidate, not about incoming media at all");
    console.log("incoming media not playing, will .play() it");
    console.log("incoming media not playing, will .play() it");
    console.log("incoming media is playing");
    console.warn("cannot set source for fallback audio element: element is not mounted");
    console.error("remote sent an invalid mutedState message", { videoEnabled: 1 });
    const cyclic = {};
    cyclic.self = cyclic;
    console.error("odd", cyclic);
    world.windowListeners.unhandledrejection[0]({ reason: new TypeError("no answer") });
    world.windowListeners.error[0]({});
    assert.deepEqual(said(), [
        "page: incStream.videoTracks() muted states: [true]",
        "page: incoming media not playing, will .play() it",
        "page: incoming media is playing",
        "page warn: cannot set source for fallback audio element: element is not mounted",
        "page error: remote sent an invalid mutedState message {\"videoEnabled\":1}",
        "page error: odd [object Object]",
        "page error: TypeError: no answer",
        "page error: an error"
    ]);
    assert.equal(world.console.length, 9, "the page's console no longer says what it is told");
    assert.deepEqual(world.console[1].slice(0, 2), ["log", "sending ICE candidate to remote peer"]);

    // Where the call stands, in order with the rest.
    connection.signalingState = "have-local-offer";
    connection.fire("signalingstatechange");
    assert.equal(posts(world, "/log").at(-1), "signalling: have-local-offer");
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
    assert.deepEqual([box.style.left, box.style.top, box.style.right], ["auto", "10vh", "4vw"],
        "the picture is not 4% in from the right and 10% down");

    // Theirs gone: the whole screen. Back: the same corner.
    theirs.muted = true;
    theirs.fire("mute");
    assert.deepEqual([box.style.left, box.style.top, box.style.right, box.style.width],
        ["0", "0", "0", "100%"]);
    theirs.muted = false;
    theirs.fire("unmute");
    assert.deepEqual([box.style.left, box.style.top, box.style.right, box.style.width],
        ["auto", "10vh", "4vw", "27vw"]);
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
        { local: false, remote: false, localPainted: false, remotePainted: false, front: true, failed: false });
    // No switch drawn, and none ever will be: nothing keeps looking.
    world.timers.length = 0;
    await command(world, "camera-off");
    assert.equal(world.timers.length, 0, "the bridge looks forever for a switch");
    // Not asked for the camera: the page's own audio-only retry.
    const audio = await world.window.navigator.mediaDevices.getUserMedia({ audio: true });
    assert.equal(audio.getVideoTracks().length, 0);
});

test("a call placed here offers VP8 first, its retransmissions beside it", async () => {
    const world = makeWorld(VIDEO);
    await page(world);
    // As Sailfish's Gecko writes an offer with hardware H264 on: H264
    // first, VP8 after, each with its retransmission type.
    const offer = [
        "v=0",
        "m=audio 9 UDP/TLS/RTP/SAVPF 109 9 0 8 101",
        "a=rtpmap:109 opus/48000/2",
        "m=video 9 UDP/TLS/RTP/SAVPF 126 127 97 98 120 124 121 125 123 122 119",
        "a=rtpmap:126 H264/90000",
        "a=rtpmap:97 H264/90000",
        "a=rtpmap:120 VP8/90000",
        "a=rtpmap:121 VP9/90000",
        "a=rtpmap:123 ulpfec/90000",
        "a=rtpmap:122 red/90000",
        "a=rtpmap:127 rtx/90000",
        "a=fmtp:127 apt=126",
        "a=rtpmap:98 rtx/90000",
        "a=fmtp:98 apt=97",
        "a=rtpmap:124 rtx/90000",
        "a=fmtp:124 apt=120",
        "a=rtpmap:125 rtx/90000",
        "a=fmtp:125 apt=121",
        "a=rtpmap:119 rtx/90000",
        "a=fmtp:119 apt=122",
        "",
    ].join("\r\n");
    world.window.calls.startCall(offer);

    const sent = posts(world, "/start");
    assert.equal(sent.length, 1, "the offer was not sent");
    const lines = sent[0].split("\r\n");
    assert.equal(lines.find((line) => line.startsWith("m=video")),
        "m=video 9 UDP/TLS/RTP/SAVPF 120 124 126 127 97 98 121 125 123 122 119",
        "VP8 is not what the other end is asked for first");
    assert.equal(lines.find((line) => line.startsWith("m=audio")),
        "m=audio 9 UDP/TLS/RTP/SAVPF 109 9 0 8 101", "the voice's codecs moved");
    assert.deepEqual(lines.filter((line) => !line.startsWith("m=")),
        offer.split("\r\n").filter((line) => !line.startsWith("m=")),
        "more of the offer changed than the order of its picture's codecs");

    // An offer with no VP8 in it goes as it is.
    const noVp8 = "v=0\r\nm=video 9 UDP/TLS/RTP/SAVPF 126 97\r\na=rtpmap:126 H264/90000\r\n";
    world.window.calls.startCall(noVp8);
    assert.equal(posts(world, "/start")[1], noVp8);
});

test("a camera whose name says which way it faces is the one opened for that way", async () => {
    const world = makeWorld(VIDEO, {
        cameras: [
            { deviceId: "cam-r", label: "Back camera", facing: "environment" },
            { deviceId: "cam-f", label: "Front camera", facing: "user" },
        ],
    });
    const { connection, picture } = await page(world);
    assert.equal(videoSender(connection).track.label, "camera-user",
        "the front camera was not the one opened first");
    assert.equal(picture.style.transform, "scaleX(-1)");

    await command(world, "facing-environment");
    assert.equal(videoSender(connection).track.label, "camera-environment");
    await command(world, "facing-user");
    assert.equal(videoSender(connection).track.label, "camera-user",
        "turned back, the front camera was not found by its name");
});

test("a turn logs how long the other camera took to open, and to be drawn", async () => {
    const world = makeWorld(VIDEO);
    const { picture } = await page(world);
    assert.ok(posts(world, "/log").some((line) => line === "camera open: camera-user 640x480@30"),
        "the camera the call opened with is timed as a turn: " + posts(world, "/log"));
    // The page's picture loading its first stream is not a turn either.
    picture.fire("loadeddata");
    assert.ok(!posts(world, "/log").some((line) => line.startsWith("camera drawn")),
        "a picture drawn with no turn was timed: " + posts(world, "/log"));

    await command(world, "facing-environment");
    assert.ok(posts(world, "/log").some((line) =>
        /^camera open: camera-environment 640x480@30, \d+ ms after the turn$/.test(line)),
    "the turned-to camera's opening is not timed: " + posts(world, "/log"));
    picture.fire("loadeddata");
    picture.fire("loadeddata");
    const drawn = posts(world, "/log").filter((line) => line.startsWith("camera drawn"));
    assert.equal(drawn.length, 1, "its first picture is not timed, once: " + drawn);
    assert.match(drawn[0], /^camera drawn: \d+ ms after the turn$/);

    // Switched on after being off, the camera has not been turned.
    await command(world, "camera-off");
    await command(world, "camera-on");
    picture.fire("loadeddata");
    const opens = posts(world, "/log").filter((line) => line.startsWith("camera open"));
    assert.match(opens[opens.length - 1], /^camera open: camera-\w+ 640x480@30$/,
        "a camera switched back on is timed as a turn");
    assert.equal(posts(world, "/log").filter((line) => line.startsWith("camera drawn")).length, 1);
});

test("a turn that fails, or ends switched off, leaves nothing timed for later", async () => {
    const world = makeWorld(VIDEO, { cameraFails: (facing) => facing === "environment" });
    const { picture } = await page(world);
    await command(world, "facing-environment");
    picture.fire("loadeddata");
    assert.ok(!posts(world, "/log").some((line) => line.includes("after the turn")),
        "the camera it fell back to is timed as the turn's: " + posts(world, "/log"));

    const slow = makeWorld(VIDEO);
    const shown = await page(slow);
    slow.cameraDelay = true;
    await command(slow, "facing-environment");
    await command(slow, "camera-off");
    slow.cameraDelay();
    await settleAll();
    slow.cameraDelay = null;
    await command(slow, "camera-on");
    shown.picture.fire("loadeddata");
    const opens = posts(slow, "/log").filter((line) => line.startsWith("camera open"));
    assert.match(opens[opens.length - 1], /^camera open: camera-\w+ 640x480@30$/,
        "a camera switched on later is timed from a turn switched off");
    assert.ok(!posts(slow, "/log").some((line) => line.startsWith("camera drawn")),
        "a camera switched on later is drawn as a turn's: " + posts(slow, "/log"));
});

test("the cameras are listed until the engine names them, then not again on every turn", async () => {
    const world = makeWorld(VIDEO);
    await page(world);
    await command(world, "facing-environment");
    await command(world, "facing-user");
    assert.equal(world.log.filter((line) => line === "enumerate").length, 1,
        "named cameras were listed again: " + world.log);

    const unnamed = makeWorld(VIDEO, {
        cameras: [
            { deviceId: "cam-a", label: "", facing: "user" },
            { deviceId: "cam-b", label: "", facing: "environment" },
        ],
    });
    const { connection } = await page(unnamed);
    await command(unnamed, "facing-environment");
    await command(unnamed, "facing-user");
    assert.equal(unnamed.log.filter((line) => line === "enumerate").length, 3,
        "cameras with no names yet were kept as they were: " + unnamed.log);
    assert.equal(videoSender(connection).track.label, "camera-user");
});

test("the back camera opened is the one the host names, and the log says which", async () => {
    const cameras = [
        { deviceId: "cam-f", label: "Droid front camera", facing: "user" },
        { deviceId: "cam-w", label: "Droid rear camera", facing: "environment" },
        { deviceId: "cam-m", label: "Droid rear camera", facing: "environment" },
    ];
    const opened = (world) => world.log.filter((line) => line.startsWith("gum:{\"video\""))
        .map((line) => JSON.parse(line.slice(4)).video.deviceId.exact);
    const opens = (world) => posts(world, "/log").filter((line) => line.startsWith("camera open"));

    const told = makeWorld(VIDEO, { cameras });
    await page(told);
    await command(told, "back-camera-1");
    await command(told, "facing-environment");
    assert.deepEqual(opened(told), ["cam-f", "cam-m"], "the main lens was not the one opened");
    assert.match(opens(told)[0], /^camera open: camera-user 640x480@30$/,
        "the one front camera is numbered");
    assert.match(opens(told)[1], /^camera open: camera-environment 640x480@30 \(2 of 2\), \d+ ms after the turn$/);
    assert.equal(told.window.location.hash, "", "the lens was passed on to the page");
    await command(told, "facing-user");
    assert.match(opens(told)[2], /^camera open: camera-user 640x480@30, \d+ ms after the turn$/,
        "the back lens's number was said of the front camera");

    // The host's lens is a back one: the front camera is the first.
    const fronts = makeWorld(VIDEO, { cameras: [
        { deviceId: "cam-f", label: "Droid front camera", facing: "user" },
        { deviceId: "cam-g", label: "Droid front camera", facing: "user" },
        ...cameras.slice(1),
    ] });
    await page(fronts);
    await command(fronts, "back-camera-1", "camera-off", "camera-on");
    assert.equal(opened(fronts).at(-1), "cam-f");

    // Told nothing, the first the engine lists; told of one past the
    // last, the last.
    const untold = makeWorld(VIDEO, { cameras });
    await page(untold);
    await command(untold, "facing-environment");
    assert.equal(opened(untold)[1], "cam-w");
    assert.match(opens(untold)[1], / \(1 of 2\), /);
    const past = makeWorld(VIDEO, { cameras });
    await page(past);
    await command(past, "back-camera-5", "facing-environment");
    assert.equal(opened(past)[1], "cam-m");
});

/* How their picture is laid out: its size, and where, against the box
 * the page gives it. */
function zoomOf(picture) {
    const { position, width, height, left, top } = picture.style;
    return [position, width, height, left, top].join(" ");
}

test("their picture is enlarged towards filling the screen, by no more than a third", async () => {
    const world = makeWorld(VIDEO);
    const { connection, picture: ours } = await page(world);
    const holder = new Element("div");
    const picture = holder.appendChild(new Element("video"));
    const theirs = new Track("video", "theirs");
    picture.srcObject = new Stream([theirs]);
    Object.assign(picture, { videoWidth: 0, videoHeight: 0 });
    Object.assign(holder, { clientWidth: 540, clientHeight: 1200 });
    world.videos.push(picture);

    // No frame yet, so no size to go by.
    connection.fire("track", { track: theirs, streams: [picture.srcObject] });
    assert.equal(zoomOf(picture), "relative 100.0% 100.0% 0.0% 0.0%");

    // An upright phone's picture on a taller screen: filled, as ours is,
    // around its middle.
    Object.assign(picture, { videoWidth: 720, videoHeight: 1280 });
    picture.fire("resize");
    assert.equal(zoomOf(picture), "relative 125.0% 125.0% -12.5% -12.5%");
    assert.equal(holder.style.overflow, "hidden", "what is enlarged past the screen is not cut");
    assert.equal(picture.style.transform, undefined,
        "their picture is scaled: one more layer for the engine to put together");
    assert.equal(ours.style.transform, "scaleX(-1)", "our own picture was fitted as theirs");
    assert.equal(ours.style.width, undefined, "our own picture was fitted as theirs");

    // Held sideways: some of its bars go, not half of it.
    Object.assign(picture, { videoWidth: 1280, videoHeight: 720 });
    picture.fire("resize");
    assert.equal(zoomOf(picture), "relative 135.0% 135.0% -17.5% -17.5%");

    // The screen's own shape: nothing to enlarge.
    Object.assign(picture, { videoWidth: 540, videoHeight: 1200 });
    picture.fire("resize");
    assert.equal(zoomOf(picture), "relative 100.0% 100.0% 0.0% 0.0%");

    // Nearly the screen's shape: not enlarged by a pixel's worth either.
    Object.assign(picture, { videoWidth: 1079, videoHeight: 2400 });
    picture.fire("resize");
    assert.equal(zoomOf(picture), "relative 100.0% 100.0% 0.0% 0.0%");

    // The phone turned to landscape, with an upright picture.
    Object.assign(picture, { videoWidth: 720, videoHeight: 1280 });
    Object.assign(holder, { clientWidth: 1200, clientHeight: 540 });
    world.windowListeners.resize.forEach((fn) => fn());
    assert.equal(zoomOf(picture), "relative 135.0% 135.0% -17.5% -17.5%");
});

test("their picture is fitted the moment the page gives it its stream", async () => {
    const world = makeWorld(VIDEO, { mediaElement: true });
    await page(world);
    const holder = new Element("div");
    const picture = holder.appendChild(world.newVideo());
    Object.assign(picture, { videoWidth: 720, videoHeight: 1280 });
    Object.assign(holder, { clientWidth: 540, clientHeight: 1200 });
    world.videos.push(picture);
    picture.srcObject = new Stream([new Track("video", "theirs")]);
    runTimers(world);
    assert.equal(zoomOf(picture), "relative 125.0% 125.0% -12.5% -12.5%");
});

test("what the camera says it faces is what is mirrored and reported", async () => {
    // One camera, on the back, that says so: asked for the front, the
    // back is what there is.
    const world = makeWorld(VIDEO, {
        reportsFacing: true,
        cameras: [{ deviceId: "cam-x", label: "camera 0", facing: "environment" }],
    });
    const { picture } = await page(world);
    assert.equal(lastVideo(world).front, false, "the back camera was reported as the front");
    assert.equal(picture.style.transform, "none", "the back camera's picture is mirrored");
});

test("a picture the page draws late is placed, however often it was looked for", async () => {
    const world = makeWorld(VIDEO);
    const connection = new world.window.RTCPeerConnection({});
    const channel = connection.createDataChannel("mutedState", { negotiated: true, id: 3 });
    const stream = await world.window.navigator.mediaDevices.getUserMedia({
        video: true, audio: true
    });
    stream.getTracks().forEach((track) => connection.addTrack(track, stream));
    await settleAll();
    // The other end says things about its camera while this end's camera
    // is still on its way to the page's picture: each is a report, and
    // each report looks for the picture.
    for (let i = 0; i < 8; i++) {
        channel.say(JSON.stringify({ audioEnabled: true, videoEnabled: i % 2 === 0 }));
    }
    for (let round = 0; round < 8; round++) {
        runTimers(world);
    }
    const box = new Element("div");
    const picture = box.appendChild(new Element("video"));
    picture.srcObject = stream;
    world.videos.push(picture);
    runTimers(world);

    assert.equal(box.style.display, "block",
        "the looks ran out before a picture drawn two seconds late");
});

test("the moment the page puts the stream on its picture, it is placed", async () => {
    const world = makeWorld(VIDEO, { mediaElement: true });
    const connection = new world.window.RTCPeerConnection({});
    const stream = await world.window.navigator.mediaDevices.getUserMedia({
        video: true, audio: true
    });
    stream.getTracks().forEach((track) => connection.addTrack(track, stream));
    await settleAll();
    // Long after every look has been spent.
    for (let round = 0; round < 60; round++) {
        runTimers(world);
    }
    const box = new Element("div");
    const picture = box.appendChild(world.newVideo());
    world.videos.push(picture);
    picture.srcObject = stream;
    assert.equal(picture.held, stream, "the page's own srcObject was not kept");
    runTimers(world);

    assert.equal(box.style.display, "block", "the picture was not placed when it was drawn");
});

test("what only a phone shows is written to the app's log", async () => {
    const world = makeWorld(VIDEO);
    const { connection } = await page(world);
    assert.ok(posts(world, "/log").some((line) => /^camera open: camera-user 640x480@30/.test(line)),
        "the camera that opened is not logged: " + posts(world, "/log"));

    connection.localDescription = { sdp: [
        "v=0",
        "m=audio 9 UDP/TLS/RTP/SAVPF 109",
        "a=rtpmap:109 opus/48000/2",
        "m=video 9 UDP/TLS/RTP/SAVPF 120 124 126",
        "a=sendrecv",
        "a=rtpmap:120 VP8/90000",
        "a=rtpmap:124 rtx/90000",
        "a=rtpmap:126 H264/90000",
    ].join("\r\n") };
    connection.remoteDescription = { sdp: [
        "v=0",
        "m=video 9 UDP/TLS/RTP/SAVPF 126",
        "a=recvonly",
        "a=rtpmap:126 H264/90000",
        "m=video 0 UDP/TLS/RTP/SAVPF 120",
        "a=rtpmap:120 VP8/90000",
    ].join("\r\n") };
    connection.signalingState = "stable";
    connection.fire("signalingstatechange");
    assert.ok(posts(world, "/log").includes(
        "video, ours: sendrecv VP8,H264 | theirs: recvonly H264; rejected VP8"),
        "the negotiated codecs are not logged: " + posts(world, "/log"));

    const track = connection.transceivers[1].receiver.track;
    track.muted = true;
    connection.fire("track", { track, streams: [] });
    track.muted = false;
    track.fire("unmute");
    const lines = posts(world, "/log");
    assert.ok(lines.includes("their picture: track muted, in no stream"), String(lines));
    assert.ok(lines.includes("their picture: frames"), String(lines));

    // An engine with no stats to give: connected, and nothing logged of them.
    await connect(world, connection, "connected");
    assert.equal(statLines(world).length, 0);
});

/* What Gecko's getStats gives for a call: RTP stats each way, and the
 * codecs they name. */
function callStats(extra) {
    return new Map([
        ["c1", { id: "c1", type: "codec", mimeType: "video/VP8" }],
        ["c2", { id: "c2", type: "codec", mimeType: "video/H264" }],
        ["o1", {
            type: "outbound-rtp", kind: "video", codecId: "c1", frameWidth: 480, frameHeight: 640,
            framesEncoded: 100, bytesSent: 300000, pliCount: 2, firCount: 0, nackCount: 1
        }],
        ["o2", { type: "outbound-rtp", kind: "audio", bytesSent: 5 }],
        ["i1", {
            type: "inbound-rtp", mediaType: "video", codecId: "c2",
            framesDecoded: 50, bytesReceived: 100000, packetsLost: 3
        }],
        ["r1", { type: "inbound-rtp", kind: "video", isRemote: true, framesDecoded: 9 }],
    ].concat(extra || []));
}

function statLines(world) {
    return posts(world, "/log").filter((line) => line.startsWith("video sent:"));
}

async function connect(world, connection, state) {
    connection.iceConnectionState = state;
    connection.fire("iceconnectionstatechange");
    runTimers(world);
    await settleAll();
}

test("once the call connects, what goes each way is logged, and again when the camera turns", async () => {
    const world = makeWorld(VIDEO);
    const { connection, picture } = await page(world);
    picture.paused = false;
    picture.videoWidth = 480;
    picture.videoHeight = 640;
    picture.mozPaintedFrames = 240;
    // Theirs, which the page holds hidden, from an engine that does not
    // count what it paints.
    const holder = new Element("div");
    const theirs = holder.appendChild(new Element("video"));
    theirs.srcObject = new Stream([new Track("video", "theirs")]);
    Object.assign(theirs, { paused: false, videoWidth: 352, videoHeight: 640 });
    holder.style.display = "none";
    world.videos.push(theirs);
    connection.getStats = () => Promise.resolve(callStats());
    const line = "video sent: VP8 480x640 100 frames 3000 B/frame PLI 2 FIR 0 NACK 1"
        + " (camera on live; ours playing 480x640 painted 240; theirs hidden playing 352x640 painted ?)"
        + " | got: H264 50 frames 2000 B/frame PLI 0 FIR 0 NACK 0 lost 3";

    // Turned while the call is still connecting: nothing goes yet.
    await command(world, "facing-environment");
    runTimers(world);
    await settleAll();
    await connect(world, connection, "checking");
    assert.equal(statLines(world).length, 0, "logged before the call connected");

    await connect(world, connection, "connected");
    assert.deepEqual(statLines(world), [line, line, line]);

    // Connected again is not a new call: nothing more.
    await connect(world, connection, "completed");
    assert.equal(statLines(world).length, 3, "logged again for the same connection");

    // Turned twice before anything was logged: one round, the last.
    await command(world, "facing-user");
    await command(world, "facing-environment");
    runTimers(world);
    await settleAll();
    assert.equal(statLines(world).length, 6, String(statLines(world)));
});

test("what a sender sends is told apart, and stats that say little say what they can", async () => {
    const world = makeWorld(VOICE);
    const { connection, picture } = await page(world);
    picture.paused = true;
    connection.getStats = () => Promise.resolve(new Map([
        ["o1", { type: "outbound-rtp", kind: "video", framesEncoded: 0, keyFramesEncoded: 0 }],
    ]));
    connection.iceConnectionState = "completed";
    connection.fire("iceconnectionstatechange");
    // The first sample with the blank track on the sender, the others
    // with none.
    const due = world.timers.splice(0);
    due.slice(0, -2).forEach((fn) => fn());
    videoSender(connection).track = null;
    due.slice(-2).forEach((fn) => fn());
    await settleAll();
    assert.deepEqual(statLines(world).map((line) => line.replace(/.* \(|;.*/g, "")),
        ["blank off live", "nothing", "nothing"]);
    assert.equal(statLines(world)[0], "video sent: 0 frames 0 key PLI 0 FIR 0 NACK 0"
        + " (blank off live; ours hidden paused 0x0 painted ?; theirs not drawn) | got: none");

    connection.getStats = () => Promise.reject(new Error("closed"));
    await command(world, "camera-on");
    runTimers(world);
    await settleAll();
    assert.ok(posts(world, "/log").includes("video stats: closed"), String(posts(world, "/log")));
});

test("this end's picture, left paused, is asked to play until it does", async () => {
    const world = makeWorld(VIDEO);
    const { connection, picture } = await page(world);
    let plays = 0;
    picture.paused = true;
    // Refused, as an engine refuses a play it will not allow yet.
    picture.play = () => {
        plays += 1;
        return Promise.reject(new Error("NotAllowedError"));
    };
    connection.channels.mutedState.say(JSON.stringify({ audioEnabled: true, videoEnabled: true }));
    await settleAll();
    assert.equal(plays, 1, "a paused picture was not asked to play");
    runTimers(world);
    await settleAll();
    assert.equal(plays, 2, "a picture still paused was not asked again");

    picture.paused = false;
    runTimers(world);
    runTimers(world);
    assert.equal(plays, 2, "a playing picture was asked to play");
});

test("a picture that never plays is asked a bounded number of times, and logged", async () => {
    const world = makeWorld(VIDEO);
    const { connection, picture } = await page(world);
    let plays = 0;
    picture.paused = true;
    picture.play = () => {
        plays += 1;
    };
    connection.channels.mutedState.say(JSON.stringify({ audioEnabled: true, videoEnabled: false }));
    for (let round = 0; round < 60; round++) {
        runTimers(world);
    }
    // Laid out again afterwards: the looks stay spent, and said once.
    connection.channels.mutedState.say(JSON.stringify({ audioEnabled: true, videoEnabled: true }));
    runTimers(world);
    assert.ok(plays > 10 && plays <= 42, "asked to play " + plays + " times");
    assert.deepEqual(posts(world, "/log").filter((line) => line.startsWith("our picture:")),
        ["our picture: it never played"]);
});

test("what goes to the host goes one request at a time, in the order it was said", async () => {
    const world = makeWorld(VIDEO);
    world.holdPosts = true;
    await page(world);
    const sent = () => world.requests.filter((r) => r.method === "POST");
    assert.equal(sent().length, 1, "a request went before the one ahead of it was answered");
    assert.ok(sent()[0].timeout > 0, "a request never answered would hold the rest for good");

    // Answered, or failed, or given up on: each lets the next go.
    while (sent().some((r) => r.status === undefined)) {
        const waiting = sent().filter((r) => r.status === undefined);
        assert.equal(waiting.length, 1, "two requests in flight at once");
        waiting[0].done();
    }
    const said = sent().map((r) => r.url.replace(/.*\//, "/") + " " + r.body);
    assert.ok(/^\/log camera open: /.test(said[0]), String(said));
    assert.equal(said[said.length - 2], '/log reported: {"local":true,"remote":false,"localPainted":false,"remotePainted":false,"front":true,"failed":false}');
    assert.equal(said[said.length - 1], '/video {"local":true,"remote":false,"localPainted":false,"remotePainted":false,"front":true,"failed":false}');

    // Nothing waiting: the next is sent at once.
    world.holdPosts = false;
    world.window.console.warn("later");
    assert.equal(sent().at(-1).body, "page warn: later");
});

/* A picture of the other end, shown by the page: in a box of its own,
 * playing, with a size, and as many frames painted as `painted` says --
 * none said where it is undefined. Each time its stream is given to it
 * is counted. */
function theirPicture(world, connection, painted) {
    const holder = new Element("div");
    const picture = holder.appendChild(new Element("video"));
    const track = new Track("video", "theirs");
    const stream = new Stream([track]);
    const seen = { given: 0, moves: [], onGiven: () => {} };
    let source = stream;
    Object.defineProperty(picture, "srcObject", {
        get: () => source,
        set(value) {
            source = value;
            seen.given += 1;
            seen.onGiven();
        }
    });
    stream.removeTrack = (one) => {
        seen.moves.push("off:" + one.label);
        Stream.prototype.removeTrack.call(stream, one);
    };
    stream.addTrack = (one) => {
        seen.moves.push("on:" + one.label);
        Stream.prototype.addTrack.call(stream, one);
    };
    Object.assign(picture, { paused: false, videoWidth: 352, videoHeight: 640 });
    if (painted !== undefined) {
        picture.mozPaintedFrames = painted;
    }
    world.videos.push(picture);
    connection.fire("track", { track, streams: [stream] });
    return { picture, holder, seen };
}

function paintLines(world) {
    return posts(world, "/log").filter((line) => / picture( put back|: still)/.test(line));
}

test("a picture on the screen that paints nothing is put back, and only so often", async () => {
    const world = makeWorld(VIDEO);
    const { connection, picture: ours, stream } = await page(world);
    Object.assign(ours, { paused: false, videoWidth: 480, videoHeight: 640, mozPaintedFrames: 0 });
    const moves = [];
    const removeTrack = stream.removeTrack.bind(stream);
    stream.removeTrack = (one) => {
        moves.push("off:" + one.label);
        removeTrack(one);
    };
    // Theirs paints all along.
    const { picture, seen } = theirPicture(world, connection, 10);
    // Said of again and again before the look: still one look a round.
    for (let i = 0; i < 4; i++) {
        connection.channels.mutedState.say(JSON.stringify({ audioEnabled: true, videoEnabled: i % 2 === 0 }));
    }
    connection.channels.mutedState.say(JSON.stringify({ audioEnabled: true, videoEnabled: true }));

    // A look to see where each stands, and the next finds ours unpainted.
    runTimers(world);
    picture.mozPaintedFrames += 24;
    assert.equal(paintLines(world).length, 0, "put back before it had a look's time to paint");
    runTimers(world);
    // Looked at soon while it has yet to show.
    assert.deepEqual(paintLines(world), ["our picture put back: nothing painted in 0.5 s"]);
    assert.deepEqual(moves, ["off:camera-user"], "our picture's frames were not taken off and put on");
    assert.deepEqual(stream.getVideoTracks().map((t) => t.label), ["camera-user"]);
    assert.equal(ours.srcObject, stream);
    // Put back, it has a look's time of its own to paint.
    picture.mozPaintedFrames += 24;
    runTimers(world);
    assert.equal(paintLines(world).length, 1, "put back again without a look's time to paint");

    // Painting again after that: left as it is.
    for (let round = 0; round < 4; round++) {
        ours.mozPaintedFrames += 24;
        picture.mozPaintedFrames += 24;
        runTimers(world);
    }
    assert.equal(paintLines(world).length, 1, String(paintLines(world)));

    // Never painting: three times in all, then said once, then left.
    for (let round = 0; round < 20; round++) {
        picture.mozPaintedFrames += 24;
        runTimers(world);
    }
    // Once it has shown, it is looked at less often.
    assert.deepEqual(paintLines(world), [
        "our picture put back: nothing painted in 0.5 s",
        "our picture put back: nothing painted in 2 s",
        "our picture put back: nothing painted in 2 s",
        "our picture: still not painted"
    ]);
    assert.equal(seen.given, 0, "their picture, painting, was put back");

    // Theirs stops painting: counted on its own.
    for (let round = 0; round < 20; round++) {
        runTimers(world);
    }
    assert.deepEqual(paintLines(world).slice(4), [
        "their picture put back: nothing painted in 2 s",
        "their picture put back: nothing painted in 2 s",
        "their picture put back: nothing painted in 2 s",
        "their picture: still not painted"
    ]);
    assert.equal(seen.given, 3);
});

test("a picture hidden, paused, or with no frame yet is not put back", async () => {
    const world = makeWorld(VIDEO);
    const { connection, box, picture: ours } = await page(world);
    Object.assign(ours, { paused: true, videoWidth: 480, videoHeight: 640, mozPaintedFrames: 0 });
    const { picture, holder, seen } = theirPicture(world, connection, 0);
    // Seen once with a frame, then none for a while: the look's time
    // starts again once it has one.
    runTimers(world);
    picture.videoWidth = 0;
    for (let round = 0; round < 6; round++) {
        runTimers(world);
    }
    // The page draws their picture anew: the new one has a look's time
    // of its own.
    picture.videoWidth = 352;
    runTimers(world);
    const again = theirPicture(world, connection, 0);
    world.videos.splice(world.videos.indexOf(picture), 1);
    runTimers(world);
    assert.deepEqual(paintLines(world), [], "a picture drawn anew was put back at once");
    again.holder.style.display = "none";
    box.style.display = "none";
    ours.paused = false;
    for (let round = 0; round < 6; round++) {
        runTimers(world);
    }
    assert.deepEqual(paintLines(world), []);
    assert.equal(seen.given + again.seen.given, 0);
    assert.equal(world.timers.length, 0, "nothing on the screen, and still looked at");
});

test("where the engine does not count what it paints, each picture is put back once", async () => {
    const world = makeWorld(VIDEO);
    const { connection, picture: ours } = await page(world);
    Object.assign(ours, { paused: false, videoWidth: 480, videoHeight: 640 });
    const { seen } = theirPicture(world, connection);
    for (let round = 0; round < 12; round++) {
        runTimers(world);
    }
    assert.deepEqual(paintLines(world).sort(), [
        "our picture put back: the engine does not say what it painted",
        "their picture put back: the engine does not say what it painted"
    ]);
    assert.equal(seen.given, 1);
    assert.deepEqual(seen.moves, ["off:theirs", "on:theirs"]);
});

test("a picture put back is asked to play until it does: it carries their voice", async () => {
    const world = makeWorld(VOICE);
    const { connection } = await page(world);
    const { picture, seen } = theirPicture(world, connection);
    let plays = 0;
    picture.play = () => {
        plays += 1;
        return Promise.reject(new Error("NotAllowedError"));
    };
    // Given its stream again, a picture starts paused.
    seen.onGiven = () => {
        picture.paused = true;
    };
    runTimers(world);
    runTimers(world);
    assert.equal(seen.given, 1);
    assert.equal(plays, 1, "a picture put back paused was not asked to play");
    runTimers(world);
    runTimers(world);
    await settleAll();
    assert.equal(plays, 3, "a picture still paused was not asked again");
    picture.paused = false;
    runTimers(world);
    runTimers(world);
    assert.equal(plays, 3, "a playing picture was asked to play");
});

test("in the background, or with the page held hidden, the pictures are not looked at", async () => {
    const world = makeWorld(VIDEO, { visibilityState: "visible" });
    const { connection, picture: ours } = await page(world);
    assert.equal(posts(world, "/log")[0], "visibility: visible");
    Object.assign(ours, { paused: false, videoWidth: 480, videoHeight: 640, mozPaintedFrames: 0 });
    theirPicture(world, connection, 0);
    await command(world, "hidden");
    for (let round = 0; round < 6; round++) {
        runTimers(world);
    }
    assert.deepEqual(paintLines(world), []);
    assert.equal(world.timers.length, 0, "looked at in the background");

    // In front again: looked at again.
    await command(world, "shown");
    runTimers(world);
    runTimers(world);
    assert.equal(paintLines(world).length, 2, String(paintLines(world)));

    // The engine holds the page hidden: said, and not looked at.
    world.window.document.visibilityState = "hidden";
    world.documentListeners.visibilitychange.forEach((one) => one.fn());
    assert.equal(posts(world, "/log").at(-1), "visibility: hidden");
    for (let round = 0; round < 6; round++) {
        runTimers(world);
    }
    assert.equal(paintLines(world).length, 2, String(paintLines(world)));
    assert.equal(world.timers.length, 0);
    world.window.document.visibilityState = "visible";
    world.documentListeners.visibilitychange.forEach((one) => one.fn());
    assert.ok(world.timers.length > 0, "shown again, and nothing looks");
});

test("every press of one of the page's buttons is logged, and by whom", async () => {
    const world = makeWorld(VOICE);
    await page(world);
    const [press] = world.documentListeners.click;
    assert.equal(press.capture, true, "a press the page stops would go unlogged");
    const button = new Element("button");
    button.getAttribute = (name) => (name === "aria-label" ? "Mute microphone" : null);
    const icon = button.appendChild(new Element("svg"));
    icon.getAttribute = () => null;
    press.fn({ target: icon, isTrusted: true });
    press.fn({ target: button, isTrusted: false });
    press.fn({ target: new Element("div"), isTrusted: true });
    assert.deepEqual(posts(world, "/log").filter((line) => line.startsWith("pressed: ")), [
        "pressed: Mute microphone, by a touch",
        "pressed: Mute microphone, by the app"
    ]);
});

test("a picture is said to show once it paints a frame, and black until then", async () => {
    const world = makeWorld(VIDEO);
    const { connection, picture: ours } = await page(world);
    Object.assign(ours, { paused: false, videoWidth: 480, videoHeight: 640, mozPaintedFrames: 7 });
    // Turned: the picture is given the other camera, which it has yet to
    // show, whatever it painted of the one before.
    await command(world, "facing-environment");
    assert.equal(lastVideo(world).localPainted, false);
    runTimers(world);
    assert.equal(lastVideo(world).localPainted, false, "said to show what it has not painted");
    ours.mozPaintedFrames += 1;
    runTimers(world);
    assert.equal(lastVideo(world).localPainted, true, "a picture that painted is still said to be black");
    // Turned back: black again until it paints.
    await command(world, "facing-user");
    assert.equal(lastVideo(world).localPainted, false, "turned, the picture is still said to show");
    ours.mozPaintedFrames += 1;
    runTimers(world);
    assert.equal(lastVideo(world).localPainted, true);

    // Theirs comes, black at first.
    const { picture } = theirPicture(world, connection, 3);
    assert.equal(lastVideo(world).remote, true);
    assert.equal(lastVideo(world).remotePainted, false);
    runTimers(world);
    assert.equal(lastVideo(world).remotePainted, false);
    picture.mozPaintedFrames += 1;
    runTimers(world);
    assert.equal(lastVideo(world).remotePainted, true);
    assert.equal(lastVideo(world).localPainted, true, "theirs coming made ours black");

    // Hidden, it shows nothing; back, it has a frame to paint again.
    picture.parentElement.style.display = "none";
    runTimers(world);
    assert.equal(lastVideo(world).remotePainted, false, "a hidden picture is said to show");
    picture.parentElement.style.display = "block";
    runTimers(world);
    assert.equal(lastVideo(world).remotePainted, false, "shown again, said to show before it painted");
    assert.deepEqual(paintLines(world).filter((line) => line.startsWith("their")), [],
        "shown again, put back without a look's time to paint");
    picture.mozPaintedFrames += 1;
    runTimers(world);
    assert.equal(lastVideo(world).remotePainted, true);

    // Drawn anew, on another picture: black until that one paints.
    world.videos.splice(world.videos.indexOf(picture), 1);
    const again = theirPicture(world, connection, 9).picture;
    assert.equal(lastVideo(world).remotePainted, false, "a picture drawn anew is said to show");
    again.mozPaintedFrames += 1;
    runTimers(world);
    assert.equal(lastVideo(world).remotePainted, true);
});

test("a picture yet to show is looked at sooner than one that shows", async () => {
    const world = makeWorld(VIDEO);
    const { connection, picture: ours } = await page(world);
    Object.assign(ours, { paused: false, videoWidth: 480, videoHeight: 640, mozPaintedFrames: 0 });
    await command(world, "shown");
    ours.mozPaintedFrames += 1;
    runTimers(world);
    assert.equal(lastVideo(world).localPainted, true);
    // Said of again and again: still the one look to come.
    const timers = world.timers.length;
    for (let i = 0; i < 4; i++) {
        connection.channels.mutedState.say(JSON.stringify({ audioEnabled: true, videoEnabled: true }));
    }
    assert.equal(world.timers.length, timers, "each report started a look of its own");

    // Theirs comes, and never paints: looked at sooner, in place of the
    // look that was due, and put back a look's time after it came.
    const { picture } = theirPicture(world, connection, 0);
    assert.equal(world.timers.at(-1).delay, 500, "the look due was left to come in its own time");
    ours.mozPaintedFrames += 1;
    runTimers(world);
    assert.deepEqual(paintLines(world), [], "put back without a look's time to paint");
    ours.mozPaintedFrames += 1;
    runTimers(world);
    assert.deepEqual(paintLines(world), ["their picture put back: nothing painted in 0.5 s"]);

    // Theirs gone, and ours, which shows, stops painting: looked at as
    // often as a picture that shows.
    world.videos.splice(world.videos.indexOf(picture), 1);
    ours.mozPaintedFrames += 1;
    runTimers(world);
    runTimers(world);
    assert.deepEqual(paintLines(world).slice(1), ["our picture put back: nothing painted in 2 s"]);
});

test("turned, this end's picture is black from the turn until the other camera paints, however slow", async () => {
    const world = makeWorld(VIDEO);
    const { connection, picture: ours } = await page(world);
    Object.assign(ours, { paused: false, videoWidth: 480, videoHeight: 640, mozPaintedFrames: 7 });
    await command(world, "facing-environment");
    runTimers(world);
    ours.mozPaintedFrames += 1;
    runTimers(world);
    assert.equal(lastVideo(world).localPainted, true);
    // Theirs, which paints nothing while this camera turns.
    theirPicture(world, connection, 3);

    world.cameraDelay = true;
    await command(world, "facing-user");
    assert.equal(lastVideo(world).localPainted, false,
        "said to show from the turn until the other camera is given");
    // Seconds of looks, the camera still on its way, the last frame held.
    for (let i = 0; i < 10; i++) {
        runTimers(world);
    }
    assert.equal(lastVideo(world).localPainted, false, "a camera slow to turn was given up on");
    assert.deepEqual(paintLines(world).filter((line) => line.startsWith("our")), [],
        "a camera on its way was put back");
    assert.ok(paintLines(world).some((line) => line.startsWith("their picture put back")),
        "their picture was not looked after while this camera turned");

    const finish = world.cameraDelay;
    world.cameraDelay = null;
    finish();
    await settleAll();
    runTimers(world);
    assert.equal(lastVideo(world).localPainted, false, "said to show before the other camera painted");
    ours.mozPaintedFrames += 1;
    runTimers(world);
    assert.equal(lastVideo(world).localPainted, true);
    assert.equal(lastVideo(world).front, true);
});

test("a picture that never shows is given up on, rather than waited for", async () => {
    const world = makeWorld(VIDEO);
    const { picture: ours } = await page(world);
    Object.assign(ours, { paused: false, videoWidth: 480, videoHeight: 640, mozPaintedFrames: 0 });
    await command(world, "shown");
    // Put back three times, a look apart; looked at, and said, once more.
    for (let round = 0; round < 7; round++) {
        runTimers(world);
    }
    assert.equal(paintLines(world).length, 3, String(paintLines(world)));
    assert.equal(lastVideo(world).localPainted, false);
    runTimers(world);
    assert.equal(paintLines(world).at(-1), "our picture: still not painted");
    assert.equal(lastVideo(world).localPainted, true, "a picture given up on is still waited for");
});

test("where the engine does not count what it paints, a picture with a frame shows", async () => {
    const world = makeWorld(VIDEO);
    const { picture: ours } = await page(world);
    Object.assign(ours, { paused: true, videoWidth: 480, videoHeight: 640 });
    await command(world, "shown");
    runTimers(world);
    assert.equal(lastVideo(world).localPainted, false, "a paused picture is said to show");
    Object.assign(ours, { paused: false, videoWidth: 0 });
    runTimers(world);
    assert.equal(lastVideo(world).localPainted, false, "a picture with no frame is said to show");
    ours.videoWidth = 480;
    runTimers(world);
    assert.equal(lastVideo(world).localPainted, true);
});

test("their picture given a new stream has yet to show it, and given the same one again still shows", async () => {
    const world = makeWorld(VIDEO, { mediaElement: true });
    const { connection, stream } = await page(world);
    const holder = new Element("div");
    const picture = holder.appendChild(world.newVideo());
    Object.assign(picture, { paused: false, videoWidth: 352, videoHeight: 640, mozPaintedFrames: 50 });
    world.videos.push(picture);
    const track = new Track("video", "theirs");
    const first = new Stream([track]);
    connection.fire("track", { track, streams: [first] });
    picture.srcObject = first;
    runTimers(world);
    picture.mozPaintedFrames += 1;
    runTimers(world);
    assert.equal(lastVideo(world).remotePainted, true);

    // Given it again, as a picture put back is: still showing.
    const reports = posts(world, "/video").length;
    picture.srcObject = first;
    runTimers(world);
    assert.equal(lastVideo(world).remotePainted, true, "the same stream given again is said to be black");
    assert.equal(posts(world, "/video").length, reports);

    // This end's stream put on a picture is nothing to do with theirs.
    const mine = world.newVideo();
    mine.srcObject = stream;
    runTimers(world);
    assert.equal(lastVideo(world).remotePainted, true, "our stream given made their picture black");

    // A new stream: black until it paints, the engine counting afresh.
    picture.srcObject = new Stream([new Track("video", "theirs-again")]);
    runTimers(world);
    assert.equal(lastVideo(world).remotePainted, false, "a new stream is said to show before it painted");
    picture.mozPaintedFrames = 0;
    runTimers(world);
    picture.mozPaintedFrames += 1;
    runTimers(world);
    assert.equal(lastVideo(world).remotePainted, true, "a picture counted afresh is never said to show");
});
