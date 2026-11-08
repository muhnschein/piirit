import QtQuick 2.0
import Nemo.KeepAlive 1.2
import Piirit 1.0

/*
 * Some chat gets reader's location live: phone's position to core, which sends it on (at
 * most once a minute). App in background too, for the time reader picked: phone kept awake,
 * else suspend stops the feed. Off once core says no chat wants it. Fresh fixes only: stale one
 * would go out as current.
 */
Item {
    id: root

    /// Positions going to core.
    readonly property bool active: live.active
    /// Sharing, but no position going out: location off or lost.
    readonly property bool stalled: live.active && (source.failed || source.lost)
    /// Ms without fresh fix until `stalled`: three missed updates. Tests shorten it.
    property int staleAfter: 90000

    LiveLocation {
        id: live
        objectName: "liveLocation"
    }

    Connections {
        target: core
        // Qt 5.6 handler syntax; see WelcomePage.qml.
        onCore_event: live.handle_event(context_id, kind, payload_json)
        onStatus_changed: {
            if (core.status === "ready") {
                live.refresh()
            }
        }
    }

    Component.onCompleted: {
        if (core.status === "ready") {
            live.refresh()
        }
    }

    FreshPosition {
        id: source
        objectName: "liveFresh"
        sourceName: "livePosition"
        active: live.active
        updateInterval: 30000
        staleAfter: root.staleAfter
        onFix: live.report(latitude, longitude, accuracy)
    }

    KeepAlive {
        objectName: "locationKeepAlive"
        enabled: live.active
    }
}
