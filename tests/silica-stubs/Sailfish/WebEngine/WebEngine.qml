pragma Singleton
import QtQuick 2.0

// Stub for Sailfish.WebEngine WebEngine. Records what app tells it: one line
// per notification. Real engine drops notifyObservers before its event loop
// starts (qtmozembed QMozContext). Test sets `ready` false for cold start;
// `start()` finishes.
QtObject {
    property string notified: ""
    property bool ready: true

    signal initialized()

    function isInitialized() { return ready }

    function start() {
        ready = true
        initialized()
    }

    function notifyObservers(topic, data) {
        if (!ready) {
            return
        }
        notified += topic + " " + JSON.stringify(data) + "\n"
    }
}
