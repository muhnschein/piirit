pragma Singleton
import QtQuick 2.0

// Silica StandardPaths: user's folders on device. Writable here: test points
// page at own directory.
QtObject {
    property string documents: "/tmp/piirit-stub-standardpaths/Documents"
    property string music: "/tmp/piirit-stub-standardpaths/Music"
    property string pictures: "/tmp/piirit-stub-standardpaths/Pictures"
    property string videos: "/tmp/piirit-stub-standardpaths/Videos"
    property string download: "/tmp/piirit-stub-standardpaths/Downloads"
}
