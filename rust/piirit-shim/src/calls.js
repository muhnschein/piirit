/*
 * `window.calls` for calls-webapp, served by the call host
 * (call_host.rs) as the page's own `calls.js`.
 *
 * The page does the call -- the camera and the microphone, the peer
 * connection, the codecs -- and asks its host for five things: to send
 * its offer, to send its answer, to end the call, the ICE servers, and a
 * picture of the other end. Each of those is the core's to do, and the
 * core is on the other side of the loopback host, so each is a request
 * to it. What comes back the other way -- the other end's answer -- the
 * page takes as a hash command, which this sets once the host has it.
 *
 * Nothing in here is a secret, and nothing is filled in. The key to the
 * host's API is in the page's own address, which the app hands its own
 * WebView and nothing else: a page elsewhere that loads this script into
 * itself -- a browser allows that across origins -- finds no key in its
 * own address, and can reach nothing with it.
 *
 * XMLHttpRequest rather than fetch, as in webxdc.js: Gecko has both, and
 * this one is older than every release this could run on.
 */
(function () {
    "use strict";

    /* The key, from the page's own query. */
    var KEY = (function () {
        var found = /[?&]key=([0-9a-f]+)/.exec(window.location.search);
        return found ? found[1] : "";
    }());
    /* Where the core is, behind the host. */
    var API = "/call-api/" + KEY;
    /* Milliseconds to wait before asking again when the host failed. */
    var RETRY = 1000;

    /*
     * Gecko before 126 has no RTCIceCandidate.type. The page reads it to
     * send its offer as soon as it has a candidate that goes through a
     * relay; without it that never matches, and the offer waits for
     * gathering to finish -- every server tried, the slow ones to their
     * timeouts -- while the other end is not yet ringing. The type is in
     * the candidate line itself, so it is read from there.
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

    function post(path, body) {
        var xhr = new XMLHttpRequest();
        xhr.open("POST", API + path, true);
        xhr.setRequestHeader("Content-Type", "text/plain; charset=utf-8");
        xhr.send(body);
    }

    /*
     * The page says nothing about how the connection is doing -- nothing
     * when it connects, nothing when it fails. The peer connection does,
     * so every one the page makes is watched, and what it says is passed
     * on. That is what the app's own screen says "Connected" from, and
     * what ends a call whose connection has gone for good.
     *
     * Made the way the page makes one, `new` and all: a constructor that
     * returns an object gives that object to `new`, and sharing the
     * prototype keeps `instanceof` true.
     */
    var Native = window.RTCPeerConnection;

    /*
     * Gecko before 115 -- ESR 91, which Sailfish OS 5.0 ships -- has no
     * setConfiguration. The page makes its connection with no servers and
     * sets them once getIceServers answers; without the method that
     * throws, and both placing and answering wait on it, so no call would
     * ever start. There, the servers go in when the connection is made --
     * asked for there and then, the host being on the same machine -- and
     * the page's later setConfiguration does nothing, the connection
     * already having what it would have set.
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
     * What the host has for the page: hash commands, in order. A long
     * poll -- the host holds the request until it has something -- so
     * the other end's answer arrives as it arrives.
     */
    var waiting = [];
    function apply() {
        if (waiting.length === 0) {
            return;
        }
        window.location.hash = waiting.shift();
        /* One per turn: the page reads the hash when the change reaches
         * it, and two set in one turn would both be read as the last. */
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
                for (var i = 0; i < commands.length; i++) {
                    waiting.push(commands[i]);
                }
                apply();
                listen();
            } else if (xhr.status !== 404) {
                window.setTimeout(listen, RETRY);
            }
            /* A 404 is the call being over: nothing more will come. */
        };
        xhr.onerror = function () {
            window.setTimeout(listen, RETRY);
        };
        xhr.send();
    }

    window.calls = {
        /* The page's offer, gathered: placing the call is the core's. */
        startCall: function (offer) {
            post("/start", offer);
        },
        /* The page's answer to the call it was opened on. */
        acceptCall: function (answer) {
            post("/accept", answer);
        },
        /* The red button, in any state. The app takes the page away. */
        endCall: function () {
            post("/end", "");
        },
        /* The core's servers, as the JSON string the page parses. Asked
         * for rather than written in, so that they stay behind the key:
         * a TURN server's credentials are the account's. */
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
        /* Drawn over the page's own placeholder, which still shows where
         * the other end has no picture and the host has none to give. */
        getAvatar: function () {
            return API + "/avatar";
        }
    };

    listen();
}());
