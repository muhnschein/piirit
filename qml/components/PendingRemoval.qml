import QtQuick 2.0

/*
 * The wait before something is destroyed, owned by the list rather than
 * by the row.
 *
 * Silica has this already -- `ListItem.remorseAction` -- and puts the
 * countdown in an item parented to the row it was asked for on. For
 * deleting things *out of* a list that is the one place it cannot go: a
 * delete is exactly what destroys rows. Deleting a run of messages lost
 * most of itself on a phone, and the chat list, the profiles list and a
 * group's members were all open to the same thing -- any reload or
 * reorder destroys rows, and a message arriving reorders the chat list.
 *
 * So the wait lives here, beside the list rather than inside it. What is
 * *drawn* is still Silica's own `RemorseItem`, raised by the row over
 * what is going and handed a callback that does nothing; this is what
 * actually acts when a wait is up.
 *
 * Every id carries its own deadline, and the timer is armed for
 * whichever is soonest. One countdown shared between them would have to
 * be restarted on each new delete, so the first message's countdown would
 * run out, the platform would put that message back, and everything would
 * go together at the last countdown's end -- which a single delete never
 * shows. `qml_delete_clocks` pins that shape.
 *
 * Whoever holds one of these draws the waiting row -- `pending(id)` says
 * which, `remaining(id)` says how much of the wait is left, for a row
 * rebuilt in the middle of one. A page on its way out calls `flush()`,
 * for the reason ConversationPage writes its draft there: leaving is
 * exactly when a timer has not fired yet, and somebody who asked for a
 * thing to go and then left asked for it to go.
 *
 * A `QtObject` rather than an `Item`: it draws nothing, and an Item
 * declared inside a Column would take a turn in the layout.
 */
QtObject {
    id: root

    /// What to do to each one, once its wait is up.
    signal remove(int id)

    /// How long a wait is, in milliseconds. Silica's own remorse waits
    /// five seconds; four, to match what the rows ask their `RemorseItem`
    /// for. A property because a test turns it down rather than sitting
    /// through it.
    property int delay: 4000

    /// The ids waiting, each against the clock reading its own wait is up
    /// at.
    ///
    /// Replaced rather than changed in place -- a binding does not re-run
    /// when the contents of an object it read change, only when the
    /// property is assigned. Which is what makes `pending()` usable in a
    /// row's bindings at all.
    property var ids: ({})

    /// Whether this one is on its way out.
    function pending(id) {
        return root.ids[id] !== undefined
    }

    /// How much of this one's wait is left, in milliseconds; 0 if it is
    /// not waiting. What a row rebuilt mid-wait puts the platform's
    /// countdown back up with.
    function remaining(id) {
        if (!root.pending(id)) {
            return 0
        }
        return Math.max(1, root.ids[id] - Date.now())
    }

    /// How long to give the platform's countdown over this one: what is
    /// left of a wait already running, or a whole one if it has only
    /// just been asked for.
    ///
    /// The one number a row is allowed to pass `RemorseItem.execute`, so
    /// the drawn countdown and the deletion behind it cannot end at
    /// different moments. They did once: the countdown ran out and the
    /// platform put the message back while the list was still waiting on
    /// a shared clock. `qml_syntax.rs` holds every row to asking here.
    function countdownFor(id) {
        var left = root.remaining(id)
        return left > 0 ? left : root.delay
    }

    /// Ask for one to go, once the reader has had their moment.
    ///
    /// Nothing already waiting is touched: its deadline is its own, and
    /// moving it is the bug this shape exists to prevent.
    function ask(id) {
        var next = root.copied()
        next[id] = Date.now() + root.delay
        root.ids = next
        root.arm()
    }

    /// The reader said they did not mean it, about one of them. The rest
    /// keep their own deadlines.
    function spare(id) {
        var next = root.copied()
        delete next[id]
        root.ids = next
        root.arm()
    }

    /// Everything still waiting goes now, whatever is left on it.
    function flush() {
        var going = root.ids
        root.ids = ({})
        root.countdown.stop()
        // Cleared before any of them is acted on: whatever answers
        // `remove` may come straight back here, and must not find a
        // list this is still working through.
        root.send(Object.keys(going))
    }

    /// Point the timer at the soonest deadline there is, or stop it when
    /// there is none. Called whenever the set changes, and again after
    /// each firing.
    function arm() {
        var soonest = 0
        for (var key in root.ids) {
            if (soonest === 0 || root.ids[key] < soonest) {
                soonest = root.ids[key]
            }
        }
        if (soonest === 0) {
            root.countdown.stop()
            return
        }
        // At least a millisecond: an interval of 0 is a timer that never
        // fires on some Qt versions, and a deadline already past should
        // be acted on at once rather than never.
        root.countdown.interval = Math.max(1, soonest - Date.now())
        root.countdown.restart()
    }

    /// Whatever is due goes; the rest keep waiting.
    function ripe() {
        var now = Date.now()
        var going = []
        var next = {}
        for (var key in root.ids) {
            if (root.ids[key] <= now) {
                going.push(key)
            } else {
                next[key] = root.ids[key]
            }
        }
        root.ids = next
        root.arm()
        root.send(going)
    }

    /// Say `remove` for each of them, in the order their ids were asked
    /// for, oldest first.
    function send(waiting) {
        waiting.sort(function(one, other) {
            return parseInt(one, 10) - parseInt(other, 10)
        })
        for (var i = 0; i < waiting.length; i++) {
            root.remove(parseInt(waiting[i], 10))
        }
    }

    /// The set as it stands, to be changed and assigned back.
    function copied() {
        var next = {}
        for (var key in root.ids) {
            next[key] = root.ids[key]
        }
        return next
    }

    property Timer countdown: Timer {
        objectName: "removalCountdown"
        onTriggered: root.ripe()
    }
}
