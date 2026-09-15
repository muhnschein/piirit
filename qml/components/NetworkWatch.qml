import QtQuick 2.0
import Nemo.DBus 2.0

/*
 * The phone's own account of its network, passed on as two signals.
 *
 * A connection the core is holding is killed by a change of network --
 * wi-fi to mobile data on the way out of the house, and back again on the
 * way in -- and killed silently: nothing arrives on it and nothing says
 * so. The core finds out when its IDLE times out five minutes later, and
 * until then a message sent to this phone does not land. The window asks
 * the core to look again when it comes back to the front, which covers
 * the reader who opens the app; this covers the reader who does not,
 * which is where a notification has to come from.
 *
 * The other half is the network going away and staying away -- a tunnel, a
 * lift, a basement, a flight. The core keeps trying to reconnect over a
 * network that is not there, and every attempt wakes the radio for a
 * failure. So a loss that lasts is passed on too, and the window stops the
 * core's IO until there is something to carry it. See docs/POWER.md.
 *
 * Nothing here polls or wakes anything. connman announces every change of
 * connectivity on the system bus the moment it happens, because the rest
 * of the phone depends on it; this listens, and is silent in between. The
 * sandbox already allows it: `Internet` in the desktop file's
 * `[X-Sailjail]` section includes `Connman.permission`, which is what
 * grants `net.connman` on the system bus.
 *
 * connman's `State` is one of "offline", "idle", "ready" or "online". The
 * first two are no usable network and the last two are one. A state that
 * has not changed is not news: connman repeats itself.
 *
 * What this cannot see is a handover where connman never leaves "online",
 * which is possible when a second service is already up and takes over
 * directly. The way back from that is the window's own ask on the way in,
 * and the core's IDLE timeout behind it.
 */
Item {
    id: watch

    /// The network has come back, or changed under the app to one that
    /// works. Whoever holds this decides what to do about it.
    signal networkChanged()

    /// There has been no network for a while and there is still none.
    ///
    /// Deliberately slow to arrive (see `lostMs`) and never sent on a
    /// state connman has not positively announced: what is done about it
    /// costs messages if it is wrong.
    signal networkLost()

    /// The last connectivity connman announced, "" before it has said
    /// anything. Kept so a state repeated is not read as a change.
    property string connectivity: ""

    /// Whether the last thing connman said was that there is a network.
    /// False before it has said anything, which is not the same as knowing
    /// there is none -- nothing acts on this being false on its own.
    readonly property bool online: watch.connectivity === "ready"
                                   || watch.connectivity === "online"

    /// How long to wait before passing a return on.
    ///
    /// A single handover is several announcements -- idle, then ready,
    /// then online -- and each one would otherwise be its own ask. Long
    /// enough to take those as the one change they are, short enough that
    /// nobody waits on it.
    readonly property int settleMs: 1500

    /// How long the network has to stay gone before that is passed on.
    ///
    /// Much longer than `settleMs`, and for the opposite reason. A return
    /// asked for early costs a reconnection; a loss acted on early costs
    /// messages, because a handover passes through "idle" on its way from
    /// one network to the next and a phone mid-handover has not lost
    /// anything. Half a minute is longer than any handover and far shorter
    /// than the stretch of failed reconnections this exists to prevent.
    ///
    /// Writable only so a test need not sit through half a minute of it.
    /// Nothing in the app sets it.
    property int lostMs: 30000

    /// `DBusInterface.Available`, by value.
    ///
    /// Nemo.DBus declares it as `enum Status { Unknown, Unavailable,
    /// Available }` on the C++ type, so on a device `DBusInterface.Available`
    /// would resolve. It is named here instead because QML before 5.10 has
    /// no way to declare an enum, so the stub the tests load cannot carry
    /// one, and the device floor for this app is Qt 5.6.
    readonly property int dbusAvailable: 2

    /// What connman said, as the component's own entry point. The
    /// interface below calls this, and so does the first look at the
    /// state; nothing else does.
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
            // Whichever way round: a network that is back cancels a loss
            // that was on its way to being announced.
            lost.stop()
            settle.restart()
        } else {
            settle.stop()
            // Started, never restarted: connman passes through "idle" on
            // its way to "offline", and a timer restarted on each of them
            // would put the announcement off for as long as the phone kept
            // changing its mind about which kind of nothing it has.
            if (!lost.running) {
                lost.start()
            }
        }
    }

    /// Ask connman what the state is now, rather than waiting for it to
    /// change.
    ///
    /// Without this the app knows nothing until the next handover, so a
    /// phone launched in a basement would hold IO open against a network
    /// it has never had. connman does not carry its properties on the
    /// standard interface, so this is its own `GetProperties` rather than
    /// a property read. Failure is silence: no connman means no answer
    /// means the state stays unknown, and an unknown state stops nothing.
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
        // connman is up before the app is and stays up, but it can be
        // restarted -- and an interface that introspected while it was
        // away never connects to anything afterwards. Watching the name
        // means the signals are hooked up when it comes back instead.
        watchServiceStatus: true

        /// connman's own `PropertyChanged`, which Nemo.DBus delivers to
        /// the function named after it with the first letter lowered.
        function propertyChanged(name, value) {
            watch.heard(name, value)
        }

        // Asked again each time connman appears, which covers both the
        // app starting before connman and connman being restarted under
        // it: either way the answer that matters is the one it has now.
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
        onTriggered: watch.networkLost()
    }

    Component.onCompleted: watch.look()
}
