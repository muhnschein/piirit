/*
 * Video for calls-webapp: the camera, and what is drawn of it and of
 * the other end, as the app's own call screen has them.
 *
 * Served ahead of the bridge (`calls.js`), as one script with it: the
 * bridge hands this its way to the host, every peer connection the page
 * makes, and the commands that are this file's -- the camera on or off,
 * which way it faces, and whether the app is in front. It reports back,
 * on `/video`, whether the camera is live and whether the other end's
 * picture is being drawn: what the call screen decides from whether the
 * page is shown at all.
 *
 * The page asks for the camera once, as the call starts, and keeps it
 * for the whole call. A voice call would then light the camera for
 * nothing, and a camera switched off would stay open behind a disabled
 * track. So the page is handed a stream whose video track is a blank one
 * -- a canvas that draws no frames -- until the camera is wanted, and
 * the camera's own track is put in its place, on the page's stream and
 * on the peer connection's sender, when it is. Switched off, or turned
 * the other way, the camera is let go before anything else is opened:
 * a phone opens one camera at a time.
 *
 * The page keeps whether its camera is on, and tells the other end --
 * which hides a picture that has been switched off rather than freezing
 * it -- so its own switch is pressed, found by its label, as the bridge
 * presses the microphone's.
 *
 * What is captured is held to what a phone's screen at the other end
 * needs: VGA at 24 frames, at most 700 kbit/s. Encoding is software in
 * the engine, and every pixel and frame not taken is battery kept.
 *
 * The page's own controls are not drawn: the app draws its own over the
 * page, and every element but the pictures is hidden. Where the page
 * would put this end's picture -- a fixed square, mirrored whichever way
 * the camera faces -- it is put where the call screen leaves room for it,
 * the whole screen while there is no picture from the other end.
 */
