pragma Singleton
import QtQuick 2.0

// Global: phone's cameras. Two back, one front (second lens). Test replaces
// list for one camera or none. `position`: QCamera::Position, BackFace 1,
// FrontFace 2.
QtObject {
    property var availableCameras: [
        { deviceId: "back-0", displayName: "Back", position: 1 },
        { deviceId: "front-0", displayName: "Front", position: 2 },
        { deviceId: "back-1", displayName: "Back 2", position: 1 }
    ]
    property var defaultCamera: availableCameras[0]
}
