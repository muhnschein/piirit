/*
 * `window.calls` for calls-webapp, served by call host (call_host.rs) as `calls.js`.
 *
 * Page does call (camera, mic, peer connection, codecs). Asks host for five things: send
 * offer, send answer, end call, ICE servers, other end's picture. All core's job; core is
 * behind loopback host, so each is a request. Other end's answer comes back as hash
 * command, set here once host has it.
 *
 * No secrets, nothing templated. Host API key is in page's own address, which app gives
 * its own WebView only: page elsewhere loading this script (cross-origin allowed) finds
 * no key in its address, reaches nothing.
 *
 * XHR, not fetch, as webxdc.js: Gecko has both, XHR predates every release this runs on.
 */
(function () {
    "use strict";

    /* Key from page's query. */
    var KEY = (function () {
        var found = /[?&]key=([0-9a-f]+)/.exec(window.location.search);
        return found ? found[1] : "";
    }());
    /* Core path behind host. */
    var API = "/call-api/" + KEY;
    /* Retry delay, ms, after host failure. */
    var RETRY = 1000;

    /*
     * Gecko <126 lacks RTCIceCandidate.type. Needed for offer on first relay candidate;
     * missing, offer waits for gathering (slow servers time out). Parsed from candidate.
     */
    if (window.RTCIceCandidate
            && !("type" in window.RTCIceCandidate.prototype)) {
        Object.defineProperty(window.RTCIceCandidate.prototype, "type", {
            configurable: true,
            get: function () {
                var found = / typ ([a-z]+)/.exec(this.candidate || "");
                return found ? found[1] : null;
            }
        });
    }

    /*
     * VP8 first per video line. Sailfish Gecko enables hardware H264 for WebRTC and lists it
     * first; other end answers in offer order, so calls placed here get H264, which this
     * engine draws unreliably (answered calls get VP8 from other end's offer). Order only,
     * only in what other end is sent: every codec stays offered, so any answer is one this
     * end has. Codec's retransmission (rtx) stream stays next to it.
     */
    function preferVp8(sdp) {
        var lines = sdp.split("\r\n");
        var vp8 = {};
        var rtxOf = {};
        lines.forEach(function (line) {
            var map = /^a=rtpmap:(\d+) VP8\/90000/i.exec(line);
            if (map) {
                vp8[map[1]] = true;
            }
            var rtx = /^a=fmtp:(\d+) apt=(\d+)$/.exec(line);
            if (rtx) {
                rtxOf[rtx[1]] = rtx[2];
            }
        });
        return lines.map(function (line) {
            var media = /^(m=video \d+ [A-Z/]+) (.+)$/.exec(line);
            if (!media) {
                return line;
            }
            var types = media[2].split(" ");
            var first = types.filter(function (type) {
                return vp8[type] || vp8[rtxOf[type]];
            });
            var rest = types.filter(function (type) {
                return first.indexOf(type) < 0;
            });
            return media[1] + " " + first.concat(rest).join(" ");
        }).join("\r\n");
    }

    /*
     * One request at a time, in order. Each goes on own connection; two in flight seen
     * arriving swapped, leaving call screen (goes by last picture report) with older one.
     * Unanswered request holds queue at most WAIT.
     */
    var WAIT = 10000;
    var outbox = [];
    var posting = false;
    function post(path, body) {
        outbox.push({ path: path, body: body });
        if (!posting) {
            postNext();
        }
    }
    function postNext() {
        var next = outbox.shift();
        posting = Boolean(next);
        if (!next) {
            return;
        }
        var xhr = new XMLHttpRequest();
        xhr.open("POST", API + next.path, true);
        xhr.setRequestHeader("Content-Type", "text/plain; charset=utf-8");
        xhr.timeout = WAIT;
        /* Any outcome, incl. timeout. */
        xhr.onloadend = postNext;
        xhr.send(next.body);
    }

    /* Video (call_video.js, served ahead): hears every peer connection, takes camera commands. */
    piiritVideo.attach(post);

    /*
     * Page never reports connection state; peer connections do. Each watched,
     * state passed on: app's "Connected" and end-on-failure use it.
     *
     * Built as page does, with `new`: constructor returning object hands it to `new`;
     * shared prototype keeps `instanceof` true.
     */
    var Native = window.RTCPeerConnection;

    /*
     * Gecko <115 (ESR 91, Sailfish OS 5.0) lacks setConfiguration. Page makes connection
     * with no servers, sets them after getIceServers; that throws, place and answer both
     * wait on it, so no call starts. There servers go in at construction (sync fetch, host
     * on same machine); page's later setConfiguration does nothing.
     */
    var fixedServers = Native && !Native.prototype.setConfiguration;
    function iceServersNow() {
        try {
            var xhr = new XMLHttpRequest();
            xhr.open("GET", API + "/ice", false);
            xhr.send();
            return xhr.status === 200 ? JSON.parse(xhr.responseText) : [];
        } catch (err) {
            return [];
        }
    }

    if (Native) {
        var Watched = function (configuration) {
            var settings = configuration;
            if (fixedServers) {
                settings = {};
                for (var key in configuration || {}) {
                    settings[key] = configuration[key];
                }
                settings.iceServers = iceServersNow();
            }
            var connection = new Native(settings);
            if (fixedServers) {
                connection.setConfiguration = function () {};
            }
            piiritVideo.watch(connection);
            connection.addEventListener("iceconnectionstatechange", function () {
                post("/state", connection.iceConnectionState);
            });
            return connection;
        };
        Watched.prototype = Native.prototype;
        if (Native.generateCertificate) {
            Watched.generateCertificate = Native.generateCertificate;
        }
        window.RTCPeerConnection = Watched;
    }

    /*
     * Mic switched from app's own screen; page runs unseen under it. Page keeps mic state
     * and tells other end from its own switch (labelled with what a press would do): press
     * that switch, never turn track off behind page. Switch not drawn yet: retry until it is.
     */
    var MUTE_LABEL = "Mute microphone";
    var UNMUTE_LABEL = "Unmute microphone";
    var wantMuted = null;
    var lookingAgain = false;
    function labelled(label) {
        return document.querySelector("[aria-label=\"" + label + "\"]");
    }
    function applyMute() {
        if (wantMuted === null) {
            return;
        }
        var press = labelled(wantMuted ? MUTE_LABEL : UNMUTE_LABEL);
        if (press) {
            press.click();
            return;
        }
        if (labelled(wantMuted ? UNMUTE_LABEL : MUTE_LABEL) || lookingAgain) {
            return;
        }
        lookingAgain = true;
        window.setTimeout(function () {
            lookingAgain = false;
            applyMute();
        }, 250);
    }

    /*
     * Host queue for page, in order: hash commands, plus two mic commands above. Long poll:
     * host holds request until it has something, so other end's answer arrives promptly.
     */
    var waiting = [];
    function apply() {
        if (waiting.length === 0) {
            return;
        }
        window.location.hash = waiting.shift();
        /* One per turn: page reads hash when change reaches it; two in one turn read as last. */
        window.setTimeout(apply, 0);
    }
    function listen() {
        var xhr = new XMLHttpRequest();
        xhr.open("GET", API + "/commands", true);
        xhr.onload = function () {
            if (xhr.status === 200) {
                var commands = [];
                try {
                    commands = JSON.parse(xhr.responseText) || [];
                } catch (err) {
                    commands = [];
                }
                var switched = false;
                for (var i = 0; i < commands.length; i++) {
                    if (commands[i] === "mute" || commands[i] === "unmute") {
                        wantMuted = commands[i] === "mute";
                        switched = true;
                    } else if (!piiritVideo.take(commands[i])) {
                        waiting.push(commands[i]);
                    }
                }
                apply();
                /* Last word only: presses would undo each other. */
                if (switched) {
                    applyMute();
                }
                listen();
            } else if (xhr.status !== 404) {
                window.setTimeout(listen, RETRY);
            }
            /* 404: call over, nothing more. */
        };
        xhr.onerror = function () {
            window.setTimeout(listen, RETRY);
        };
        xhr.send();
    }

    window.calls = {
        /* Page's offer, gathered. Core places call. */
        startCall: function (offer) {
            post("/start", preferVp8(offer));
        },
        /* Page's answer to call it was opened on. */
        acceptCall: function (answer) {
            post("/accept", answer);
        },
        /* Red button, any state. App removes page. */
        endCall: function () {
            post("/end", "");
        },
        /* Core's servers, JSON string. Fetched, not baked in: keeps TURN credentials behind key. */
        getIceServers: function () {
            return new Promise(function (resolve) {
                var xhr = new XMLHttpRequest();
                xhr.open("GET", API + "/ice", true);
                xhr.onload = function () {
                    resolve(xhr.status === 200 ? xhr.responseText : "[]");
                };
                xhr.onerror = function () {
                    resolve("[]");
                };
                xhr.send();
            });
        },
        /* Drawn over page's placeholder, which still shows if host has no picture. */
        getAvatar: function () {
            return API + "/avatar";
        }
    };

    listen();
}());
