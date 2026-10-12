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
    /* Milliseconds after the call connects, and after the camera changes
     * in it, at which what goes each way is logged: a picture the other
     * end sees black is one that is sent black, or one it cannot decode,
     * and only the counts tell which. */
    var SAMPLES = [4000, 15000, 45000];
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
    var ask = media?.getUserMedia ? media.getUserMedia.bind(media) : null;

    /* How the bridge reaches the host: post(path, body). Nowhere, until
     * the bridge attaches. */
    var send = noop;
    /* Whether the camera is wanted. The page's own query says so for the
     * start of the call, as the page itself reads it. */
    var wanted = !window.location.search.includes("noOutgoingVideoInitially")
        && !window.location.search.includes("disableVideoCompletely");
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
    /* The device the open camera is, kept when it is let go: a camera
     * turned is one that is not this. */
    var device = "";
    /* The cameras, once the engine has named them. A turn asks for one
     * by its name, and listing them costs the engine a round trip to its
     * camera service: listed again only while their names are not out,
     * as they are not before a camera has first been opened. */
    var listed = null;
    /* When the camera was last turned, while the other one is on its way
     * and drawn: what a turn takes, logged. */
    var turned = 0;
    /* Every peer connection the page has made. */
    var connections = [];
    /* The other end's stream, and what it last said about its camera. */
    var remote = null;
    var remoteEnabled = null;
    var lastReport = "";
    var lookingAgain = false;
    var looks = 0;
    var layoutDue = false;
    /* The call is connected; and which round of samples is the current
     * one, a later round taking the place of an earlier. */
    var connected = false;
    var round = 0;

    function noop() {}

    /* A line for the app's log: what only a phone shows, seen there. */
    function note(text) {
        send("/log", text);
    }

    /* What a track is, for the log. */
    function described(track) {
        var settings = track.getSettings ? track.getSettings() : {};
        return (track.label || "camera") + " " + (settings.width || "?") + "x"
            + (settings.height || "?") + "@" + (settings.frameRate || "?")
            + (settings.facingMode ? " " + settings.facingMode : "");
    }

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
        if (stream?.getVideoTracks().length === 0) {
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

    /* Whether a camera's name says it faces `which`. */
    var NAMED = {
        user: /front|user|selfie/i,
        environment: /back|rear|environment|main/i
    };

    /* The camera to open to face `which`, as a device id, or "" to let
     * the engine pick by facingMode. A facingMode is only a preference,
     * and Gecko on a phone may weigh it at nothing -- turned, the same
     * camera came back -- so a camera whose name says which way it faces
     * is asked for by its id; and turning from one, when no name says,
     * asks for any camera that is not the one that was open. */
    function cameraFor(which, avoid) {
        if (!media.enumerateDevices) {
            return Promise.resolve("");
        }
        return cameraList().then(function (cameras) {
            if (cameras.length < 2) {
                return "";
            }
            var named = cameras.filter(function (one) {
                return NAMED[which].test(one.label || "");
            });
            if (named.length > 0) {
                return named[0].deviceId;
            }
            var other = cameras.filter(function (one) {
                return one.deviceId !== avoid;
            });
            return avoid && other.length > 0 ? other[0].deviceId : "";
        }, function () {
            return "";
        });
    }

    /* The cameras the engine lists: the ones kept, once they are named. */
    function cameraList() {
        if (listed) {
            return Promise.resolve(listed);
        }
        return media.enumerateDevices().then(function (all) {
            var found = all.filter(function (one) {
                return one.kind === "videoinput" && one.deviceId;
            });
            if (found.length > 0 && found.every(function (one) { return one.label; })) {
                listed = found;
            }
            return found;
        });
    }

    /* Open the camera that faces the way asked for; `avoid` is the one
     * that was open, when it was turned from. */
    function capture(avoid) {
        if (!ask) {
            return Promise.reject(new Error("no camera"));
        }
        return cameraFor(facing, avoid).then(function (id) {
            var video = id ? { deviceId: { exact: id } } : { facingMode: { ideal: facing } };
            for (var key in CAPTURE) {
                video[key] = CAPTURE[key];
            }
            return ask({ video: video });
        }).then(function (given) {
            var track = given.getVideoTracks()[0];
            if (!track) {
                throw new Error("no camera");
            }
            var settings = track.getSettings ? track.getSettings() : {};
            device = settings.deviceId || "";
            /* What the camera says it faces wins over what was asked:
             * the picture is mirrored, and the app told, by it. */
            if (settings.facingMode === "user" || settings.facingMode === "environment") {
                facing = settings.facingMode;
            }
            note("camera open: " + described(track)
                + (turned ? ", " + (Date.now() - turned) + " ms after the turn" : ""));
            track.addEventListener("ended", function () {
                if (track === camera) {
                    note("camera ended by the engine: " + described(track));
                }
            });
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
            for (var encoding of encodings) {
                encoding.maxBitrate = MAX_BITRATE;
                encoding.maxFramerate = MAX_FRAMERATE;
            }
            var done = sender.setParameters(parameters);
            if (done?.catch) {
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
                var kind = transceiver.receiver?.track?.kind || "";
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
        for (var video of document.getElementsByTagName("video")) {
            if (video.srcObject === stream) {
                return video;
            }
        }
        return null;
    }

    /* Put `track` where the camera's track goes: on the page's stream,
     * which is what the page's own picture plays, and on every sender
     * that carries video. */
    function use(track) {
        for (var old of stream.getVideoTracks()) {
            stream.removeTrack(old);
        }
        if (track) {
            stream.addTrack(track);
        }
        connections.forEach(function (connection) {
            videoSenders(connection).forEach(function (sender) {
                var done = sender.replaceTrack(track);
                if (done?.catch) {
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
            lookAgain("the page never drew it");
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
        start(picture);
    }

    /* Look at this end's picture again in a moment. One look at a time,
     * however often this is asked: each ask starting its own would spend
     * the looks in a fraction of the time, and a camera slow to open
     * would find none left. Once they are spent, `why` is logged. */
    function lookAgain(why) {
        if (!stream || layoutDue) {
            return;
        }
        if (looks < LOOKS) {
            looks += 1;
            layoutDue = true;
            window.setTimeout(function () {
                layoutDue = false;
                layout();
            }, AGAIN);
        } else if (looks === LOOKS) {
            looks += 1;
            note("our picture: " + why);
        }
    }

    /* The page's picture of this end plays by itself, muted -- when the
     * engine lets it: given the stream while the call screen still hid
     * the page, it was seen to sit there paused until something gave it
     * the stream again. So it is asked to play, and looked at again,
     * until it does; as the page itself does with the other end's. */
    function start(picture) {
        if (!picture.paused || !picture.play) {
            return;
        }
        var playing = picture.play();
        if (playing?.catch) {
            playing.catch(noop);
        }
        lookAgain("it never played");
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
        note("camera would not open: " + (failure || "no reason given"));
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
    function open(otherwise, avoid) {
        opening = true;
        capture(avoid).then(function (track) {
            opening = false;
            if (!wanted || !stream) {
                turned = 0;
                track.stop();
                return;
            }
            track.enabled = true;
            use(track);
            camera = track;
            failed = false;
            timeDrawing();
            report();
            sampleSoon();
        }, function (err) {
            opening = false;
            turned = 0;
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
        var was = device;
        facing = which;
        if (!camera || opening) {
            report();
            return;
        }
        turned = Date.now();
        camera.stop();
        open(function (err) {
            note("camera would not turn: " + describe(err));
            facing = before;
            open(giveUp);
        }, was);
    }

    /* After a turn, log when the other camera's first picture is drawn:
     * given the stream again, the page's picture loads it afresh, and has
     * its data once a frame has come. The rest of what a turn takes is
     * the phone's camera starting up, which `camera open` times. */
    function timeDrawing() {
        var since = turned;
        turned = 0;
        var picture = preview();
        if (!since || !picture?.addEventListener) {
            return;
        }
        picture.addEventListener("loadeddata", function () {
            note("camera drawn: " + (Date.now() - since) + " ms after the turn");
        }, { once: true });
    }

    /* Each picture section of a description, for the log: which way it
     * goes, and its codecs in the order given. */
    function videoSections(description) {
        if (!description?.sdp) {
            return "none";
        }
        var found = [];
        var names = {};
        var current = null;
        description.sdp.split(/\r?\n/).forEach(function (line) {
            var media = /^m=(\w+) (\d+) [^ ]+ (.*)$/.exec(line);
            if (media) {
                current = media[1] === "video"
                    ? { port: media[2], types: media[3].split(" "), way: "" }
                    : null;
                if (current) {
                    found.push(current);
                }
                return;
            }
            var map = /^a=rtpmap:(\d+) ([^/]+)/.exec(line);
            if (map) {
                names[map[1]] = map[2];
            }
            var way = /^a=(sendrecv|sendonly|recvonly|inactive)$/.exec(line);
            if (way && current) {
                current.way = way[1];
            }
        });
        return found.map(function (section) {
            var codecs = section.types.map(function (type) {
                return names[type] || type;
            }).filter(function (name) {
                return !/^(rtx|red|ulpfec)$/i.test(name);
            });
            return [section.port === "0" ? "rejected" : "", section.way, codecs.join(",")]
                .filter(Boolean).join(" ");
        }).join("; ") || "none";
    }

    /* Which track a sender sends, for the log. */
    function sending(sender) {
        var track = sender.track;
        if (!track) {
            return "nothing";
        }
        var which = "other";
        if (track === camera) {
            which = "camera";
        } else if (track === blank) {
            which = "blank";
        }
        return which + " " + (track.enabled ? "on" : "off") + " " + track.readyState;
    }

    /* One way's counts, from its RTP stats: the codec, the size, how
     * many frames and how big each is -- a black frame is a few dozen
     * bytes, a picture thousands -- and how often a keyframe was asked
     * for, which a receiver that cannot decode does over and over. */
    function counts(rtp, codecs, frames, bytes) {
        if (!rtp) {
            return "none";
        }
        var parts = [];
        var codec = codecs[rtp.codecId];
        if (codec) {
            parts.push(codec.replace(/^video\//, ""));
        }
        if (rtp.frameWidth) {
            parts.push(rtp.frameWidth + "x" + rtp.frameHeight);
        }
        var n = rtp[frames] || 0;
        parts.push(n + " frames");
        if (n > 0) {
            parts.push(Math.round((rtp[bytes] || 0) / n) + " B/frame");
        }
        if (rtp.keyFramesEncoded !== undefined) {
            parts.push(rtp.keyFramesEncoded + " key");
        }
        parts.push("PLI " + (rtp.pliCount || 0) + " FIR " + (rtp.firCount || 0)
            + " NACK " + (rtp.nackCount || 0));
        if (rtp.packetsLost !== undefined) {
            parts.push("lost " + rtp.packetsLost);
        }
        return parts.join(" ");
    }

    /* Log what one connection sends and receives of video, now. */
    function sample(connection) {
        if (!connection.getStats) {
            return;
        }
        var senders = videoSenders(connection).map(sending).join(", ") || "no sender";
        var picture = preview();
        var shown = "not drawn";
        if (picture) {
            shown = (picture.paused ? "paused " : "playing ")
                + (picture.videoWidth || 0) + "x" + (picture.videoHeight || 0);
        }
        connection.getStats(null).then(function (stats) {
            var sent = null;
            var got = null;
            var codecs = {};
            stats.forEach(function (one) {
                if (one.type === "codec") {
                    codecs[one.id] = one.mimeType || "";
                }
                if ((one.kind || one.mediaType) !== "video" || one.isRemote) {
                    return;
                }
                if (one.type === "outbound-rtp") {
                    sent = one;
                } else if (one.type === "inbound-rtp") {
                    got = one;
                }
            });
            note("video sent: " + counts(sent, codecs, "framesEncoded", "bytesSent")
                + " (" + senders + "; ours " + shown + ") | got: "
                + counts(got, codecs, "framesDecoded", "bytesReceived"));
        }, function (err) {
            note("video stats: " + describe(err));
        });
    }

    /* Log what goes each way a few times from now, in place of any
     * round still to come. Only on a connected call: before, nothing
     * goes. */
    function sampleSoon() {
        if (!connected) {
            return;
        }
        round += 1;
        var mine = round;
        SAMPLES.forEach(function (delay) {
            window.setTimeout(function () {
                if (mine === round) {
                    connections.forEach(sample);
                }
            }, delay);
        });
    }

    function watch(connection) {
        connections.push(connection);
        connection.addEventListener("iceconnectionstatechange", function () {
            var state = connection.iceConnectionState;
            if ((state === "connected" || state === "completed") && !connected) {
                connected = true;
                sampleSoon();
            }
        });
        connection.addEventListener("track", function (event) {
            if (event.streams?.[0]) {
                remote = event.streams[0];
            }
            var track = event.track;
            if (track?.kind === "video") {
                note("their picture: track " + (track.muted ? "muted" : "live")
                    + (event.streams?.[0] ? "" : ", in no stream"));
                track.addEventListener("mute", function () {
                    note("their picture: no frames");
                    report();
                });
                track.addEventListener("unmute", function () {
                    note("their picture: frames");
                    report();
                });
                track.addEventListener("ended", report);
            }
            report();
        });
        /* Negotiated: there are encodings to hold to the caps. */
        connection.addEventListener("signalingstatechange", function () {
            if (connection.signalingState === "stable") {
                note("video, ours: " + videoSections(connection.localDescription)
                    + " | theirs: " + videoSections(connection.remoteDescription));
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
            if (!asked?.video) {
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

    /* The page puts the stream on its picture when it draws it: placed
     * then, rather than whenever the next look comes round. */
    var Media = window.HTMLMediaElement;
    var source = Media && Object.getOwnPropertyDescriptor
        ? Object.getOwnPropertyDescriptor(Media.prototype, "srcObject") : null;
    if (source?.set && source.configurable) {
        Object.defineProperty(Media.prototype, "srcObject", {
            configurable: true,
            enumerable: source.enumerable,
            get: source.get,
            set: function (value) {
                source.set.call(this, value);
                if (value && value === stream) {
                    window.setTimeout(layout, 0);
                }
            }
        });
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
