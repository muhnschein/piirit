pragma Singleton
import QtQuick 2.0

/*
 * Current time as binding dependency: `Date.now()` in a binding is read once
 * (row said "now" for days; issue 108). Relative times read `Clock.now`; one
 * shared timer, not one per row (300 chats). Ticks only while visible; read at
 * once on return, not up to a tick stale.
 */
QtObject {
    id: clock

    /// Ms since epoch, at last tick.
    property double now: Date.now()

    /// Tick period: minute, coarsest label step.
    property int interval: 30000

    /// App on screen. Headless tests set it (can't change `Qt.application.state`).
    property bool appActive: Qt.application.state === Qt.ApplicationActive

    onAppActiveChanged: {
        if (clock.appActive) {
            clock.now = Date.now()
        }
    }

    /// Singleton has no children: timer as property.
    property Timer ticker: Timer {
        interval: clock.interval
        repeat: true
        running: clock.appActive
        onTriggered: clock.now = Date.now()
    }
}
