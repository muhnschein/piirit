import QtQuick 2.0
import Nemo.DBus 2.0

/*
 * Network state as two signals. Change silently kills core's connection (found at 5 min IDLE
 * timeout); window rechecks on front, this covers app in background. Lasting loss: core
 * retries waking radio, so window stops IO until return.
 *
 * connman announces on system bus (`Internet` in `[X-Sailjail]` grants `net.connman` via
 * `Connman.permission`). `State` "offline"/"idle" = none; "ready"/"online" = network.
 * Blind spot: handover staying "online" (window's ask, IDLE timeout cover it).
 */
Item {
    id: watch

    /// Network back, or changed to a working one. Holder decides.
    signal networkChanged()

    /// No network for a while. Slow (`lostMs`), only on announced state: wrong loss costs
    /// messages.
    signal networkLost()

    /// Last connman state; "" before any. Repeats not change.
    property string connectivity: ""

    /// True from `networkLost` until network announced, for listeners absent then (core
    /// restarting). Set only here; writable: QML <5.10 can't set it from handler otherwise.
    property bool lost: false

    /// Last announced state is network. False before any too: nothing acts on false alone.
    readonly property bool online: watch.connectivity === "ready"
                                   || watch.connectivity === "online"

    /// Wait before passing a return on: merges handover (idle, ready, online).
    readonly property int settleMs: 1500

    /// How long gone before loss passed on. Far over `settleMs`: early return costs reconnect,
    /// early loss costs messages; 30 s beats any handover. Writable: tests skip it.
    property int lostMs: 30000

    /// `DBusInterface.Available` by value: QML <5.10 can't declare enums, test stub lacks it.
    readonly property int dbusAvailable: 2

    /// connman's state in; interface and first look call it.
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
            // Return cancels a pending loss, ends an announced one.
            lost.stop()
            watch.lost = false
            settle.restart()
        } else {
            settle.stop()
            // Started, not restarted: "idle" en route to "offline" would defer it forever.
            if (!lost.running) {
                lost.start()
            }
        }
    }

    /// Ask connman now, else app launched offline holds IO open. connman's own
    /// `GetProperties` (no standard interface). Failure: state unknown, stops nothing.
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
        // connman can restart; interface introspected while away never reconnects, so
        // name watched.
        watchServiceStatus: true

        /// connman's `PropertyChanged`: Nemo.DBus calls same-named function, first letter lowered.
        function propertyChanged(name, value) {
            watch.heard(name, value)
        }

        // Each time connman appears: app started first, connman restart.
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
