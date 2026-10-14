import QtQuick 2.0

// Platform triggers this on share to app. Real one registers method against
// desktop-file group of same name; this carries only what test hands it, enough
// to drive this side.
Item {
    property string method: ""
    property var capabilities: []
    property bool registerName: false

    signal triggered(var resources)
}
