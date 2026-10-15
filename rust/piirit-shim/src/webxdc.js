/*
 * `window.webxdc`, injected by webxdc_host.rs before app code (API and `selfAddr` ready at
 * first line); placeholders filled as JSON literals. Sends POST, receives long-poll. No Qt or
 * core access. XHR, not fetch: predates every Gecko this runs on.
 */
(function () {
    "use strict";

    /* `/webxdc-api/<token>`, host-filled; app files at root. */
    var API = __API__;
    var SELF_ADDR = __SELF_ADDR__;
    var SELF_NAME = __SELF_NAME__;
    var MAX_SIZE = __MAX_SIZE__;
    /* Ms before re-asking after failure; answered asks follow at once. */
    var RETRY = __RETRY__;

    /* Listener, and how far it has been fed. */
    var listener = null;
    var lastSerial = 0;
    /* Resolves setUpdateListener's promise once existing updates are delivered. */
    var caughtUp = null;
    /* Update request in flight; one at a time, so a send can't race it. */
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
                    /* Handler threw: keep looping, else app freezes. */
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
        /* Held until news, except while catching up. */
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
                /* Wait for next. */
                poll();
            },
            function () {
                /* Host gone; keep asking: dead listener is worse than a failure per RETRY. */
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
         * Update to whole chat, own listener too (core echoes with serial). `description`
         * ignored: core dropped it.
         */
        sendUpdate: function (update, description) {
            var body = JSON.stringify(update);
            if (MAX_SIZE > 0 && body.length > MAX_SIZE) {
                throw new Error("webxdc: update is larger than sendUpdateMaxSize");
            }
            request("POST", "/send", body,
                function () {
                    /* Host wakes waiting request so app sees own move; poll too if none. */
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
                /* Waiting request has old serial: abort, ask now. */
                if (current !== null) {
                    var waiting = current;
                    current = null;
                    waiting.abort();
                }
                poll();
            });
        },

        /*
         * Hand file and/or text out. Spec says `sendToChat`, but app draws a download: host
         * writes file, page keeps it. Settles once host has it (last known outcome). Body is
         * the file as Blob; base64-in-JSON would hold it whole at both ends.
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

    /* To host, which streams body (file only) to disk and tells page; name, text in query. */
    function handOver(name, body, words, resolve, reject) {
        var path = "/to-chat?name=" + encodeURIComponent(name)
                   + "&text=" + encodeURIComponent(words);
        request("POST", path, body === null ? "" : body,
                function () { resolve(); },
                function (err) { reject(new Error(err)); },
                "application/octet-stream");
    }

    /* base64 -> bytes via `atob` (char per byte) into Uint8Array for Blob. */
    function fromBase64(encoded) {
        var binary = window.atob(encoded);
        var bytes = new Uint8Array(binary.length);
        for (var i = 0; i < binary.length; i++) {
            bytes[i] = binary.charCodeAt(i);
        }
        return new Blob([bytes]);
    }

    /* No importFiles/joinRealtimeChannel: absent, so feature tests fall back. */
}());
