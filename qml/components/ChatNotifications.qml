import QtQuick 2.0
import Sailfish.Silica 1.0
import Piirit 1.0
import "../js/Mute.js" as Mute

// Muted for how long and time left, as kept when muted here; length dropped when the line
// would not fit.
ValueButton {
    id: root

    property int accountId
    property int chatId
    property string chatName
    /// Chat's ChatInfo: its reload after a save rereads the kept mute.
    property QtObject chat
    property bool muted: false
    // Group with Mentions on: replies to reader still notify. Caller decides.
    property bool mentionsStillNotify: false

    // -1 forever, 0 unmute.
    signal picked(int seconds)

    MuteState {
        id: kept
        account_id: root.accountId
        chat_id: root.chatId
        muted: root.muted
    }

    Connections {
        target: root.chat
        ignoreUnknownSignals: true
        onLoaded_changed: kept.reload()
    }

    // Clock read when the kept mute comes in, not ticking.
    property real now: 0
    Connections {
        target: kept
        onLoaded_changed: root.now = kept.now()
    }

    readonly property string fullValue: Mute.rowValue(root.muted, kept.length, kept.until,
                                                      root.now, false)
    Label {
        id: measure
        visible: false
        text: root.label + "  " + root.fullValue
    }
    readonly property bool fits: measure.implicitWidth
                                 <= root.width - 2 * Theme.horizontalPageMargin

    label: qsTr("Notifications")
    value: root.fits ? root.fullValue
                     : Mute.rowValue(root.muted, kept.length, kept.until, root.now, true)

    onClicked: {
        var page = pageStack.push(Qt.resolvedUrl("../pages/ChatNotificationsPage.qml"), {
            accountId: root.accountId,
            chatId: root.chatId,
            chatName: root.chatName,
            muted: root.muted,
            mentionsStillNotify: root.mentionsStillNotify
        })
        if (page) {
            page.picked.connect(root.picked)
        }
    }
}
