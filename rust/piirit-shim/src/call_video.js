/*
 * Video for calls-webapp: camera, and drawing both pictures as app's call screen wants.
 *
 * Served ahead of bridge (`calls.js`), one script. Bridge hands in host post, every peer
 * connection, and commands: camera on/off, facing, app in front, mic shut/reopen around sound
 * move. Reports on `/video` camera live / remote drawn: call screen shows page from that.
 *
 * Page asks camera once, keeps it all call: voice call would light camera, switched-off one
 * stay open behind disabled track. So page gets blank track (canvas, no frames) until camera
 * wanted; then camera track replaces it on stream and senders. Off/turn: camera let go before
 * another opens (phone opens one at a time).
 *
 * Page keeps camera state, tells other end (hides off picture, not freezes): its switch
 * pressed, found by label, as bridge does mic's.
 *
 * Capture: VGA 24 fps, max 700 kbit/s; software encode, so less = battery kept.
 *
 * Page controls hidden (app draws own); own picture (page: square, mirrored either facing)
 * where call screen leaves room, full screen while no remote picture.
 */
var piiritVideo = (function () {
    "use strict";

    /* Preferences only: required value (max/min/exact) must suit every mode or nothing
     * opens; Gecko checks rate vs mode's most, so 30 fps fails `max: 24`. Senders capped (`cap`). */
    var CAPTURE = {
        width: { ideal: 640 },
        height: { ideal: 480 },
        frameRate: { ideal: 24 }
    };
    /* Max send per encoding. */
    var MAX_BITRATE = 700000;
    var MAX_FRAMERATE = 24;
    /* Page camera switch label. */
    var START_LABEL = "Start camera";
    var STOP_LABEL = "Stop camera";
    /* Ms between unpainted-picture looks; looks before giving up. */
    var AGAIN = 250;
    var LOOKS = 40;
    /* Ms after connect/camera change to log counts: tells black-sent from undecodable. */
    var SAMPLES = [4000, 15000, 45000];
    /* Max remote zoom past fit: upright fills taller screen, sideways loses only some bars. */
    var ZOOM_MOST = 1.35;
    /* Ms between paint checks; max put-backs per picture per call. FIRST_LOOK until first
     * frame (screen black till then). */
    var PAINT_LOOK = 2000;
    var FIRST_LOOK = 500;
    var PUT_BACK_MOST = 3;
    /* Own picture with remote drawn: top-right under call screen's top line, 4% from right,
     * 10% from top (as reference clients). */
    var CORNER = {
        top: "10vh", right: "4vw", bottom: "auto", left: "auto",
        width: "27vw", height: "36vw", borderRadius: "2vw"
    };
    /* Full screen while no remote picture. */
    var WHOLE = {
        top: "0", right: "0", bottom: "0", left: "0",
        width: "100%", height: "100%", borderRadius: "0"
    };

    var media = navigator.mediaDevices;
    var ask = media?.getUserMedia ? media.getUserMedia.bind(media) : null;

    /* Host post(path, body), once bridge attaches. */
    var send = noop;
    /* Camera wanted at start (page query). */
    var wanted = !window.location.search.includes("noOutgoingVideoInitially")
        && !window.location.search.includes("disableVideoCompletely");
    /* Facing: "user" or "environment". */
    var facing = "user";
    /* Facing of camera behind picture (sets mirroring). Old camera's until new track in place,
     * else last frame flips early. */
    var onScreen = "user";
    /* Last camera failure, engine's message. */
    var failed = false;
    var failure = "";
    /* Stream given to page. */
    var stream = null;
    /* Blank stand-in track. */
    var blank = null;
    /* Camera track while open; camera on its way. */
    var camera = null;
    var opening = false;
    /* Open camera's device; kept on let-go (turn = other device). */
    var device = "";
    /* Cameras, kept once named (turn asks by name); listing costs a round trip, so only
     * repeated before first open. */
    var listed = null;
    /* Back camera index: engine names all alike, host picks main lens from platform labels.
     * `chosen`: camera opened, for log. */
    var backCamera = 0;
    var chosen = "";
    /* Turn start, for logging turn duration. */
    var turned = 0;
    /* Every peer connection page made. */
    var connections = [];
    /* Page's mic stream (`stream` if camera asked too), and mic request. */
    var voice = null;
    var voiceAsked = true;
    /* Mic shut while call sound moves: open mic crashes sound server (CallAudio.qml). Page
     * asks wait; stopped tracks kept until reopen. */
    var micShut = false;
    var micWaits = [];
    var micStopped = [];
    /* Mic reopen retries: count, 1 s apart. */
    var MIC_TRIES = 3;
    /* Other end's stream and last camera report. */
    var remote = null;
    var remoteEnabled = null;
    var lastReport = "";
    /* Remote picture watched for size change. */
    var fitting = null;
    var lookingAgain = false;
    var looks = 0;
    var layoutDue = false;
    /* Connected; current sample round. */
    var connected = false;
    var round = 0;
    /* App backgrounded: nothing drawn. */
    var away = false;
    /* Per picture: paint count at last look, put-backs, next look time and round. */
    var paints = { ours: null, theirs: null };
    var putBacks = { ours: 0, theirs: 0 };
    var paintAt = 0;
    var paintRound = 0;
    /* Per picture: painted since arrival (not black), or given up. */
    var firsts = { ours: null, theirs: null };
    var shows = { ours: false, theirs: false };

    function noop() {}

    /* Phone-only facts for app log. */
    function note(text) {
        send("/log", text);
    }

    /* Track description, for log. */
    function described(track) {
        var settings = track.getSettings ? track.getSettings() : {};
        return (track.label || "camera") + " " + (settings.width || "?") + "x"
            + (settings.height || "?") + "@" + (settings.frameRate || "?")
            + (settings.facingMode ? " " + settings.facingMode : "");
    }

    function labelled(label) {
        return document.querySelector("[aria-label=\"" + label + "\"]");
    }

    /* Press page switch until in wanted state (absent without video track). */
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

    /* Frameless canvas video track; null if engine can't. */
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

    /* Camera name says it faces `which`. */
    var NAMED = {
        user: /front|user|selfie/i,
        environment: /back|rear|environment|main/i
    };

    /* Device id facing `which`, or "" for engine facingMode pick. Id since Gecko may ignore
     * facingMode. Turn without names: any other camera. */
    function cameraFor(which, avoid) {
        if (!media.enumerateDevices) {
            return Promise.resolve("");
        }
        return cameraList().then(function (cameras) {
            chosen = "";
            if (cameras.length < 2) {
                return "";
            }
            var named = cameras.filter(function (one) {
                return NAMED[which].test(one.label || "");
            });
            if (named.length > 0) {
                var place = which === "environment" ? Math.min(backCamera, named.length - 1) : 0;
                if (named.length > 1) {
                    chosen = " (" + (place + 1) + " of " + named.length + ")";
                }
                return named[place].deviceId;
            }
            var other = cameras.filter(function (one) {
                return one.deviceId !== avoid;
            });
            return avoid && other.length > 0 ? other[0].deviceId : "";
        }, function () {
            return "";
        });
    }

    /* Listed cameras, kept once named. */
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

    /* Open camera facing `which`; `avoid` = one open before turn. */
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
            /* Camera's own facing wins: mirror and app follow. */
            if (settings.facingMode === "user" || settings.facingMode === "environment") {
                facing = settings.facingMode;
            }
            onScreen = facing;
            note("camera open: " + described(track) + chosen
                + (turned ? ", " + (Date.now() - turned) + " ms after the turn" : ""));
            track.addEventListener("ended", function () {
                if (track === camera) {
                    note("camera ended by the engine: " + described(track));
                }
            });
            return track;
        });
    }

    /* Hold sender to caps; nothing before negotiation, refusal keeps engine value: no fail. */
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
            /* Kept as is. */
        }
    }

    /* Video senders, track or not. */
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

    /* Run once mic not shut: now, or when it opens. */
    function micFree() {
        if (!micShut) {
            return Promise.resolve();
        }
        return new Promise(function (resolve) {
            micWaits.push(resolve);
        });
    }

    /* Shut mic: stop tracks, off senders (device released); kept on stream for switch. */
    function micOff() {
        micShut = true;
        if (!voice) {
            return;
        }
        voice.getAudioTracks().forEach(function (track) {
            if (track.readyState === "ended") {
                return;
            }
            var senders = [];
            connections.forEach(function (connection) {
                connection.getSenders().forEach(function (sender) {
                    if (sender.track === track) {
                        senders.push(sender);
                        var done = sender.replaceTrack(null);
                        if (done?.catch) {
                            done.catch(noop);
                        }
                    }
                });
            });
            track.stop();
            micStopped.push({ track: track, senders: senders });
            note("microphone: stopped while the sound moves");
        });
    }

    /* Reopen mic: asks resume; new track replaces stopped, on/off as page had. */
    function micOn() {
        micShut = false;
        var waits = micWaits;
        micWaits = [];
        waits.forEach(function (resolve) {
            resolve();
        });
        var stopped = micStopped;
        micStopped = [];
        if (stopped.length > 0) {
            reopen(stopped, MIC_TRIES);
        }
    }

    function reopen(stopped, tries) {
        ask({ audio: voiceAsked }).then(function (given) {
            var track = given.getAudioTracks()[0];
            /* Shut again while opening: this one goes too. */
            if (micShut || !track) {
                given.getTracks().forEach(function (one) {
                    one.stop();
                });
                micStopped = stopped.concat(micStopped);
                return;
            }
            track.enabled = stopped.every(function (one) {
                return one.track.enabled;
            });
            stopped.forEach(function (one) {
                voice.removeTrack(one.track);
            });
            voice.addTrack(track);
            stopped.forEach(function (one) {
                one.senders.forEach(function (sender) {
                    var done = sender.replaceTrack(track);
                    if (done?.catch) {
                        done.catch(noop);
                    }
                });
            });
            note("microphone: open again");
        }, function (err) {
            note("microphone: would not open again: " + describe(err));
            if (micShut) {
                micStopped = stopped.concat(micStopped);
            } else if (tries > 1) {
                window.setTimeout(function () {
                    if (micShut) {
                        micStopped = stopped.concat(micStopped);
                    } else {
                        reopen(stopped, tries - 1);
                    }
                }, 1000);
            }
        });
    }

    /* Own picture element (page stream). */
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

    /* Set `track` on page stream and every video sender. */
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
        /* Picture plays stream as when given: give again to show new track. */
        firsts.ours = null;
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

    /* Place own picture: mirrored if reader-facing; full, corner, or hidden without camera.
     * Stream not on yet: retry. */
    function layout() {
        var picture = preview();
        if (!picture) {
            lookAgain("the page never drew it");
            return;
        }
        picture.style.transform = onScreen === "user" ? "scaleX(-1)" : "none";
        var box = picture.parentElement;
        if (!box) {
            return;
        }
        if (!camera) {
            box.style.display = "none";
            return;
        }
        box.style.display = "block";
        /* Own picture from start (page waits for call up). */
        if (box.parentElement) {
            box.parentElement.style.setProperty("display", "block", "important");
        }
        var whole = !remoteShown();
        place(box, whole ? WHOLE : CORNER);
        box.style.boxShadow = whole ? "none" : "";
        start(picture);
    }

    /* Remote picture element. */
    function theirs() {
        for (var video of document.getElementsByTagName("video")) {
            if (video.srcObject && video.srcObject !== stream) {
                return video;
            }
        }
        return null;
    }

    /* Fill screen with remote up to ZOOM_MOST, overflow cut. Laid out, not scaled (extra
     * layer; engine seen leaving pictures unpainted). Redo on resize. */
    function fitTheirs() {
        var picture = theirs();
        var box = picture?.parentElement;
        if (!box) {
            return;
        }
        if (picture !== fitting && picture.addEventListener) {
            fitting = picture;
            picture.addEventListener("resize", fitTheirs);
        }
        var width = picture.videoWidth;
        var height = picture.videoHeight;
        var boxWidth = box.clientWidth;
        var boxHeight = box.clientHeight;
        var zoom = 1;
        if (width > 0 && height > 0 && boxWidth > 0 && boxHeight > 0) {
            var fit = Math.min(boxWidth / width, boxHeight / height);
            var fill = Math.max(boxWidth / width, boxHeight / height);
            zoom = Math.min(fill / fit, ZOOM_MOST);
        }
        if (zoom < 1.001) {
            zoom = 1;
        }
        var size = (zoom * 100).toFixed(1) + "%";
        var shift = ((1 - zoom) * 50).toFixed(1) + "%";
        place(picture, { position: "relative", width: size, height: size, left: shift, top: shift });
        box.style.overflow = "hidden";
    }

    /* Page hides picture (display: none on it or ancestor). */
    function hidden(picture) {
        for (var at = picture; at; at = at.parentElement) {
            if (at.style?.display === "none") {
                return true;
            }
        }
        return false;
    }

    /* Frames painted (not just decoded), or null. */
    function paintedOf(picture) {
        var count = picture.mozPaintedFrames;
        return typeof count === "number" ? count : null;
    }

    /* Meant to be seen: present, not hidden, page in front. */
    function meant(picture) {
        return Boolean(picture) && !hidden(picture) && !away
            && document.visibilityState !== "hidden";
    }

    /* Ask picture to play if paused. */
    function play(picture) {
        if (!picture.paused || !picture.play) {
            return;
        }
        var playing = picture.play();
        if (playing?.catch) {
            playing.catch(noop);
        }
    }

    /* Schedule paint look while meant to be seen: FIRST_LOOK until first shown. Sooner look
     * replaces due one. */
    function paintSoon() {
        var ours = preview();
        var other = theirs();
        if (!(meant(ours) || meant(other))) {
            return;
        }
        var oursShown = firstSeen("ours", ours);
        var otherShown = firstSeen("theirs", other);
        var wait = oursShown && otherShown ? PAINT_LOOK : FIRST_LOOK;
        var at = Date.now() + wait;
        if (paintAt && paintAt <= at) {
            return;
        }
        paintAt = at;
        paintRound += 1;
        var round = paintRound;
        window.setTimeout(function () {
            if (round === paintRound) {
                lookAtPaint(wait);
            }
        }, wait);
    }

    /* True if shown or not meant to be; newly seen: paint count noted. */
    function firstSeen(which, picture) {
        if (!meant(picture)) {
            return true;
        }
        if (firsts[which]?.picture !== picture) {
            firsts[which] = { picture: picture, count: paintedOf(picture) };
            shows[which] = false;
        }
        return shows[which];
    }

    function lookAtPaint(wait) {
        paintAt = 0;
        var before = shows.ours + " " + shows.theirs;
        var ours = lookedAt("ours", preview(), wait);
        var other = lookedAt("theirs", theirs(), wait);
        if (shows.ours + " " + shows.theirs !== before) {
            report();
        } else if (ours || other) {
            paintSoon();
        }
    }

    /* Seen, playing, with frames, nothing painted since last look: put back (once if engine
     * not counting). True while worth looking again. */
    function lookedAt(which, picture, wait) {
        if (picture && putBacks[which] > 0 && picture.paused) {
            /* Put back, not playing: it carries other end's voice too. */
            play(picture);
        }
        if (!meant(picture)) {
            paints[which] = null;
            firsts[which] = null;
            shows[which] = false;
            return false;
        }
        firstSeen(which, picture);
        /* Paused, no frame, or camera coming: clock starts once there is. Slow turn waited
         * for, not put back or given up. */
        var last = paints[which];
        paints[which] = null;
        if (picture.paused || !picture.videoWidth || (which === "ours" && opening)) {
            return true;
        }
        var count = paintedOf(picture);
        showing(which, count);
        if (last?.picture !== picture || count !== last.count) {
            paints[which] = { picture: picture, count: count };
            return true;
        }
        var most = count === null ? 1 : PUT_BACK_MOST;
        if (putBacks[which] < most) {
            putBacks[which] += 1;
            putBack(which, picture, count, wait);
            return true;
        }
        if (putBacks[which] === most && count !== null) {
            putBacks[which] += 1;
            note((which === "ours" ? "our" : "their") + " picture: still not painted");
        }
        /* Given up: not waited for. */
        shows[which] = true;
        return true;
    }

    /* Shown once painted since arrival (at once if uncounted); count down = restarted. */
    function showing(which, count) {
        var first = firsts[which];
        if (count !== null && count < first.count) {
            first.count = count;
        }
        if (count === null || count > first.count) {
            shows[which] = true;
        }
    }

    /* Put back: track off and on, stream given again (as a turn; seen to fix blank picture). */
    function putBack(which, picture, count, wait) {
        var source = picture.srcObject;
        var tracks = source.getVideoTracks();
        tracks.forEach(function (track) {
            source.removeTrack(track);
        });
        tracks.forEach(function (track) {
            source.addTrack(track);
        });
        picture.srcObject = source;
        play(picture);
        note((which === "ours" ? "our" : "their") + " picture put back: "
            + (count === null ? "the engine does not say what it painted"
                : "nothing painted in " + wait / 1000 + " s"));
    }

    /* Look at own picture soon. One at a time: parallel would spend LOOKS before slow camera.
     * Once spent, `why` logged. */
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

    /* Own picture autoplays muted, but given stream while page hidden it sat paused: asked to
     * play and looked at until it does, as page does with remote. */
    function start(picture) {
        if (!picture.paused || !picture.play) {
            return;
        }
        play(picture);
        lookAgain("it never played");
    }

    /* Report picture state on change. */
    function report() {
        layout();
        fitTheirs();
        paintSoon();
        var state = {
            local: camera !== null,
            remote: remoteShown(),
            localPainted: shows.ours,
            remotePainted: shows.theirs,
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
        note("reported: " + now);
        send("/video", now);
    }

    /* Engine refusal: name (the reason) and message. */
    function describe(err) {
        if (!err) {
            return "";
        }
        var name = err.name && err.name !== "Error" ? err.name : "";
        var message = err.message || "";
        return name && message ? name + ": " + message : name || message;
    }

    /* Camera failed: page told off, app told why. */
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

    /* Open camera, put in place; `otherwise` on failure. Switched off meanwhile: let go. */
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
        /* Retry after failure: second failure is news to app. */
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

    /* Turn: let go, open other; on failure reopen previous. */
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
        /* Black/last frame until other camera's first: spinner meanwhile. */
        firsts.ours = null;
        report();
        open(function (err) {
            note("camera would not turn: " + describe(err));
            facing = before;
            open(giveUp);
        }, was);
    }

    /* Log turned camera's first picture; rest is startup (`camera open`). */
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

    /* Video sections of SDP for log: direction, codecs. */
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

    /* Track a sender sends, for log. */
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

    /* One way's RTP counts: codec, size, frames, bytes each (black ~dozens, picture thousands),
     * keyframe requests (undecodable receiver keeps asking). */
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

    /* Picture draw state for log: hidden, playing/paused, size, frames on screen. */
    function drawn(picture) {
        if (!picture) {
            return "not drawn";
        }
        var count = paintedOf(picture);
        return (hidden(picture) ? "hidden " : "") + (picture.paused ? "paused " : "playing ")
            + (picture.videoWidth || 0) + "x" + (picture.videoHeight || 0)
            + " painted " + (count === null ? "?" : count);
    }

    /* Log one connection's video send/recv now. */
    function sample(connection) {
        if (!connection.getStats) {
            return;
        }
        var senders = videoSenders(connection).map(sending).join(", ") || "no sender";
        var shown = drawn(preview()) + "; theirs " + drawn(theirs());
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

    /* Log send/recv a few times from now; only once connected. */
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
        /* Negotiated: encodings to hold to caps. */
        connection.addEventListener("signalingstatechange", function () {
            note("signalling: " + connection.signalingState);
            if (connection.signalingState === "stable") {
                note("video, ours: " + videoSections(connection.localDescription)
                    + " | theirs: " + videoSections(connection.remoteDescription));
                videoSenders(connection).forEach(cap);
            }
        });
        /* Remote camera report on page's channel. */
        var create = connection.createDataChannel;
        connection.createDataChannel = function (label, options) {
            var channel = create.call(connection, label, options);
            if (label === "mutedState") {
                channel.addEventListener("message", function (event) {
                    try {
                        var said = JSON.parse(event.data);
                        /* Only what page accepts: both flags. */
                        if (said && typeof said.videoEnabled === "boolean"
                            && typeof said.audioEnabled === "boolean") {
                            note("their camera: " + (said.videoEnabled ? "on" : "off")
                                + ", they say");
                            remoteEnabled = said.videoEnabled;
                            report();
                        }
                    } catch (err) {
                        /* Page says what's wrong. */
                    }
                });
            }
            return channel;
        };
    }

    /* One host command; false if not ours. */
    function take(command) {
        var lens = /^back-camera-(\d+)$/.exec(command);
        if (lens) {
            backCamera = Number(lens[1]);
            return true;
        }
        switch (command) {
        case "camera-on":
            cameraOn();
            return true;
        case "camera-off":
            cameraOff();
            return true;
        case "microphone-off":
            micOff();
            return true;
        case "microphone-on":
            micOn();
            return true;
        case "facing-user":
            face("user");
            return true;
        case "facing-environment":
            face("environment");
            return true;
        case "hidden":
            /* Background: pictures not drawn; voice plays. */
            away = true;
            document.documentElement.classList.add("piirit-hidden");
            return true;
        case "shown":
            away = false;
            document.documentElement.classList.remove("piirit-hidden");
            paintSoon();
            return true;
        default:
            return false;
        }
    }

    /* Page asks camera+mic once. Camera only if wanted, else blank track (switch drawn, call
     * can become video); failed camera doesn't stop call. Mic shut: waits for reopen. */
    function askFor(asked) {
        if (!asked?.video) {
            return ask(asked).then(function (given) {
                if (asked?.audio) {
                    voice = given;
                    voiceAsked = asked.audio;
                }
                return given;
            });
        }
        voiceAsked = asked.audio === undefined ? true : asked.audio;
        return ask({ audio: voiceAsked }).then(function (given) {
            stream = given;
            voice = given;
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
    }

    if (ask) {
        media.getUserMedia = function (asked) {
            return micFree().then(function () {
                return askFor(asked);
            });
        };
    }

    /* Page sets stream on picture: remote fitted, both watched for first frame. */
    var Media = window.HTMLMediaElement;
    var source = Media && Object.getOwnPropertyDescriptor
        ? Object.getOwnPropertyDescriptor(Media.prototype, "srcObject") : null;
    if (source?.set && source.configurable) {
        Object.defineProperty(Media.prototype, "srcObject", {
            configurable: true,
            enumerable: source.enumerable,
            get: source.get,
            set: function (value) {
                var before = source.get.call(this);
                source.set.call(this, value);
                if (value && value !== stream && value !== before) {
                    firsts.theirs = null;
                }
                if (value) {
                    window.setTimeout(report, 0);
                }
            }
        });
    }

    /* Only page pictures: app draws rest. */
    var sheet = document.createElement("style");
    sheet.textContent = "body * { visibility: hidden !important; }\n"
        + "video { visibility: visible !important; }\n"
        + "html.piirit-hidden video { visibility: hidden !important; }\n";
    (document.head || document.documentElement).appendChild(sheet);
    /* Rotated: new screen shape. */
    if (window.addEventListener) {
        window.addEventListener("resize", fitTheirs);
    }

    /* Page console to call log: warnings, errors, remote picture/camera and command lines
     * (rest carry addresses); repeats once; uncaught failures too. */
    var PAGE_SAYS = [
        "incStream.videoTracks() muted states:",
        "incoming media ",
        "Sending our muted state ",
        "Want to report muted state ",
        "mutedStateDataChannel ",
        "URL hash CMD",
        "unexpected URL hash",
        "Accept call prompt "
    ];
    var pageSaid = "";

    function heard(level, words) {
        var text = Array.prototype.map.call(words, function (one) {
            if (typeof one === "string") {
                return one;
            }
            try {
                return JSON.stringify(one);
            } catch (err) {
                return String(one);
            }
        }).join(" ");
        if (level === "log" && !PAGE_SAYS.some(function (start) {
            return text.startsWith(start);
        })) {
            return;
        }
        var line = "page" + (level === "log" ? "" : " " + level) + ": " + text;
        if (line !== pageSaid) {
            pageSaid = line;
            note(line);
        }
    }

    var pageConsole = window.console;
    if (pageConsole) {
        ["log", "warn", "error"].forEach(function (level) {
            var own = pageConsole[level];
            if (typeof own === "function") {
                pageConsole[level] = function () {
                    heard(level, arguments);
                    return own.apply(pageConsole, arguments);
                };
            }
        });
    }
    /* Log visibility and button presses: hidden page paints nothing; stray press = mic off
     * nobody chose. */
    if (document.addEventListener) {
        document.addEventListener("visibilitychange", function () {
            note("visibility: " + document.visibilityState);
            paintSoon();
        });
        document.addEventListener("click", function (event) {
            for (var at = event.target; at; at = at.parentElement) {
                var label = at.getAttribute ? at.getAttribute("aria-label") : null;
                if (label) {
                    note("pressed: " + label + (event.isTrusted ? ", by a touch" : ", by the app"));
                    return;
                }
            }
        }, true);
    }
    if (window.addEventListener) {
        window.addEventListener("error", function (event) {
            heard("error", [event.message || "an error"]);
        });
        window.addEventListener("unhandledrejection", function (event) {
            heard("error", [describe(event.reason) || "a promise failed"]);
        });
    }

    return {
        /* Bridge's post(path, body). */
        attach: function (post) {
            send = post;
            if (typeof document.visibilityState === "string") {
                note("visibility: " + document.visibilityState);
            }
        },
        watch: watch,
        take: take
    };
}());
