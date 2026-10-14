pragma Singleton
import QtQuick 2.0

/*
 * Current time as a binding dependency.
 *
 * `Date.now()` in a binding is read once; QML never re-evaluates. Row drawn
 * on arrival said "now" for delegate's life (days). Issue 108.
 *
 * Relative times read `Clock.now`; one tick re-evaluates all. One shared
 * timer: per-row timers would be 300 chats = 300 timers.
 *
 * Ticks only while app visible. On return clock read at once, so labels are not
 * up to a tick stale (overnight phone shows morning's answer).
 */
QtObject {
    id: clock

    /// Ms since epoch, at last tick.
    property double now: Date.now()

    /// Tick period. Minute is coarsest label step, so labels lag at most this.
    property int interval: 30000

    /// App on screen, so time is needed. Headless tests set this: cannot change
    /// `Qt.application.state`.
    property bool appActive: Qt.application.state === Qt.ApplicationActive

    onAppActiveChanged: {
        if (clock.appActive) {
            clock.now = Date.now()
        }
    }

    /// Singleton: no children for timer, so a property.
    property Timer ticker: Timer {
        interval: clock.interval
        repeat: true
        running: clock.appActive
        onTriggered: clock.now = Date.now()
    }
}
