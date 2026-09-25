import QtQuick 2.0

/*
 * One move of the page stack, held until the stack can actually make it.
 *
 * Silica's PageStack runs one transition at a time. A push or a replace
 * asked for while another is still running is refused, and it says so on
 * stderr and nowhere a reader will ever look -- the app simply stays
 * where it was. That is fine for a move a tap asked for, because a tap
 * lands between transitions; it is not fine for a move the *core* asked
 * for, because the core answers whenever it answers.
 *
 * Both of the ones that matter were met on a phone. A backup finishes
 * importing while the file browser that chose it is still animating
 * away, so the page that was to hand over to the new profile's chats
 * asked while the stack was busy, was refused, and dropped back to
 * "Restore from a backup" with the profile imported behind it. And the
 * last profile is deleted as the profiles page is leaving, so the move
 * back to the welcome page is asked for mid-pop and refused, leaving a
 * chat list open on an account that no longer exists.
 *
 * So the move is kept here and tried again until it goes. `busy` is
 * Silica's own property; a stack that declares none -- the test
 * harness's probe -- is never busy, and the move is made at once.
 * `ready` is the owner's own condition, which a page sets to "I am the
 * page on screen": a page with a picker over it is not one Silica will
 * move from either.
 *
 * And a page with a call over it must not move at all. A call's page
 * is pushed over whatever is on screen (CallCenter.qml), and every move
 * here acts on the top of the stack -- a pop takes the top page, and a
 * replace everything above its target -- so a move made then takes the
 * call's page, and the call with it. So a move waits for `ready` for as
 * long as it takes, and `patience` only bounds the wait on a stack that
 * is busy: the page being on screen again is what makes the move its own.
 */
QtObject {
    id: root

    /// The stack to move. Handed over by whoever owns one of these: a
    /// `pageStack` is a context property, and this cannot reach it.
    property var stack: null

    /// Whether the owner is in a position to move at all. A page sets
    /// `page.status === PageStatus.Active`: the page on screen, with
    /// nothing over it.
    property bool ready: true

    /// Whether a move is waiting to be made. A page with one waiting
    /// closes its back gesture over it: swiping away would take the
    /// page, and the move with it.
    readonly property bool pending: root._move !== null

    /// How often to look again while the stack is mid-transition.
    property int retryInterval: 50

    /// How long to keep trying while the stack is busy, in
    /// milliseconds. A page transition is a few hundred of them; this is
    /// well past any of them, and it is here so that a stack which never
    /// goes quiet leaves a timer running for a few seconds rather than
    /// for the rest of the day.
    property int patience: 5000

    /// The move waiting, as `{ kind, target, page, properties }`, or null.
    property var _move: null
    /// Milliseconds spent waiting for this one.
    property int _waited: 0

    /// Replace everything above `target` with `page` -- now, or as soon
    /// as the stack is finished with whatever it is doing.
    function replaceAbove(target, page, properties) {
        root._hold({ kind: "replaceAbove", target: target, page: page,
                     properties: properties })
    }

    /// Replace the owner with `page`, once the owner is on top.
    function replace(page, properties) {
        root._hold({ kind: "replace", page: page, properties: properties })
    }

    /// Take the owner off the stack, once it is on top.
    function pop() {
        root._hold({ kind: "pop" })
    }

    function _hold(move) {
        root._move = move
        root._waited = 0
        root._go()
    }

    /// Drop a move not yet made. Nothing in the app calls this; it is
    /// here so that a page which changes its mind has a way to say so.
    function forget() {
        root._move = null
        root._retry.stop()
    }

    /// Make the move, or arrange to look again.
    function _go() {
        if (!root._move || !root.stack) {
            return
        }
        // Not until the owner is on screen, however long: looked at
        // again when it is (`onReadyChanged`).
        if (!root.ready) {
            root._retry.stop()
            return
        }
        // `=== true` rather than a plain test: a stack without the
        // property hands back undefined, which is not "busy".
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
