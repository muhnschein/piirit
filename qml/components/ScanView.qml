import QtQuick 2.0
import QtMultimedia 5.6
import Sailfish.Silica 1.0
import Piirit 1.0

/*
 * Point the camera at a QR code -- or, from the button under the
 * viewfinder, type or paste the link the code would carry. What either
 * says is handed out on `scanned` and the view waits; what to do with it
 * is the host's -- an invite is followed -- and the core is asked what
 * it is first, the same as for a pasted link.
 *
 * A component of its own, loaded by URL by the pages that want it -- the
 * QR page for an invite, the take-over page for the code a device shows
 * while offering its profile: this is the only file that names a Camera,
 * so a device without one costs the scanner rather than the page around
 * it. Both give it the rest of the page, which is what a code needs to
 * land in enough pixels to read.
 *
 * What it is looking for is the host's to say (`hintText`), and so is
 * whether the link behind the code can be typed instead (`offerLink`).
 *
 * Frames come through `grabToImage`, which is the one way QML on Qt 5.6
 * hands a viewfinder's pixels to anything, and go to the scanner as a
 * file. The next frame is grabbed only once the last has been decoded, so
 * a slow decode never queues frames behind it.
 *
 * Once a code is read the view stays, with the camera off and a busy
 * indicator on, until the host takes it down: Silica drops any stack
 * operation asked for while a transition is running, so a page that
 * popped itself here would have the host's own navigation -- opening the
 * chat the invite led to -- land during the pop and be lost. The typed
 * link is a panel over the viewfinder rather than a dialog for the same
 * reason: a dialog's own pop would be the transition the host's
 * navigation lands in.
 *
 * Focus is asked for three ways, because a camera left to itself gave a
 * viewfinder too soft for any code to register: continuous autofocus in
 * the video pipeline, where the platform's cameras run it; a focus search
 * every couple of seconds while nothing has been read, for a camera that
 * only focuses when told; and a tap on the viewfinder, which focuses on
 * the point under the finger.
 */
