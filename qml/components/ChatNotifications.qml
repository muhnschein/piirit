import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Contact/group page row: notifications On or Muted. Tap opens
 * MuteDialog.qml for both; muted chat gets Unmute there. Core keeps no
 * mute end time readable here, so value says only Muted.
 */
ValueButton {
    id: root

    /// Chat muted now (core's `isMuted`).
    property bool muted: false
    /// Replies to reader still notify while muted: group with mention
    /// setting on. Caller decides.
    property bool mentionsStillNotify: false

    /// Reader picked; -1 forever, 0 unmute.
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
