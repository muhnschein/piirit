pragma Singleton
import QtQuick 2.0

// QtMultimedia's global object: what cameras the phone has. Two on the
// back and one on the front by default, as on a phone with a second
// lens; a test replaces the list to give the phone one camera, or none
// on a side. `position` is QCamera::Position's value: BackFace 1,
// FrontFace 2.
QtObject {
    property var availableCameras: [
        { deviceId: "back-0", displayName: "Back", position: 1 },
        { deviceId: "front-0", displayName: "Front", position: 2 },
        { deviceId: "back-1", displayName: "Back 2", position: 1 }
    ]
    property var defaultCamera: availableCameras[0]
}
