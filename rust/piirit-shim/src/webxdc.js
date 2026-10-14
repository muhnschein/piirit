/*
 * `window.webxdc` in the app's WebView, injected by webxdc_host.rs before app code: API must
 * exist at app's first line, `selfAddr` readable without waiting. Placeholders filled by host
 * as JSON literals. Sends are POSTs, receives long polls held until chat has news. Nothing here
 * talks to Qt or the core. XMLHttpRequest, not fetch: Gecko has both; XHR predates any release
 * this runs on.
 */
(function () {
    "use strict";

    /* Chat URL, `/webxdc-api/<token>`, filled in by host. App files at root. */
    var API = __API__;
    var SELF_ADDR = __SELF_ADDR__;
    var SELF_NAME = __SELF_NAME__;
    var MAX_SIZE = __MAX_SIZE__;
    /* Ms to wait before re-asking after a failed request. Answered: follow at once. */
    var RETRY = __RETRY__;

    /* Listener, and how far it has been fed. */
    var listener = null;
    var lastSerial = 0;
    /* Resolves setUpdateListener's promise once existing updates are delivered. */
    var caughtUp = null;
    /* Update request in flight. One at a time: a send must not race the waiting one. */
    var current = null;
    var timer = null;

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
        return xhr;
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
                    /* App handler threw: its problem; stopping the loop would freeze app. */
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
            }, RETRY);
        }
    }

    function poll() {
        if (current !== null) {
            return;
        }
        /* Held by host until new updates, except while catching up: answered now. */
        var path = "/updates?serial=" + lastSerial + (caughtUp ? "" : "&wait=1");
        var mine = request("GET", path, null,
            function (text) {
                if (current !== mine) {
                    return;
                }
                current = null;
                var updates = [];
                try {
                    updates = JSON.parse(text) || [];
                } catch (err) {
                    updates = [];
                }
                deliver(updates);
                settle();
                /* Back to waiting for the next. */
                poll();
            },
            function () {
                /* Host gone (app closed, or shim stopped it). Keep asking: a dead
                 * listener is worse than a request failing every RETRY ms. */
                if (current !== mine) {
                    return;
                }
                current = null;
                settle();
                schedule();
            });
        current = mine;
    }

    window.webxdc = {
        selfAddr: SELF_ADDR,
        selfName: SELF_NAME,
        /* Core's limits; apps should respect them. */
        sendUpdateInterval: __INTERVAL__,
        sendUpdateMaxSize: MAX_SIZE,

        /*
         * Send update to whole chat, incl. own listener (core echoes it with serial).
         *
         * `description` accepted and ignored: core dropped it.
         */
        sendUpdate: function (update, description) {
            var body = JSON.stringify(update);
            if (MAX_SIZE > 0 && body.length > MAX_SIZE) {
                throw new Error("webxdc: update is larger than sendUpdateMaxSize");
            }
            request("POST", "/send", body,
                function () {
                    /* Core has it: host wakes waiting request, app sees own move at once.
                     * Polls here too, for when none is waiting. */
                    poll();
                },
                function (err) {
                    if (window.console) {
                        window.console.error(err);
                    }
                });
        },

        /*
         * Updates from `serial` on. Resolves once existing ones are handed over.
         */
        setUpdateListener: function (callback, serial) {
            listener = callback;
            lastSerial = typeof serial === "number" ? serial : 0;
            return new Promise(function (resolve) {
                caughtUp = resolve;
                /* Request waiting from old serial may wait long: abort, ask now. */
                if (current !== null) {
                    var waiting = current;
                    current = null;
                    waiting.abort();
                }
                poll();
            });
        },

        /*
         * Hand a file and/or text out of the app. Spec says `sendToChat` (and warns app may not
         * come back), but app draws a download and reader means their phone: host writes file,
         * page keeps it. Settles once host has it: last moment anything here knows the outcome.
         *
         * Body is the file itself: base64 and text become a Blob here, Blob sent as is. No
         * base64-in-JSON: would hold whole file at both ends.
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

    /* Hand one over to host, which writes it out and tells the page.
     * Name and words go in query; body is only the file, copied to disk as it arrives. */
    function handOver(name, body, words, resolve, reject) {
        var path = "/to-chat?name=" + encodeURIComponent(name)
                   + "&text=" + encodeURIComponent(words);
        request("POST", path, body === null ? "" : body,
                function () { resolve(); },
                function (err) { reject(new Error(err)); },
                "application/octet-stream");
    }

    /* base64 as bytes. `atob` gives one char per byte; Uint8Array feeds Blob. */
    function fromBase64(encoded) {
        var binary = window.atob(encoded);
        var bytes = new Uint8Array(binary.length);
        for (var i = 0; i < binary.length; i++) {
            bytes[i] = binary.charCodeAt(i);
        }
        return new Blob([bytes]);
    }

    /* No importFiles or joinRealtimeChannel: absent, not failing, so feature tests fall back. */
}());
