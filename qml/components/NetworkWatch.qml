import QtQuick 2.0
import Nemo.DBus 2.0

/*
 * The phone's own account of its network, passed on as one signal.
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
 * Nothing here polls or wakes anything. connman announces every change of
 * connectivity on the system bus the moment it happens, because the rest
 * of the phone depends on it; this listens, and is silent in between. The
 * sandbox already allows it: `Internet` in the desktop file's
 * `[X-Sailjail]` section includes `Connman.permission`, which is what
 * grants `net.connman` on the system bus.
 *
 * Only arriving at a working network is worth passing on. connman's
 * `State` is one of "offline", "idle", "ready" or "online"; the core's
 * `maybe_network` means "the network may have come back", so losing it is
 * not news -- the core has nothing useful to do about it, and asking it
 * to reconnect to nothing costs a failed attempt. Nor is a state that has
 * not changed: connman repeats itself.
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

    /// The last connectivity connman announced, "" before it has said
    /// anything. Kept so a state repeated is not read as a change.
    property string connectivity: ""

    /// How long to wait before passing a change on.
    ///
    /// A single handover is several announcements -- idle, then ready,
    /// then online -- and each one would otherwise be its own ask. Long
    /// enough to take those as the one change they are, short enough that
    /// nobody waits on it.
    readonly property int settleMs: 1500

    /// What connman said, as the component's own entry point. The
    /// interface below calls this; nothing else does.
    function heard(name, value) {
        if (name !== "State") {
            return
        }
        var state = "" + value
        if (state === watch.connectivity) {
            return
        }
        watch.connectivity = state
        if (state !== "ready" && state !== "online") {
            return
        }
        settle.restart()
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
    }

    Timer {
        id: settle
        interval: watch.settleMs
        onTriggered: watch.networkChanged()
    }
}
