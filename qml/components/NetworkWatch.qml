import QtQuick 2.0
import Nemo.DBus 2.0

/*
 * Phone's network state, as two signals.
 *
 * Network change silently kills core's connection; core learns only at IDLE timeout (5 min).
 * Window asks core to recheck on coming to front; this covers readers who don't open the
 * app, which is where notifications come from.
 *
 * Lasting loss: core retries uselessly, waking radio. So loss is passed on too; window
 * stops IO until network returns.
 *
 * No polling: connman announces changes on system bus. `Internet` in `[X-Sailjail]` grants
 * `net.connman` via `Connman.permission`.
 *
 * connman `State`: "offline"/"idle" = no network; "ready"/"online" = network. Repeats not news.
 *
 * Blind spot: handover staying "online". Way back: window's ask, plus IDLE timeout.
 */
Item {
    id: watch

    /// Network back, or changed to a working one. Holder decides.
    signal networkChanged()

    /// No network for a while. Deliberately slow (`lostMs`), never sent on a state connman
    /// has not announced: acting on a wrong loss costs messages.
    signal networkLost()

    /// Last connman state; "" before any. Repeats not change.
    property string connectivity: ""

    /// True from `networkLost` until a network is announced.
    ///
    /// For readers not listening then (e.g. core restarting), to ask later. Never set on
    /// an unannounced state.
    /// Set only here; writable because QML <5.10 can't set it from a handler otherwise.
    property bool lost: false

    /// Last announced state is network. False before any announcement, not known none.
    /// Nothing acts on false alone.
    readonly property bool online: watch.connectivity === "ready"
                                   || watch.connectivity === "online"

    /// Wait before passing a return on.
    ///
    /// Merges handover announcements (idle, ready, online) into one.
    readonly property int settleMs: 1500

    /// How long gone before loss passed on.
    ///
    /// Much longer than `settleMs`: early return costs a reconnect, early loss costs messages
    /// (handover passes "idle"). Half a minute: longer than any handover.
    /// Writable so tests skip the wait.
    property int lostMs: 30000

    /// `DBusInterface.Available` by value.
    ///
    /// Literal: QML <5.10 can't declare enums, so the test stub lacks one. Device floor Qt 5.6.
    readonly property int dbusAvailable: 2

    /// connman's state, entry point. Interface and first look call it; nothing else.
    function heard(name, value) {
        if (name !== "State") {
            return
        }
        var state = "" + value
        if (state === watch.connectivity) {
            return
        }
        watch.connectivity = state
        if (watch.online) {
            // Either way: back network cancels a loss being announced, ends one announced.
            lost.stop()
            watch.lost = false
            settle.restart()
        } else {
            settle.stop()
            // Started, not restarted: connman passes "idle" en route to "offline"; restart
            // on each would defer it forever.
            if (!lost.running) {
                lost.start()
            }
        }
    }

    /// Ask connman for state now. Without it, a phone launched in a basement holds IO
    /// open on a network it never had.
    ///
    /// connman's own `GetProperties` (no standard properties interface). Failure is silence:
    /// state stays unknown, and unknown stops nothing.
    function look() {
        manager.typedCall("GetProperties", [], function (properties) {
            if (properties && properties.State !== undefined) {
                watch.heard("State", properties.State)
            }
        }, function () {})
    }

    DBusInterface {
        id: manager
        objectName: "connmanManager"
        bus: DBus.SystemBus
        service: "net.connman"
        path: "/"
        iface: "net.connman.Manager"
        signalsEnabled: true
        // connman can restart; interface introspected while away never reconnects.
        // Watching the name re-hooks signals on return.
        watchServiceStatus: true

        /// connman's `PropertyChanged`: Nemo.DBus calls same-named function, first letter lowered.
        function propertyChanged(name, value) {
            watch.heard(name, value)
        }

        // Asked each time connman appears: covers app starting first, and connman restart.
        onStatusChanged: {
            if (manager.status === watch.dbusAvailable) {
                watch.look()
            }
        }
    }

    Timer {
        id: settle
        interval: watch.settleMs
        onTriggered: watch.networkChanged()
    }

    Timer {
        id: lost
        interval: watch.lostMs
        onTriggered: {
            watch.lost = true
            watch.networkLost()
        }
    }

    Component.onCompleted: watch.look()
}
