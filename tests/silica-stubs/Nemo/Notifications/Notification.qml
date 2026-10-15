import QtQuick 2.0

// Stub: device-only Notification. Records raise and take-down. Same idea as
// tests/silica-stubs/Sailfish/Silica.
QtObject {
    property string appName
    property string appIcon
    property string summary
    property string body
    property string previewSummary
    property string previewBody
    property string category
    // Real enum on C++ class: `Notification.Critical` resolves here.
    enum Urgency { Low, Normal, Critical }
    // Normal.
    property int urgency: 1
    property int itemCount: 1
    property var timestamp
    property var remoteActions: []
    property int replacesId: 0

    // What the tests read.
    property bool isPublished: false
    property int publishCount: 0
    property int closeCount: 0

    function publish() {
        isPublished = true
        publishCount += 1
        // Real one allocates id on first publish, reuses it.
        if (replacesId === 0) {
            replacesId = publishCount
        }
    }

    function close() {
        isPublished = false
        closeCount += 1
        replacesId = 0
    }
}
