import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * DoorRows two to a line. Any name too wide for its half (long word, big text):
 * one per line. Bindings, not Grid: polish never runs headless.
 */
Item {
    id: grid

    /// Kinds in order (Media.doors).
    property var kinds: []

    signal opened(string kind)

    /// Name room in half a line: page margin, icon and its gap; small gap mid-line.
    readonly property real halfRoom: grid.width / 2 - Theme.horizontalPageMargin
                                     - Theme.iconSizeMedium - Theme.paddingLarge
                                     - Theme.paddingMedium
    // Reads each row's labelWidth: refits on language or text size change.
    readonly property bool paired: {
        for (var i = 0; i < doors.count; i++) {
            var door = doors.itemAt(i)
            if (door && door.labelWidth > grid.halfRoom) {
                return false
            }
        }
        return doors.count > 0
    }
    readonly property int columns: grid.paired ? 2 : 1

    width: parent ? parent.width : 0
    height: Math.ceil(grid.kinds.length / grid.columns) * Theme.itemSizeSmall

    Repeater {
        id: doors
        model: grid.kinds

        DoorRow {
            readonly property int column: index % grid.columns

            objectName: modelData + "Row"
            kind: modelData
            x: column * grid.width / grid.columns
            y: Math.floor(index / grid.columns) * Theme.itemSizeSmall
            width: grid.width / grid.columns
            leftMargin: column === 1 ? Theme.paddingMedium : Theme.horizontalPageMargin
            rightMargin: grid.paired && column === 0 ? Theme.paddingMedium
                                                     : Theme.horizontalPageMargin
            onClicked: grid.opened(modelData)
        }
    }
}
