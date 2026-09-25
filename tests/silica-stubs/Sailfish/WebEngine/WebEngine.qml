pragma Singleton
import QtQuick 2.0

// Stands in for Sailfish.WebEngine's WebEngine, the browser engine's own
// context. What this app tells it is recorded rather than sent, so a test
// can see what the engine would have been told: one line per
// notification, the topic and the data as JSON.
//
// The real engine starts on the event loop after its module is first
// imported, and until it has, notifyObservers drops what it is told with
// nothing but a warning (qtmozembed's QMozContext::notifyObservers). A
// test sets `ready` false to start it cold, and `start()` to finish.
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
