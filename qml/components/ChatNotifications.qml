import QtQuick 2.0
import Sailfish.Silica 1.0

// Core gives no mute end time: value says only Muted.
ValueButton {
    id: root

    property bool muted: false
    // Group with Mentions on: replies to reader still notify. Caller decides.
    property bool mentionsStillNotify: false

    // -1 forever, 0 unmute.
    signal picked(int seconds)

    label: qsTr("Notifications")
    //: Value of the Notifications row on a contact's or group's page:
    //: chat muted, or notifications on.
    value: root.muted ? qsTr("Muted") : qsTr("On")

    onClicked: {
        var dialog = pageStack.push(Qt.resolvedUrl("../pages/MuteDialog.qml"), {
            muted: root.muted,
            mentionsStillNotify: root.mentionsStillNotify
        })
        if (dialog) {
            dialog.picked.connect(root.picked)
        }
    }
}
