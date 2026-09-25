import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * The phone's own way to take a call that rings: the handset, dragged
 * off to either side, answers it; dragged up, it silences the ringing
 * and brings up what else can be done with the call (the page's).
 *
 * Chevrons either side of the handset, and one under it, say which way,
 * drifting the way they point. The one under goes once the ringing is
 * silenced: up has been used.
 *
 * A drag goes the way it first went far enough to tell -- across or up
 * -- and stays that way, so a thumb that wanders does not answer a call
 * it was silencing. Let go short of the mark, the handset goes back.
 */
Item {
    id: root

    /// The ringing has been silenced: up is spent.
    property bool silenced: false

    /// Dragged far enough to either side.
    signal answered()

    /// Dragged far enough up.
    signal silenceRequested()

    /// How far to either side the handset goes to answer.
    readonly property real reach: Math.min(root.width / 3, 2 * Theme.itemSizeLarge)
    /// How far up it goes to silence.
    readonly property real lift: Theme.itemSizeLarge

    /// Where the handset is, from where it rests.
    property real dx: 0
    property real dy: 0
    /// Which way the drag under way goes: "" until it has gone far enough
    /// to tell, then "x" or "y".
    property string axis: ""

    /// From 0 to 1 and round again: how far the chevrons have drifted.
    property real drift: 0

    width: parent ? parent.width : 0
    height: 3 * Theme.itemSizeLarge

    /// A drag let go of at `dx`, `dy` from where it started: what it does.
    /// "answered", "silenced", or "back" for neither.
    function release(dx, dy) {
        if (Math.abs(dx) >= root.reach) {
            root.answered()
            return "answered"
        }
        if (!root.silenced && -dy >= root.lift) {
            root.silenceRequested()
            return "silenced"
        }
        return "back"
    }

    // Back to rest, at the pace a let-go thing falls back.
    Behavior on dx {
        enabled: root.axis === ""
        NumberAnimation { duration: 200; easing.type: Easing.OutQuad }
    }
    Behavior on dy {
        enabled: root.axis === ""
        NumberAnimation { duration: 200; easing.type: Easing.OutQuad }
    }

    NumberAnimation on drift {
        from: 0
        to: 1
        duration: 1200
        loops: Animation.Infinite
        running: root.visible && Qt.application.state === Qt.ApplicationActive
    }

    // The theme's large handset, and its ordinary one where it has no
    // large one.
    Image {
        id: handset
        objectName: "handset"
        anchors {
            centerIn: parent
            horizontalCenterOffset: root.dx
            verticalCenterOffset: root.dy
        }
        width: Theme.iconSizeLarge
        height: Theme.iconSizeLarge
        sourceSize.width: Theme.iconSizeLarge
        sourceSize.height: Theme.iconSizeLarge
        source: "image://theme/icon-l-answer"
        onStatusChanged: {
            if (handset.status === Image.Error) {
                handset.source = "image://theme/icon-m-call"
            }
        }
    }

    // Green, either side: answering is either way.
    Image {
        objectName: "answerLeft"
        anchors.verticalCenter: parent.verticalCenter
        x: root.width / 2 - Theme.iconSizeLarge / 2 - Theme.paddingLarge - width
           - root.drift * Theme.paddingLarge
        opacity: root.axis === "y" ? 0 : 1 - root.drift * 0.7
        width: Theme.iconSizeSmall
        height: Theme.iconSizeSmall
        source: "image://theme/icon-m-left?" + root.answerColor
    }

    Image {
        objectName: "answerRight"
        anchors.verticalCenter: parent.verticalCenter
        x: root.width / 2 + Theme.iconSizeLarge / 2 + Theme.paddingLarge
           + root.drift * Theme.paddingLarge
        opacity: root.axis === "y" ? 0 : 1 - root.drift * 0.7
        width: Theme.iconSizeSmall
        height: Theme.iconSizeSmall
        source: "image://theme/icon-m-right?" + root.answerColor
    }

    // Red, under it: up silences, and leads to declining.
    Image {
        objectName: "silenceUp"
        visible: !root.silenced
        anchors.horizontalCenter: parent.horizontalCenter
        y: root.height / 2 + Theme.iconSizeLarge / 2 + Theme.paddingLarge
           - root.drift * Theme.paddingLarge
        opacity: root.axis === "x" ? 0 : 1 - root.drift * 0.7
        width: Theme.iconSizeSmall
        height: Theme.iconSizeSmall
        source: "image://theme/icon-m-up?" + Theme.errorColor
    }

    /// The green the phone answers in.
    readonly property color answerColor: "#4cd964"

    MouseArea {
        anchors.fill: parent
        // A drag here is the handset's, not the page's to flick.
        preventStealing: true

        property real startX: 0
        property real startY: 0

        onPressed: {
            startX = mouse.x
            startY = mouse.y
            root.axis = ""
        }

        onPositionChanged: {
            var moveX = mouse.x - startX
            var moveY = mouse.y - startY
            if (root.axis === "") {
                if (Math.abs(moveX) > Theme.startDragDistance
                        && Math.abs(moveX) >= Math.abs(moveY)) {
                    root.axis = "x"
                } else if (!root.silenced && -moveY > Theme.startDragDistance) {
                    root.axis = "y"
                }
            }
            if (root.axis === "x") {
                root.dx = Math.max(-1.2 * root.reach, Math.min(1.2 * root.reach, moveX))
            } else if (root.axis === "y") {
                root.dy = Math.max(-1.2 * root.lift, Math.min(0, moveY))
            }
        }

        onReleased: {
            var went = root.release(root.axis === "x" ? root.dx : 0,
                                    root.axis === "y" ? root.dy : 0)
            root.axis = ""
            if (went !== "answered") {
                root.dx = 0
                root.dy = 0
            }
        }

        onCanceled: {
            root.axis = ""
            root.dx = 0
            root.dy = 0
        }
    }
}
