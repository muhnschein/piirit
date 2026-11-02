import QtQuick 2.0

/*
 * Stub: Silica's page-wide countdown before an action; tap cancels. Countdown 50ms, not
 * five seconds. Tests read `active`, `text`.
 */
Item {
    id: root

    property string text
    readonly property bool active: countdown.running
    property var _callback: null
    signal canceled()
    signal triggered()

    function execute(title, callback, timeout) {
        root.text = title
        root._callback = callback
        countdown.restart()
    }

    function cancel() {
        if (!countdown.running) {
            return
        }
        countdown.stop()
        root._callback = null
        root.canceled()
    }

    Timer {
        id: countdown
        interval: 50
        onTriggered: {
            var callback = root._callback
            root._callback = null
            if (callback) {
                callback()
            }
            root.triggered()
        }
    }
}
