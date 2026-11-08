import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Page-stack move, held until stack can make it.
 *
 * Silica PageStack refuses push/replace mid-transition (stderr only): core
 * moves (backup import, last-profile delete) got lost on phone. So retry.
 * `busy`: Silica's (absent in test probe). `ready`: owner on screen, nothing
 * over it (Silica won't move from under picker; move under call page would
 * take call with it, CallCenter.qml). Waits for `ready` indefinitely;
 * `patience` bounds busy waits only.
 */
QtObject {
    id: root

    /// Stack; owner passes it (`pageStack` unreachable here).
    property var stack: null

    /// Owner movable: page binds `page.status === PageStatus.Active`.
    property bool ready: true

    /// Move waiting; page closes back gesture meanwhile.
    readonly property bool pending: root._move !== null

    /// Retry period mid-transition, ms.
    property int retryInterval: 50

    /// Max busy wait, ms: never-quiet stack can't keep timer alive.
    property int patience: 5000

    /// Move waiting: `{ kind, target, page, properties }`, or null.
    property var _move: null
    /// Ms spent waiting on it.
    property int _waited: 0

    /// Replace all above `target` with `page`, now or once stack idle.
    function replaceAbove(target, page, properties) {
        root._hold({ kind: "replaceAbove", target: target, page: page,
                     properties: properties })
    }

    /// Replace owner with `page`, once owner on top.
    function replace(page, properties) {
        root._hold({ kind: "replace", page: page, properties: properties })
    }

    /// Pop owner, once on top.
    function pop() {
        root._hold({ kind: "pop" })
    }

    /// Pop down to `target`, no slide, once owner on top.
    function popTo(target) {
        root._hold({ kind: "popTo", target: target })
    }

    function _hold(move) {
        root._move = move
        root._waited = 0
        root._go()
    }

    /// Drop pending move (unused in app).
    function forget() {
        root._move = null
        root._retry.stop()
    }

    /// Make move, or retry later.
    function _go() {
        if (!root._move || !root.stack) {
            return
        }
        // Wait for owner; `onReadyChanged` retries.
        if (!root.ready) {
            root._retry.stop()
            return
        }
        // `=== true`: missing property = not busy.
        if (root.stack.busy === true && root._waited < root.patience) {
            root._waited += root.retryInterval
            root._retry.restart()
            return
        }
        var move = root._move
        root._move = null
        root._retry.stop()
        if (move.kind === "pop") {
            root.stack.pop()
        } else if (move.kind === "popTo") {
            root.stack.pop(move.target, PageStackAction.Immediate)
        } else if (move.kind === "replace") {
            root.stack.replace(move.page, move.properties)
        } else {
            root.stack.replaceAbove(move.target, move.page, move.properties)
        }
    }

    onReadyChanged: {
        if (root.ready) {
            root._waited = 0
            root._go()
        }
    }

    property Timer _retry: Timer {
        objectName: "navigationRetry"
        interval: root.retryInterval
        onTriggered: root._go()
    }
}
