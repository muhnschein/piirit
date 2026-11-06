import QtQuick 2.0
import QtPositioning 5.2

/*
 * Phone's position, fresh only. Sailfish's source hands last known fix (kept on disk) as
 * current, and on location off just stops: fix stays valid, no error. Fix counts under a
 * minute old; none for `staleAfter`: `lost`.
 */
Item {
    id: root

    property bool active: false
    property int updateInterval: 1000
    /// Ms without fresh fix until `lost`.
    property int staleAfter: 15000
    property alias sourceName: source.objectName
    property alias position: source.position

    /// Fresh fix in hand.
    property bool fresh: false
    /// Looking, nothing fresh for `staleAfter`: location likely off.
    property bool lost: false
    /// Source refuses: location off at start, or no access. NoError is 3, not 0 (Qt's enum).
    readonly property bool failed: source.sourceError !== PositionSource.NoError || !source.valid
    readonly property bool ready: root.fresh && !root.failed

    /// Fresh fix; accuracy 0 unknown.
    signal fix(real latitude, real longitude, real accuracy)

    function take(position) {
        if (!position.latitudeValid || !position.longitudeValid) {
            return
        }
        // No timestamp: NaN, not fresh.
        if (!(Date.now() - position.timestamp.getTime() <= 60000)) {
            return
        }
        root.fresh = true
        root.lost = false
        stale.restart()
        root.fix(position.coordinate.latitude, position.coordinate.longitude,
                 position.horizontalAccuracyValid ? position.horizontalAccuracy : 0)
    }

    function reset() {
        root.fresh = false
        root.lost = false
        if (root.active) {
            stale.restart()
            root.take(source.position)
        } else {
            stale.stop()
        }
    }

    onActiveChanged: root.reset()
    Component.onCompleted: root.reset()

    PositionSource {
        id: source
        active: root.active
        updateInterval: root.updateInterval
        onPositionChanged: root.take(source.position)
    }

    Timer {
        id: stale
        interval: root.staleAfter
        onTriggered: {
            root.fresh = false
            root.lost = true
        }
    }
}
