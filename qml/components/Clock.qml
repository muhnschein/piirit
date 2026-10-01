pragma Singleton
import QtQuick 2.0

/*
 * What time it is, as something a binding can depend on.
 *
 * A row that says how long ago its chat's last message was ("now",
 * "5 min", "2 h") is answering a question about the present, and
 * `Date.now()` inside a binding is read once: nothing tells QML the
 * present has moved on, so the binding is never evaluated again. A row
 * drawn at the moment a message arrived said "now" for as long as its
 * delegate lived -- days, for a row at the top of a list that is never
 * scrolled away, since an app here is closed about as often as the
 * phone is restarted. Pinning the chat or opening it rewrote the row's
 * other roles and left its time alone, because the time had not
 * changed; restarting the app built the delegate again. Issue 108.
 *
 * Every relative time reads `Clock.now` instead, so a tick re-evaluates
 * every one of them at once. One shared timer rather than one per row:
 * a list of three hundred chats is three hundred delegates.
 *
 * Only while the app is on screen. A time label nobody can see does not
 * need to be right, and the moment it can be seen again the clock is
 * read at once, so a phone left in a drawer overnight shows the morning's
 * answer rather than the evening's for up to a tick.
 */
QtObject {
    id: clock

    /// Milliseconds since the epoch, as of the last tick.
    property double now: Date.now()

    /// How often the clock is read. The coarsest label boundary is a
    /// minute, so a label is at most this late in crossing it.
    property int interval: 30000

    /// Whether anything can be on screen to need the time. A headless
    /// test cannot change `Qt.application.state`, so it sets this
    /// instead, as it does the window's own `appActive`.
    property bool appActive: Qt.application.state === Qt.ApplicationActive

    onAppActiveChanged: {
        if (clock.appActive) {
            clock.now = Date.now()
        }
    }

    /// A singleton has no children of its own to hold the timer, so it is
    /// a property: a `QtObject` takes no child objects.
    property Timer ticker: Timer {
        interval: clock.interval
        repeat: true
        running: clock.appActive
        onTriggered: clock.now = Date.now()
    }
}
