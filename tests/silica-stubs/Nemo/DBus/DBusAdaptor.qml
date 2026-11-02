import QtQuick 2.0
import Nemo.DBus 2.0

// Stub: session-bus object, held in StubBus as Nemo's adaptor and QDBusConnection leave it.
// Path taken: registering fails. Teardown unregisters path and name, whoever holds them.
QtObject {
    id: adaptor
    property string service
    property string path
    property string iface
    property string xml

    Component.onCompleted: {
        if (adaptor.path.length > 0 && !(adaptor.path in StubBus.objects)) {
            StubBus.objects[adaptor.path] = adaptor
        }
        if (adaptor.service.length > 0) {
            StubBus.names[adaptor.service] = true
        }
    }
    Component.onDestruction: {
        delete StubBus.objects[adaptor.path]
        if (adaptor.service.length > 0) {
            delete StubBus.names[adaptor.service]
        }
    }
}
