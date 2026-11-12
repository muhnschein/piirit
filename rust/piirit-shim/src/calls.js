/*
 * `window.calls` for calls-webapp, served by call host (call_host.rs) as `calls.js`.
 *
 * Page does media. Core, behind loopback host, does offer, answer, end, ICE servers, peer
 * picture: one request each. Peer's answer returns as hash command.
 *
 * No secrets, nothing templated: API key is in page's address, given only to app's own
 * WebView; another page loading this script (cross-origin allowed) reaches nothing.
 *
 * XHR, not fetch, as webxdc.js: XHR predates every Gecko this runs on.
 */
(function () {
    "use strict";

    /* From page's query. */
    var KEY = (function () {
        var found = /[?&]key=([0-9a-f]+)/.exec(window.location.search);
        return found ? found[1] : "";
    }());
    /* Core path behind host. */
    var API = "/call-api/" + KEY;
    /* Retry delay, ms, after host failure. */
    var RETRY = 1000;

    /*
     * Gecko <126 lacks RTCIceCandidate.type, needed to offer on first relay candidate
     * (else waits for gathering; slow servers time out). Parsed from candidate.
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
     * VP8 first per video line. Sailfish Gecko lists hardware H264 first; peer answers in
     * offer order, and this engine draws H264 unreliably. Reorder only, sent offer only:
     * every codec stays offered. rtx stream stays beside its codec.
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
     * One request at a time: on separate connections two arrived swapped, leaving the
     * screen (last picture report wins) stale. Unanswered holds queue at most WAIT.
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
        /* Any outcome, timeout too. */
        xhr.onloadend = postNext;
        xhr.send(next.body);
    }

    /* MEASURE-301: temp. Raw mic; beep (1 kHz) onsets of far and mic sound in one audio clock. */
    var m301 = { ctx: null, keep: [] };
    function m301log(text) {
        post("/log", "M301 " + text);
    }
    function m301tap(stream, name) {
        try {
            if (!m301.ctx) {
                m301.ctx = new AudioContext();
                m301log("ctx rate=" + m301.ctx.sampleRate + " base=" + m301.ctx.baseLatency
                    + " out=" + m301.ctx.outputLatency);
            }
            var ctx = m301.ctx;
            var src = ctx.createMediaStreamSource(stream);
            var proc = ctx.createScriptProcessor(256, 1, 1);
            var quiet = ctx.createGain();
            quiet.gain.value = 0;
            var k = 2 * Math.cos(2 * Math.PI * 1000 / ctx.sampleRate);
            var floor = -1;
            var run = 0;
            var first = 0;
            var firstWall = 0;
            var firstNow = 0;
            var lastOn = -10;
            var count = 0;
            proc.onaudioprocess = function (e) {
                var d = e.inputBuffer.getChannelData(0);
                var a = 0;
                var b = 0;
                for (var i = 0; i < d.length; i++) {
                    var t = d[i] + k * a - b;
                    b = a;
                    a = t;
                }
                var p = (a * a + b * b - k * a * b) / (d.length * d.length);
                if (floor < 0) {
                    floor = p;
                }
                var limit = Math.max(floor * 20, 1e-9);
                if (p > limit) {
                    if (run === 0) {
                        first = e.playbackTime;
                        firstWall = Date.now();
                        firstNow = ctx.currentTime;
                    }
                    run += 1;
                    if (run === 3 && first - lastOn > 0.5) {
                        lastOn = first;
                        count += 1;
                        if (count <= 60) {
                            m301log(name + " audio=" + (first * 1000).toFixed(1) + " wall=" + firstWall
                                + " now=" + (firstNow * 1000).toFixed(1) + " p=" + p.toExponential(2)
                                + " floor=" + floor.toExponential(2));
                        }
                    }
                } else {
                    run = 0;
                }
                floor += 0.02 * (Math.min(p, 2 * floor + 1e-12) - floor);
            };
            src.connect(proc);
            proc.connect(quiet);
            quiet.connect(ctx.destination);
            m301.keep.push(src, proc, quiet);
            m301log(name + " tapped");
        } catch (err) {
            m301log(name + " tap failed " + err);
        }
    }
    if (navigator.mediaDevices && navigator.mediaDevices.getUserMedia) {
        var m301gum = navigator.mediaDevices.getUserMedia.bind(navigator.mediaDevices);
        navigator.mediaDevices.getUserMedia = function (wanted) {
            if (wanted && wanted.audio) {
                wanted.audio = { echoCancellation: false, noiseSuppression: false, autoGainControl: false };
            }
            return m301gum(wanted).then(function (stream) {
                if (stream.getAudioTracks().length) {
                    m301tap(new MediaStream(stream.getAudioTracks()), "mic");
                }
                return stream;
            });
        };
    }
    function m301watch(connection) {
        connection.addEventListener("track", function (ev) {
            if (ev.track.kind === "audio") {
                m301tap(new MediaStream([ev.track]), "far");
            }
        });
        var told = 0;
        window.setInterval(function () {
            connection.getStats().then(function (stats) {
                stats.forEach(function (s) {
                    if (s.type === "inbound-rtp" && s.kind === "audio" && told < 12) {
                        told += 1;
                        if (told === 1) {
                            m301log("stats keys " + Object.keys(s).join(","));
                        }
                        var picked = [];
                        for (var key in s) {
                            if (/jitter|delay|buffer|conceal|emitted|samples|packets/i.test(key)) {
                                picked.push(key + "=" + s[key]);
                            }
                        }
                        m301log("stats " + picked.join(" "));
                    }
                });
            });
        }, 5000);
    }

    /* Video (call_video.js, served ahead): hears every peer connection, takes camera commands. */
    piiritVideo.attach(post);

    /*
     * Page never reports connection state: watch each peer connection for app's
     * "Connected" and end-on-failure. Constructor returns the native object to `new`;
     * shared prototype keeps `instanceof`.
     */
    var Native = window.RTCPeerConnection;

    /*
     * Gecko <115 (ESR 91, Sailfish OS 5.0) lacks setConfiguration, which page calls after
     * getIceServers: throws, no call starts. There servers go in at construction (sync
     * fetch, local host); page's later call is a no-op.
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
            m301watch(connection);
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
     * Mic switched from app screen over unseen page. Page owns mic state and tells peer via
     * its switch (labelled with press's effect): press it, never kill track behind page.
     * Not drawn yet: retry.
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
     * Host's ordered queue: hash commands plus mic commands. Long poll, so answers arrive
     * promptly.
     */
    var waiting = [];
    function apply() {
        if (waiting.length === 0) {
            return;
        }
        window.location.hash = waiting.shift();
        /* One per turn: two in one turn read as last. */
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
        /* Gathered offer; core places call. */
        startCall: function (offer) {
            post("/start", preferVp8(offer));
        },
        /* Page's answer to call it was opened on. */
        acceptCall: function (answer) {
            post("/accept", answer);
        },
        /* Red button, any state; app removes page. */
        endCall: function () {
            post("/end", "");
        },
        /* JSON string; fetched so TURN credentials stay behind key. */
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
        /* Over page's placeholder, shown if host has none. */
        getAvatar: function () {
            return API + "/avatar";
        }
    };

    listen();
}());
