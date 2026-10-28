import QtQuick 2.0
import QtPositioning 5.2
import Nemo.KeepAlive 1.2
import Piirit 1.0

/*
 * Some chat gets reader's location live: phone's position to core, which sends it on (at
 * most once a minute). App in background too, for the time reader picked: phone kept awake,
 * else suspend stops the feed. Off once core says no chat wants it.
 */
Item {
    id: root

    /// Positions going to core.
    readonly property bool active: live.active

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

    PositionSource {
        id: source
        objectName: "livePosition"
        active: live.active
        updateInterval: 30000
        onPositionChanged: {
            var fix = source.position
            if (fix.latitudeValid && fix.longitudeValid) {
                live.report(fix.coordinate.latitude, fix.coordinate.longitude,
                            fix.horizontalAccuracyValid ? fix.horizontalAccuracy : 0)
            }
        }
    }

    KeepAlive {
        objectName: "locationKeepAlive"
        enabled: live.active
    }
}
