import QtQuick 2.0
import Sailfish.Silica 1.0

// Accepts. Silica fills `acceptDestination` on screen, exposed as
// `acceptDestinationInstance` for `onAccepted`. Here harness sets instance,
// calls `accept()`; push, with `acceptDestinationProperties`, recorded by injected
// `pageStack`.
Page {
    property bool canAccept: true
    property var acceptDestination
    property var acceptDestinationProperties
    property int acceptDestinationAction: 0
    property var acceptDestinationInstance
    signal accepted()
    signal rejected()
    function accept() {
        if (!canAccept) {
            return
        }
        accepted()
        if (!acceptDestination) {
            return
        }
        // Silica's default: on top, back returns here.
        if (acceptDestinationAction === PageStackAction.Replace) {
            pageStack.replace(acceptDestination, acceptDestinationProperties || {})
        } else {
            pageStack.push(acceptDestination, acceptDestinationProperties || {})
        }
    }
    function reject() { rejected() }
}