var piiritVideo = (function () {
    "use strict";

    /* What the camera is asked for: preferences only. A required value
     * -- a max, a min, an exact -- is one every mode of the camera has to
     * meet or the engine opens nothing, and Gecko holds a frame rate
     * against the most a mode can do: a phone camera that runs at 30
     * fails `max: 24` in every mode. The senders are held to 24 instead
     * (`cap`). */
    var CAPTURE = {
        width: { ideal: 640 },
        height: { ideal: 480 },
        frameRate: { ideal: 24 }
    };
    /* What one encoding may send. */
    var MAX_BITRATE = 700000;
    var MAX_FRAMERATE = 24;
    /* The page's camera switch, labelled with what a press would do. */
    var START_LABEL = "Start camera";
    var STOP_LABEL = "Stop camera";
    /* Milliseconds between looks for something the page has not drawn
     * yet, and how many looks before giving up. */
    var AGAIN = 250;
    var LOOKS = 40;
    /* This end's picture, while the other end's is drawn: in the corner
     * the call screen leaves free under its top line. */
    var CORNER = {
        top: "10vh", right: "4vw", bottom: "auto", left: "auto",
        width: "27vw", height: "36vw", borderRadius: "2vw"
    };
    /* And the whole screen, while it is not. */
    var WHOLE = {
        top: "0", right: "0", bottom: "0", left: "0",
        width: "100%", height: "100%", borderRadius: "0"
    };

    var media = navigator.mediaDevices;
    var ask = media && media.getUserMedia ? media.getUserMedia.bind(media) : null;

    /* How the bridge reaches the host: post(path, body). */
    var send = function () {};
    /* Whether the camera is wanted. The page's own query says so for the
     * start of the call, as the page itself reads it. */
    var wanted = window.location.search.indexOf("noOutgoingVideoInitially") < 0
        && window.location.search.indexOf("disableVideoCompletely") < 0;
    /* Which way the camera faces: "user" or "environment". */
    var facing = "user";
    /* The last camera asked for would not open, and what the engine said
     * of it. */
    var failed = false;
    var failure = "";
    /* The stream handed to the page, once it has asked. */
    var stream = null;
    /* The blank track that stands in for the camera. */
    var blank = null;
    /* The camera's track, while it is open; and a camera on its way. */
    var camera = null;
    var opening = false;
    /* Every peer connection the page has made. */
    var connections = [];
    /* The other end's stream, and what it last said about its camera. */
    var remote = null;
    var remoteEnabled = null;
    var lastReport = "";
    var lookingAgain = false;
    var looks = 0;

    function noop() {}

    function labelled(label) {
        return document.querySelector("[aria-label=\"" + label + "\"]");
    }

    /* The page's switch, pressed until it says what is wanted. Nothing
     * to press before the page has drawn it, nor ever on a stream with
     * no video track at all, where the page draws no switch. */
    function syncSwitch() {
        var press = labelled(wanted ? START_LABEL : STOP_LABEL);
        if (press) {
            press.click();
            return;
        }
        if (labelled(wanted ? STOP_LABEL : START_LABEL) || lookingAgain) {
            return;
        }
        if (stream && stream.getVideoTracks().length === 0) {
            return;
        }
        lookingAgain = true;
        window.setTimeout(function () {
            lookingAgain = false;
            syncSwitch();
        }, AGAIN);
    }

    /* A video track that sends nothing: a canvas never asked for a frame.
     * Null where the engine cannot make one. */
    function placeholder() {
        if (blank && blank.readyState !== "ended") {
            blank.enabled = false;
            return blank;
        }
        var canvas = document.createElement("canvas");
        if (!canvas.captureStream) {
            return null;
        }
        canvas.width = 2;
        canvas.height = 2;
        var context = canvas.getContext("2d");
        if (context) {
            context.fillRect(0, 0, 2, 2);
        }
        blank = canvas.captureStream(0).getVideoTracks()[0] || null;
        if (blank) {
            blank.enabled = false;
        }
        return blank;
    }

    /* Open the camera that faces the way asked for. */
    function capture() {
        if (!ask) {
            return Promise.reject(new Error("no camera"));
        }
        var video = { facingMode: { ideal: facing } };
        for (var key in CAPTURE) {
            video[key] = CAPTURE[key];
        }
        return ask({ video: video }).then(function (given) {
            var track = given.getVideoTracks()[0];
            if (!track) {
                throw new Error("no camera");
            }
            return track;
        });
    }

    /* Hold what one sender sends to the caps above. Before negotiation
     * there is nothing to set, and an engine that will not take a value
     * keeps its own: neither is worth a call. */
    function cap(sender) {
        if (!sender.getParameters || !sender.setParameters) {
            return;
        }
        try {
            var parameters = sender.getParameters();
            var encodings = parameters.encodings || [];
            if (encodings.length === 0) {
                return;
            }
            for (var i = 0; i < encodings.length; i++) {
                encodings[i].maxBitrate = MAX_BITRATE;
                encodings[i].maxFramerate = MAX_FRAMERATE;
            }
            var done = sender.setParameters(parameters);
            if (done && done.catch) {
                done.catch(noop);
            }
        } catch (err) {
            /* Kept as it was. */
        }
    }

    /* The senders that carry video, whether or not they have a track. */
    function videoSenders(connection) {
        var found = [];
        if (connection.getTransceivers) {
            connection.getTransceivers().forEach(function (transceiver) {
                var kind = transceiver.receiver && transceiver.receiver.track
                    ? transceiver.receiver.track.kind : "";
                if (kind === "video" && !transceiver.stopped) {
                    found.push(transceiver.sender);
                }
            });
            return found;
        }
        return connection.getSenders().filter(function (sender) {
            return sender.track !== null && sender.track.kind === "video";
        });
    }

    /* This end's picture element: the one playing the page's stream. */
    function preview() {
        if (!stream) {
            return null;
        }
        var videos = document.getElementsByTagName("video");
        for (var i = 0; i < videos.length; i++) {
            if (videos[i].srcObject === stream) {
                return videos[i];
            }
        }
        return null;
    }

    /* Put `track` where the camera's track goes: on the page's stream,
     * which is what the page's own picture plays, and on every sender
     * that carries video. */
    function use(track) {
        var old = stream.getVideoTracks();
        for (var i = 0; i < old.length; i++) {
            stream.removeTrack(old[i]);
        }
        if (track) {
            stream.addTrack(track);
        }
        connections.forEach(function (connection) {
            videoSenders(connection).forEach(function (sender) {
                var done = sender.replaceTrack(track);
                if (done && done.catch) {
                    done.catch(noop);
                }
                cap(sender);
            });
        });
        /* The picture plays the stream it was given when it was given
         * it: given it again, it plays what is on it now. */
        var picture = preview();
        if (picture) {
            picture.srcObject = stream;
        }
    }

    function remoteShown() {
        if (!remote || remoteEnabled === false) {
            return false;
        }
        return remote.getVideoTracks().some(function (track) {
            return !track.muted && track.readyState !== "ended";
        });
    }

    function place(element, where) {
        for (var key in where) {
            element.style[key] = where[key];
        }
    }

    /* Draw this end's picture where it goes: mirrored only from the
     * camera that faces the reader, the whole screen until the other
     * end's picture comes, the corner after, and nowhere without a
     * camera. Before the page has put the stream on its picture there is
     * nothing to place, and it is looked for again. */
    function layout() {
        var picture = preview();
        if (!picture) {
            if (stream && looks < LOOKS) {
                looks += 1;
                window.setTimeout(layout, AGAIN);
            }
            return;
        }
        picture.style.transform = facing === "user" ? "scaleX(-1)" : "none";
        var box = picture.parentElement;
        if (!box) {
            return;
        }
        if (!camera) {
            box.style.display = "none";
            return;
        }
        box.style.display = "block";
        /* The page shows its pictures only once the call is up; this
         * end's is shown from the start, as a phone's call screen does. */
        if (box.parentElement) {
            box.parentElement.style.setProperty("display", "block", "important");
        }
        var whole = !remoteShown();
        place(box, whole ? WHOLE : CORNER);
        box.style.boxShadow = whole ? "none" : "";
    }

    /* Say how the pictures stand, when that has changed. */
    function report() {
        layout();
        var state = {
            local: camera !== null,
            remote: remoteShown(),
            front: facing === "user",
            failed: failed
        };
        if (failed) {
            state.error = failure;
        }
        var now = JSON.stringify(state);
        if (now === lastReport) {
            return;
        }
        lastReport = now;
        send("/video", now);
    }

    /* What an engine's refusal says: its name, which is the reason, and
     * its message. */
    function describe(err) {
        if (!err) {
            return "";
        }
        var name = err.name && err.name !== "Error" ? err.name : "";
        var message = err.message || "";
        return name && message ? name + ": " + message : name || message;
    }

    /* The camera would not open: the page is told it is off, and the app
     * that it failed, and why. */
    function giveUp(err) {
        failed = true;
        failure = describe(err);
        wanted = false;
        if (camera) {
            camera.stop();
            camera = null;
        }
        if (stream) {
            use(placeholder());
        }
        syncSwitch();
        report();
    }

    /* Open the camera and put it in place; `otherwise` if it will not.
     * A camera switched off while it was opening is let go again. */
    function open(otherwise) {
        opening = true;
        capture().then(function (track) {
            opening = false;
            if (!wanted || !stream) {
                track.stop();
                return;
            }
            track.enabled = true;
            use(track);
            camera = track;
            failed = false;
            report();
        }, function (err) {
            opening = false;
            if (wanted) {
                otherwise(err);
            }
        });
    }

    function cameraOn() {
        wanted = true;
        /* Asked again after a failure: said, so that a second failure
         * is a change the app hears too. */
        if (failed) {
            failed = false;
            report();
        }
        syncSwitch();
        if (camera || opening || !stream) {
            return;
        }
        open(giveUp);
    }

    function cameraOff() {
        wanted = false;
        failed = false;
        syncSwitch();
        if (camera) {
            var old = camera;
            camera = null;
            use(placeholder());
            old.stop();
        }
        report();
    }

    /* Turn the camera the other way: the open one is let go first, and
     * if the other will not open, the one before is opened again. */
    function face(which) {
        if (facing === which) {
            return;
        }
        var before = facing;
        facing = which;
        if (!camera || opening) {
            report();
            return;
        }
        camera.stop();
        open(function () {
            facing = before;
            open(giveUp);
        });
    }

    function watch(connection) {
        connections.push(connection);
        connection.addEventListener("track", function (event) {
            if (event.streams && event.streams[0]) {
                remote = event.streams[0];
            }
            var track = event.track;
            if (track && track.kind === "video") {
                track.addEventListener("mute", report);
                track.addEventListener("unmute", report);
                track.addEventListener("ended", report);
            }
            report();
        });
        /* Negotiated: there are encodings to hold to the caps. */
        connection.addEventListener("signalingstatechange", function () {
            if (connection.signalingState === "stable") {
                videoSenders(connection).forEach(cap);
            }
        });
        /* What the other end says about its camera, on the channel the
         * page opens for it. */
        var create = connection.createDataChannel;
        connection.createDataChannel = function (label, options) {
            var channel = create.call(connection, label, options);
            if (label === "mutedState") {
                channel.addEventListener("message", function (event) {
                    try {
                        var said = JSON.parse(event.data);
                        if (said && typeof said.videoEnabled === "boolean") {
                            remoteEnabled = said.videoEnabled;
                            report();
                        }
                    } catch (err) {
                        /* The page says what was wrong with it. */
                    }
                });
            }
            return channel;
        };
    }

    /* One command from the host. False for one that is not this file's. */
    function take(command) {
        switch (command) {
        case "camera-on":
            cameraOn();
            return true;
        case "camera-off":
            cameraOff();
            return true;
        case "facing-user":
            face("user");
            return true;
        case "facing-environment":
            face("environment");
            return true;
        case "hidden":
            /* In the background: the pictures are not drawn, and what
             * they carry -- the other end's voice -- still plays. */
            document.documentElement.classList.add("piirit-hidden");
            return true;
        case "shown":
            document.documentElement.classList.remove("piirit-hidden");
            return true;
        default:
            return false;
        }
    }

    /* The page asks for the camera and the microphone together, once.
     * It is given the microphone, and the camera only if it is wanted --
     * the blank track otherwise, so that its switch is drawn and the
     * call can be turned into a video call. A camera that will not open
     * is not a call that cannot start. */
    if (ask) {
        media.getUserMedia = function (asked) {
            if (!asked || !asked.video) {
                return ask(asked);
            }
            return ask({ audio: asked.audio === undefined ? true : asked.audio })
                .then(function (given) {
                    stream = given;
                    if (!wanted) {
                        var stand = placeholder();
                        if (stand) {
                            given.addTrack(stand);
                        }
                        report();
                        return given;
                    }
                    opening = true;
                    return capture().then(function (track) {
                        opening = false;
                        if (!wanted) {
                            track.stop();
                            track = placeholder();
                        } else {
                            camera = track;
                        }
                        if (track) {
                            given.addTrack(track);
                        }
                        report();
                        return given;
                    }, function (err) {
                        opening = false;
                        var stand = placeholder();
                        if (stand) {
                            given.addTrack(stand);
                        }
                        giveUp(err);
                        return given;
                    });
                });
        };
    }

    /* Nothing of the page but its pictures: the app draws the rest. */
    var sheet = document.createElement("style");
    sheet.textContent = "body * { visibility: hidden !important; }\n"
        + "video { visibility: visible !important; }\n"
        + "html.piirit-hidden video { visibility: hidden !important; }\n";
    (document.head || document.documentElement).appendChild(sheet);

    return {
        /* How to reach the host: the bridge's own post(path, body). */
        attach: function (post) {
            send = post;
        },
        watch: watch,
        take: take
    };
}());
