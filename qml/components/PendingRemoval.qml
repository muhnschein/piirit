import QtQuick 2.0

/*
 * Removal wait owned by list: rows die on delete/reload/reorder (arrival
 * reorders chat list), and Silica's `ListItem.remorseAction` lives in row, so
 * delete runs lost items on phone. Drawn: Silica's `RemorseItem`, raised by
 * row with no-op callback; this acts.
 *
 * Own deadline per id, timer for soonest. One shared countdown restarted per
 * delete: first drawn one ends, platform restores item, all go at last end.
 * Pinned by `qml_delete_clocks`.
 *
 * `pending(id)`, `remaining(id)` for rows rebuilt mid-wait. Leaving page
 * calls `flush()`: timers unfired at exit, user asked for removal.
 *
 * QtObject: draws nothing; Item in Column takes layout slot.
 */
QtObject {
    id: root

    /// Action per id once wait up. `tag`: what `ask` got (e.g. delete for me vs
    /// everyone, kept to wait end); `undefined` if none.
    signal remove(int id, var tag)

    /// Wait, ms: 4 s, as rows' `RemorseItem` (Silica default 5 s). Tests shorten.
    property int delay: 4000

    /// Tag per waiting id. Apart from `ids` so row bindings read only `ids`.
    property var tags: ({})

    /// Waiting id to deadline (clock ms). Replaced, not mutated: bindings re-run
    /// on assignment only, so `pending()` works in rows.
    property var ids: ({})

    /// Whether id is on its way out.
    function pending(id) {
        return root.ids[id] !== undefined
    }

    /// Ms left on id's wait, 0 if not waiting. Seeds countdown for rebuilt row.
    function remaining(id) {
        if (!root.pending(id)) {
            return 0
        }
        return Math.max(1, root.ids[id] - Date.now())
    }

    /// Countdown ms for `RemorseItem.execute`: time left, or `delay`. Only number
    /// rows may pass (`qml_syntax.rs` checks), so countdown and deletion end
    /// together.
    function countdownFor(id) {
        var left = root.remaining(id)
        return left > 0 ? left : root.delay
    }

    /// Queue id after `delay`; `tag` goes to `remove` (omit if one kind). Others
    /// keep deadlines.
    function ask(id, tag) {
        var next = root.copied()
        next[id] = Date.now() + root.delay
        root.ids = next
        var marks = root.copiedTags()
        marks[id] = tag
        root.tags = marks
        root.arm()
    }

    /// Reader cancelled one id. Others keep deadlines.
    function spare(id) {
        var next = root.copied()
        delete next[id]
        root.ids = next
        var marks = root.copiedTags()
        delete marks[id]
        root.tags = marks
        root.arm()
    }

    /// Everything waiting goes now.
    function flush() {
        var going = root.ids
        var marks = root.tags
        root.ids = ({})
        root.tags = ({})
        root.countdown.stop()
        // Cleared first: `remove` may re-enter here.
        root.send(Object.keys(going), marks)
    }

    /// Arm timer for soonest deadline, or stop. Run on every change and firing.
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
        // At least 1 ms: interval 0 never fires on some Qt; past deadline acts now.
        root.countdown.interval = Math.max(1, soonest - Date.now())
        root.countdown.restart()
    }

    /// Remove due ids; rest keep waiting.
    function ripe() {
        var now = Date.now()
        var going = []
        var next = {}
        var marks = root.tags
        var kept = {}
        for (var key in root.ids) {
            if (root.ids[key] <= now) {
                going.push(key)
            } else {
                next[key] = root.ids[key]
                kept[key] = marks[key]
            }
        }
        root.ids = next
        root.tags = kept
        root.arm()
        root.send(going, marks)
    }

    /// Call `remove` per id, oldest first, with its tag.
    function send(waiting, marks) {
        waiting.sort(function(one, other) {
            return parseInt(one, 10) - parseInt(other, 10)
        })
        for (var i = 0; i < waiting.length; i++) {
            root.remove(parseInt(waiting[i], 10), marks[waiting[i]])
        }
    }

    /// Copy of `ids` to change and assign back.
    function copied() {
        var next = {}
        for (var key in root.ids) {
            next[key] = root.ids[key]
        }
        return next
    }

    /// Copy of `tags`.
    function copiedTags() {
        var next = {}
        for (var key in root.tags) {
            next[key] = root.tags[key]
        }
        return next
    }

    property Timer countdown: Timer {
        objectName: "removalCountdown"
        onTriggered: root.ripe()
    }
}
