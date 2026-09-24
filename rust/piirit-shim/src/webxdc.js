/*
 * `window.webxdc`, as the app running inside the WebView sees it.
 *
 * Served by the shim's own loopback host (webxdc_host.rs), which injects a
 * script tag for it into the app's index.html before anything of the app's
 * own runs -- the API has to be there when the first line of the app
 * executes, and `selfAddr` has to be readable without waiting.
 *
 * The placeholders are filled in by the host as JSON literals.
 *
 * Updates travel over the same host: sending is a POST, receiving is a
 * poll. Nothing here talks to Qt -- a page cannot -- and nothing here
 * knows the core; both are on the other side of those two requests.
 * The realtime channel is the same again, with a long poll for what the
 * others send, since a game cannot wait out a timer for every move.
 *
 * XMLHttpRequest rather than fetch: Gecko has both, and this one is
 * older than every release this could run on.
 */
(function () {
    "use strict";

    /* Where the chat is: `/webxdc-api/<token>`, which the host fills
     * in. The app's own files are at the root; this is the part of
     * the host nothing but the app is told about. */
    var API = __API__;
    var SELF_ADDR = __SELF_ADDR__;
    var SELF_NAME = __SELF_NAME__;
    var MAX_SIZE = __MAX_SIZE__;
    /* Milliseconds between polls for updates from the other end. */
    var POLL = __POLL__;
    /* The largest piece of realtime data, per the specification. */
    var MAX_REALTIME = 128000;
    /* Realtime sends that may wait behind the one in flight. Past this
     * the oldest goes: a channel promises no delivery, and what a game
     * sends is its state now, which the newest says best. */
    var MAX_REALTIME_WAITING = 256;

    /* What setUpdateListener was given, and how far it has been fed. */
    var listener = null;
    var lastSerial = 0;
    /* Resolves the promise setUpdateListener returned, once the updates
     * that existed when it was called have all been delivered. */
    var caughtUp = null;
    /* One poll at a time: a send asks for one immediately, and that must
     * not race the timer's. */
    var polling = false;
    var timer = null;

    /* The realtime channel, while the app is in it. */
    var channel = null;
    /* Joining, sending and leaving, in the order the app asked for them
     * and one at a time: each is a request of its own, and requests on
     * separate connections can reach the host in any order -- a send
     * overtaking the join it followed, or a join the leave before it. */
    var realtimeQueue = [];
    var realtimeBusy = false;

    function request(method, path, body, onDone, onFail, type) {
        var xhr = new XMLHttpRequest();
        xhr.open(method, API + path, true);
        xhr.onload = function () {
            if (xhr.status >= 200 && xhr.status < 300) {
                onDone(xhr.responseText);
            } else {
                onFail(new Error("webxdc: the app host answered " + xhr.status));
            }
        };
        xhr.onerror = function () {
            onFail(new Error("webxdc: the app host could not be reached"));
        };
        if (body === null) {
            xhr.send();
        } else {
            xhr.setRequestHeader("Content-Type",
                                 type || "application/json");
            xhr.send(body);
        }
    }

    function settle() {
        if (caughtUp) {
            var resolve = caughtUp;
            caughtUp = null;
            resolve();
        }
    }

    function deliver(updates) {
        for (var i = 0; i < updates.length; i++) {
            var update = updates[i];
            if (typeof update.serial === "number" && update.serial > lastSerial) {
                lastSerial = update.serial;
            }
            if (listener) {
                try {
                    listener(update);
                } catch (err) {
                    /* The app's own handler threw. Its problem, not the
                     * next update's: one that stopped the loop would
                     * leave the app frozen at the update it choked on. */
                    if (window.console) {
                        window.console.error(err);
                    }
                }
            }
        }
    }

    function schedule() {
        if (timer === null) {
            timer = window.setTimeout(function () {
                timer = null;
                poll();
            }, POLL);
        }
    }

    function poll() {
        if (polling) {
            return;
        }
        polling = true;
        request("GET", "/updates?serial=" + lastSerial, null,
            function (text) {
                polling = false;
                var updates = [];
                try {
                    updates = JSON.parse(text) || [];
                } catch (err) {
                    updates = [];
                }
                deliver(updates);
                settle();
                schedule();
            },
            function () {
                /* The host is gone -- the app was closed, or the shim
                 * stopped it. Keep asking: nothing else here can tell
                 * the difference between that and a lost connection,
                 * and an app left running with a dead listener is
                 * worse than a request that fails every POLL ms. */
                polling = false;
                settle();
                schedule();
            });
    }

    /* Queue one realtime request; `done` runs once the host has answered
     * it, however it answered. */
    function realtime(path, body, done) {
        if (path === "/realtime/send") {
            var waiting = 0;
            var oldest = -1;
            for (var i = 0; i < realtimeQueue.length; i++) {
                if (realtimeQueue[i].path === path) {
                    waiting++;
                    if (oldest < 0) {
                        oldest = i;
                    }
                }
            }
            if (waiting >= MAX_REALTIME_WAITING) {
                realtimeQueue.splice(oldest, 1);
            }
        }
        realtimeQueue.push({ path: path, body: body, done: done });
        nextRealtime();
    }

    function nextRealtime() {
        if (realtimeBusy || realtimeQueue.length === 0) {
            return;
        }
        realtimeBusy = true;
        var next = realtimeQueue.shift();
        function finished() {
            realtimeBusy = false;
            if (next.done) {
                next.done();
            }
            nextRealtime();
        }
        request("POST", next.path, next.body, finished, function (err) {
            if (window.console) {
                window.console.error(err);
            }
            finished();
        }, "application/octet-stream");
    }

    /*
     * One stay in the realtime channel, from joinRealtimeChannel to
     * leave(). A new object for every stay, as the specification has it:
     * once left, this one is spent, and joining again is a new one.
     */
    function RealtimeChannel() {
        var self = this;
        var listener = null;
        var left = false;

        /* Collect what the others have sent, and ask again: the host
         * holds each request until there is something to answer it
         * with, so this is not the busy loop it looks like. */
        function listen() {
            if (left) {
                return;
            }
            request("GET", "/realtime/receive", null,
                function (text) {
                    var items = [];
                    try {
                        items = JSON.parse(text) || [];
                    } catch (err) {
                        items = [];
                    }
                    for (var i = 0; i < items.length && !left; i++) {
                        if (listener) {
                            try {
                                listener(new Uint8Array(items[i]));
                            } catch (err) {
                                /* As for updates: the app's handler
                                 * throwing is no reason to stop
                                 * listening. */
                                if (window.console) {
                                    window.console.error(err);
                                }
                            }
                        }
                    }
                    listen();
                },
                function () {
                    /* The host is gone or failed: ask again, but not
                     * at once, or a closed app spins here. */
                    if (!left) {
                        window.setTimeout(listen, POLL);
                    }
                });
        }

        /* Replaces the one before, if there was one. */
        this.setListener = function (callback) {
            listener = callback;
        };

        /* Send to whoever is in the channel. Nobody may be. */
        this.send = function (data) {
            if (left) {
                throw new Error("webxdc: the realtime channel has been left");
            }
            if (!(data instanceof Uint8Array)) {
                throw new Error("webxdc: realtime data must be a Uint8Array");
            }
            if (data.length > MAX_REALTIME) {
                throw new Error("webxdc: realtime data is larger than "
                                + MAX_REALTIME + " bytes");
            }
            /* A copy: the app may well fill the same array again for
             * its next send before this one has gone. */
            realtime("/realtime/send", new Uint8Array(data), null);
        };

        this.leave = function () {
            if (left) {
                return;
            }
            left = true;
            listener = null;
            if (channel === self) {
                channel = null;
            }
            realtime("/realtime/leave", "", null);
        };

        /* Listening starts once the host has the join, so the first
         * request is one for this stay and not the last one's. */
        realtime("/realtime/join", "", listen);
    }

    window.webxdc = {
        selfAddr: SELF_ADDR,
        selfName: SELF_NAME,
        /* The core's own limits, which apps are expected to respect. */
        sendUpdateInterval: __INTERVAL__,
        sendUpdateMaxSize: MAX_SIZE,

        /*
         * Send one update to everyone in the chat, including this app's
         * own listener -- the core echoes it back with a serial.
         *
         * `description` is accepted and ignored: the core dropped it, and
         * an app that passes one is not wrong for having been written
         * against the older API.
         */
        sendUpdate: function (update, description) {
            var body = JSON.stringify(update);
            if (MAX_SIZE > 0 && body.length > MAX_SIZE) {
                throw new Error("webxdc: update is larger than sendUpdateMaxSize");
            }
            request("POST", "/send", body,
                function () {
                    /* The core has it; ask for it back rather than
                     * waiting out the poll, so the app sees its own
                     * move at once. */
                    poll();
                },
                function (err) {
                    if (window.console) {
                        window.console.error(err);
                    }
                });
        },

        /*
         * Take every update from `serial` on, and every one after.
         * Resolves once the updates that already existed have been
         * handed over, which is what apps wait on before drawing.
         */
        setUpdateListener: function (callback, serial) {
            listener = callback;
            lastSerial = typeof serial === "number" ? serial : 0;
            return new Promise(function (resolve) {
                caughtUp = resolve;
                poll();
            });
        },

        /*
         * Deprecated in favour of setUpdateListener, and still what apps
         * from before it call first thing: absent, that call throws and
         * the app never draws. Empty, as every other messenger answers
         * it: such an app pairs it with setUpdateListener, which hands
         * over everything from the start anyway, and a real list here
         * would have it apply each update twice.
         */
        getAllUpdates: function () {
            return Promise.resolve([]);
        },

        /*
         * Join the chat's realtime channel: data that reaches whoever
         * has the app open right now and nobody else, which is what a
         * game played together sends its moves over. One at a time, as
         * the specification says -- joining again without leaving is
         * an error.
         */
        joinRealtimeChannel: function () {
            if (channel !== null) {
                throw new Error("webxdc: already in the realtime channel; leave it first");
            }
            channel = new RealtimeChannel();
            return channel;
        },

        /*
         * Hand a file, a piece of text, or both out of the app.
         *
         * The specification calls this `sendToChat` and warns the app
         * that it may not come back. What the reader is asked is not
         * which chat, though: an app draws a *download* over this, and
         * what they mean by it is the file on their phone. So the host
         * writes the file out and the page keeps it. The promise settles
         * once the host has it, which is the last moment anything here
         * still knows what happened.
         *
         * A file arrives in one of three shapes and leaves as the
         * request body itself: base64 and text are turned into a Blob
         * here, and a Blob is sent as it is. Nothing is encoded on the
         * way out: reading the file into a base64 string and wrapping it
         * in JSON would hold the whole of it at both ends, and a file
         * worth exporting is exactly the size that cannot afford that.
         */
        sendToChat: function (message) {
            return new Promise(function (resolve, reject) {
                var content = message || {};
                var file = content.file;
                var words = typeof content.text === "string" ? content.text : "";
                if (!file && words.length === 0) {
                    reject(new Error("webxdc: nothing to send"));
                    return;
                }
                if (!file) {
                    handOver("", null, words, resolve, reject);
                    return;
                }
                if (typeof file.name !== "string" || file.name.length === 0) {
                    reject(new Error("webxdc: a file needs a name"));
                    return;
                }
                var body = null;
                try {
                    if (typeof file.base64 === "string") {
                        body = fromBase64(file.base64);
                    } else if (typeof file.plainText === "string") {
                        body = new Blob([file.plainText]);
                    } else if (file.blob) {
                        body = file.blob;
                    }
                } catch (err) {
                    reject(new Error("webxdc: the file could not be read"));
                    return;
                }
                if (body === null) {
                    reject(new Error("webxdc: a file needs a blob, base64 or plainText"));
                    return;
                }
                handOver(file.name, body, words, resolve, reject);
            });
        }
    };

    /* Hand one over to the host, which writes it out and tells the page.
     *
     * The name and the words go in the query, so the body is the file and
     * only the file: the host copies it onto the disk as it arrives and
     * never holds it. */
    function handOver(name, body, words, resolve, reject) {
        var path = "/to-chat?name=" + encodeURIComponent(name)
                   + "&text=" + encodeURIComponent(words);
        request("POST", path, body === null ? "" : body,
                function () { resolve(); },
                function (err) { reject(new Error(err)); },
                "application/octet-stream");
    }

    /* base64 as the bytes it stands for. `atob` gives one character per
     * byte, and a Uint8Array of those is what a Blob takes. */
    function fromBase64(encoded) {
        var binary = window.atob(encoded);
        var bytes = new Uint8Array(binary.length);
        for (var i = 0; i < binary.length; i++) {
            bytes[i] = binary.charCodeAt(i);
        }
        return new Blob([bytes]);
    }

    /* Nothing else is offered: importFiles is absent rather than present
     * and failing, so an app that feature-tests for it takes its own
     * other path. */
}());
