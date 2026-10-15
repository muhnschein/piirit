import QtQuick 2.0

/*
 * Stub: platform countdown over item being destroyed; tap cancels. From real
 * file. `execute`: sibling of item, faded not hidden (hiding also hides
 * children's `visible`). Callback runs when cut short (item destroyed, page
 * deactivated), not dropped; app relies on neither (PendingRemoval.qml owns
 * deletion). Countdown 50ms, not five seconds.
 */
Item {
    id: root

    property string text
    property string cancelText: "Tap to cancel"
    signal canceled()
    signal triggered()

    /// Drawn over; tests read: countdown up on this row.
    property Item _item: null
    readonly property bool active: root._item !== null

    property var _callback: null

    function execute(item, title, callback, timeout) {
        root.text = title
        root._callback = callback
        root._item = item
        // Sibling of covered item, faded not hidden.
        root.parent = item.parent
        root.anchors.fill = item
        item.opacity = 0.0
        countdown.interval = timeout === undefined ? 50 : Math.min(timeout, 50)
        countdown.restart()
    }

    function cancel() {
        if (!root.active) {
            return
        }
        countdown.stop()
        root._forget()
        root.canceled()
    }

    /// Tap by reader: cancel.
    function tap() {
        root.cancel()
    }

    function _forget() {
        if (root._item) {
            root._item.opacity = 1.0
        }
        root._item = null
        root._callback = null
    }

    function _run() {
        var callback = root._callback
        countdown.stop()
        root._forget()
        if (callback) {
            callback()
        }
        root.triggered()
    }

    Timer {
        id: countdown
        interval: 50
        onTriggered: root._run()
    }

    // Cut short: acts, not forgets; so does stub.
    Component.onDestruction: {
        if (countdown.running) {
            root._run()
        }
    }
}
