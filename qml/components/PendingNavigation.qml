import QtQuick 2.0

/*
 * Page-stack move, held until stack can make it.
 *
 * Silica's PageStack refuses push/replace mid-transition, logging only to
 * stderr. Fine for taps, not core moves (backup import, last-profile delete
 * lost moves on phone).
 *
 * So retry until done. `busy`: Silica's property (absent in test probe: never
 * busy). `ready`: owner's own condition, page on screen with nothing over it
 * (Silica won't move from under a picker either). Call page over owner must
 * not move: moves act on stack top, taking call page and call with it
 * (CallCenter.qml). So waits for `ready` indefinitely; `patience` only bounds
 * busy waits.
 */
QtObject {
    id: root

    /// Stack to move. Owner passes it: `pageStack` is unreachable here.
    property var stack: null

    /// Owner can move: page sets `page.status === PageStatus.Active` (on screen,
    /// nothing over it).
    property bool ready: true

    /// Move waiting. Page with one closes back gesture (swipe takes move too).
    readonly property bool pending: root._move !== null

    /// Retry period mid-transition, ms.
    property int retryInterval: 50

    /// Max wait on busy stack, ms. Stops never-quiet stack leaving timer alive.
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

    function _hold(move) {
        root._move = move
        root._waited = 0
        root._go()
    }

    /// Drop move not yet made. Unused in app; lets page change its mind.
    function forget() {
        root._move = null
        root._retry.stop()
    }

    /// Make move, or retry later.
    function _go() {
        if (!root._move || !root.stack) {
            return
        }
        // Wait until owner on screen. `onReadyChanged` retries.
        if (!root.ready) {
            root._retry.stop()
            return
        }
        // `=== true`: stack without property gives undefined, not busy.
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
