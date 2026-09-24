pragma Singleton
import QtQuick 2.0

// Stands in for Sailfish.WebEngine's WebEngine, the browser engine's own
// context. What this app tells it is recorded rather than sent, so a test
// can see what the engine would have been told: one line per
// notification, the topic and the data as JSON.
QtObject {
    property string notified: ""

    function notifyObservers(topic, data) {
        notified += topic + " " + JSON.stringify(data) + "\n"
    }
}