Item {
    id: root

    /// The view is the one on screen. The camera runs only then.
    property bool active: false

    /// A code was read, or a link typed, and this is its text.
    signal scanned(string text)
    /// The scanner could not read a frame at all. The message is its own.
    signal failed(string message)

    /// Set once a code is read, so a second frame in flight cannot
    /// report the same code twice.
    property bool done: false
    /// The link panel is open. The camera keeps running behind it: a
    /// reader who opened the panel and then found the code can still
    /// hold the phone up to it.
    property bool typing: false

    /// How far below the top of the view the line sits, for a host that
    /// has its own words up there already.
    property real hintTopMargin: 0

    /// The line saying what to point the camera at. The
    /// host's words, because what a code means is the host's: an invite
    /// on the QR page, the other phone's offer of a profile on the
    /// take-over page.
    property string hintText: qsTr("Point the camera at someone's invite code")

    /// Whether what the code carries can be typed or pasted instead.
    /// Worth having wherever the reader can get at the text another way,
    /// which is every camera in this app: a camera that will not read is
    /// otherwise a dead end.
    property bool offerLink: true

    /// What the typed half is called, since what the code carries
    /// differs by host: an invite someone sent, or the string a device
    /// shows beside the code it is offering its profile behind.
    property string linkButtonText: qsTr("Enter invite link")
    property string linkLabel: qsTr("Invite link")
    property string linkPlaceholder: "https://i.delta.chat/..."
    property string linkActionText: qsTr("Connect")

    /// The starts of the things this host's code carries, lower case.
    /// Only used to decide whether the clipboard is worth pasting into
    /// the field; what the payload means is the core's call.
    property var linkPrefixes: [
        "https://i.delta.chat/",
        "openpgp4fpr:",
        "dcaccount:",
        "dclogin:"
    ]

    /// What was read or typed, once it is worth acting on.
    function found(text) {
        if (root.done) {
            return
        }
        root.done = true
        camera.stop()
        root.scanned(text)
    }

    /// Open the link panel, with the clipboard's contents in it when
    /// they look like an invite: pasting is what the panel is for, and a
    /// link copied a moment ago is the likely reason it was opened.
    function typeLink() {
        root.typing = true
        var clip = Clipboard.text
        if (linkField.text.length === 0 && clip.length > 0 && root.looksLikeInvite(clip)) {
            linkField.text = clip
        }
    }

    /// Whether some text is one of the things this host's code carries,
    /// so the clipboard is not pasted into the field when it holds a
    /// shopping list.
    function looksLikeInvite(text) {
        var trimmed = text.trim().toLowerCase()
        for (var i = 0; i < root.linkPrefixes.length; i++) {
            if (trimmed.indexOf(root.linkPrefixes[i].toLowerCase()) === 0) {
                return true
            }
        }
        return false
    }

    function useLink() {
        var text = linkField.text.trim()
        if (text.length === 0) {
            return
        }
        root.found(text)
    }

    QrScanner {
        id: scanner
        objectName: "scanner"
        onFound: root.found(text)
        onError: root.failed(message)
    }

    Camera {
        id: camera
        objectName: "camera"
        // The video pipeline is where continuous autofocus runs.
        captureMode: Camera.CaptureVideo
        focus {
            focusMode: Camera.FocusContinuous
            focusPointMode: Camera.FocusPointAuto
        }
        // The resolution is picked as soon as the camera can say what it
        // has, which is before it goes active.
        onCameraStatusChanged: root.chooseViewfinder()
    }

    /// The long side this view wants from the camera and from a grab.
    ///
    /// A decoder reads a symbol out of the pixels each module lands in,
    /// and wants three or four of them across a module to tell one from
    /// its neighbour. The code a device shows while offering a profile
    /// carries an address and a one-time secret, so it is a dense symbol
    /// -- around seventy modules across with its quiet zone. At 640 a
    /// code filling half the frame put under three pixels in a module
    /// and would not read at all; the room here is what makes that
    /// closer to six.
    readonly property int wantedLongSide: 1280

    /// Set once the camera has been asked for a resolution, so a status
    /// change later does not ask again.
    property bool viewfinderChosen: false

    /// Ask the camera for the smallest viewfinder that is still big
    /// enough, or the biggest it has when none of them are.
    ///
    /// Asked for rather than assumed: the platform's own default is
    /// whatever is cheapest to run, and no grab can put back detail the
    /// camera never captured. Guarded all the way down, because a
    /// camera that cannot answer should leave the view working on
    /// whatever it does give.
    function chooseViewfinder() {
        if (root.viewfinderChosen
                || typeof camera.supportedViewfinderResolutions !== "function") {
            return
        }
        var offered = camera.supportedViewfinderResolutions()
        if (!offered || offered.length === 0) {
            return
        }
        var enough = null
        var largest = null
        for (var i = 0; i < offered.length; i++) {
            var size = offered[i]
            var longest = Math.max(size.width, size.height)
            if (longest <= 0) {
                continue
            }
            if (!largest || longest > Math.max(largest.width, largest.height)) {
                largest = size
            }
            if (longest >= root.wantedLongSide
                    && (!enough
                        || longest < Math.max(enough.width, enough.height))) {
                enough = size
            }
        }
        var pick = enough || largest
        if (!pick) {
            return
        }
        camera.viewfinder.resolution = Qt.size(pick.width, pick.height)
        root.viewfinderChosen = true
    }

    // An autofocus run, for a camera that does not run one on its own.
    // Unlocked first: a search that finds focus locks it, and a locked
    // focus is a fixed one.
    function refocus() {
        camera.unlock()
        camera.searchAndLock()
    }

    // Every couple of seconds while nothing has been read.
    Timer {
        id: refocusTimer
        objectName: "refocus"
        interval: 2500
        repeat: true
        triggeredOnStart: true
        running: root.active && !root.done
        onTriggered: root.refocus()
    }

    // The camera runs only while this view is the one on screen.
    //
    // The resolution is asked for here as well as on a status change: a
    // camera that is already loaded has nothing left to change, and one
    // that is not answers with nothing until it is. Whichever comes
    // first settles it.
    onActiveChanged: {
        if (root.active && !root.done) {
            root.chooseViewfinder()
            camera.start()
        } else {
            camera.stop()
        }
    }

    /// The long side of a grabbed frame: the same as the viewfinder's,
    /// since grabbing smaller throws away the detail just asked for.
    ///
    /// It costs decode time, which the grabber already handles: the next
    /// frame is taken only once the last has been read, so a slower
    /// decode scans a little less often rather than queueing up.
    readonly property int grabLongSide: root.wantedLongSide

    /// That long side, in the viewfinder's own shape. A grab to a fixed
    /// square stretches the modules by whatever the viewfinder is not
    /// square by, and a decoder reading a square symbol as an oblong one
    /// has that much less to work with.
    function grabSize() {
        var longest = Math.max(viewfinder.width, viewfinder.height)
        if (longest <= 0) {
            return Qt.size(root.grabLongSide, root.grabLongSide)
        }
        // Never upscale: a small viewfinder has no more detail to give.
        var ratio = Math.min(1, root.grabLongSide / longest)
        return Qt.size(Math.max(1, Math.round(viewfinder.width * ratio)),
                       Math.max(1, Math.round(viewfinder.height * ratio)))
    }

    Timer {
        id: grabber
        objectName: "grabber"
        interval: 300
        repeat: true
        running: root.active && !scanner.busy && !root.done
        onTriggered: {
            var path = scanner.frame_path()
            if (path.length === 0) {
                return
            }
            viewfinder.grabToImage(function(result) {
                if (result.saveToFile(path)) {
                    root.framesTried += 1
                    scanner.decode(path)
                }
            }, root.grabSize())
        }
    }

    VideoOutput {
        id: viewfinder
        objectName: "viewfinder"
        anchors.fill: parent
        source: camera
        // Filled rather than fitted. Fitted leaves bars down the sides
        // of a tall view, and those bars are grabbed and decoded along
        // with the picture -- pixels the code does not get. It also
        // means what the reader sees is what is being read.
        fillMode: VideoOutput.PreserveAspectCrop

        // Tap to focus on what is under the finger.
        MouseArea {
            objectName: "focusTap"
            anchors.fill: parent
            onClicked: {
                camera.focus.focusPointMode = Camera.FocusPointCustom
                camera.focus.customFocusPoint = Qt.point(mouse.x / width, mouse.y / height)
                root.refocus()
            }
        }
    }

    // The code was read and the host is acting on it.
    BusyIndicator {
        objectName: "acting"
        anchors.centerIn: parent
        running: root.done
        size: BusyIndicatorSize.Large
    }

    /// How many frames have been read and found nothing. Shown as a
    /// spinner rather than a number: what the reader needs to know is
    /// that the app is looking, not how hard.
    property int framesTried: 0

    // Beside the line, inside the page margin rather than anchored off
    // the end of it: anchored to the line's own left edge, it hung half
    // off the side of the screen.
    BusyIndicator {
        id: looking
        objectName: "looking"
        anchors {
            left: parent.left
            leftMargin: Theme.horizontalPageMargin
            verticalCenter: hint.verticalCenter
        }
        running: root.active && !root.done && root.framesTried > 0
        size: BusyIndicatorSize.Small
    }

    // The other way in, where a thumb finds it: a button under the
    // viewfinder that opens the panel for the link the code would carry,
    // typed or pasted. Over the viewfinder rather than instead of it.
    Button {
        id: typeLinkButton
        objectName: "typeLinkButton"
        visible: root.offerLink && !root.typing && !root.done
        anchors {
            horizontalCenter: parent.horizontalCenter
            bottom: parent.bottom
            bottomMargin: Theme.paddingLarge
        }
        text: root.linkButtonText
        onClicked: root.typeLink()
    }

    Column {
        id: linkPanel
        objectName: "linkPanel"
        visible: root.offerLink && root.typing && !root.done
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
            bottomMargin: Theme.paddingLarge
        }
        spacing: Theme.paddingSmall

        Rectangle {
            width: parent.width
            height: linkField.height + followButton.height + 2 * Theme.paddingMedium
            color: Theme.rgba(Theme.highlightDimmerColor, 0.8)

            TextField {
                id: linkField
                objectName: "linkField"
                anchors {
                    left: parent.left
                    right: parent.right
                    top: parent.top
                    topMargin: Theme.paddingMedium
                }
                label: root.linkLabel
                placeholderText: root.linkPlaceholder
                inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText
            }

            Button {
                id: followButton
                objectName: "followButton"
                anchors {
                    horizontalCenter: parent.horizontalCenter
                    top: linkField.bottom
                }
                text: root.linkActionText
                enabled: linkField.text.trim().length > 0
                onClicked: root.useLink()
            }
        }
    }

    // At the top rather than the foot: the phone is held up, the code
    // is in the middle of the picture, and the words about it belong
    // where the eye already is rather than down by the hand.
    Label {
        id: hint
        objectName: "hint"
        anchors {
            left: looking.right
            leftMargin: Theme.paddingMedium
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
            top: parent.top
            topMargin: root.hintTopMargin + Theme.paddingMedium
        }
        wrapMode: Text.Wrap
        color: Theme.secondaryHighlightColor
        visible: !root.done
        text: root.typing
              ? qsTr("Or point the camera at the code")
              : root.hintText
    }
}
