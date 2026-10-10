import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * One setting in the camera's settings panel: its choices stacked as
 * icons, the one in force on a lit disc, as the platform camera draws a
 * column of its own panel. What a choice is called shows in the panel's
 * header while it is pressed, and for a moment after it is picked.
 */
Column {
    id: column

    /// The choices, and their icons and names in the same order.
    property var values: []
    property var icons: []
    property var labels: []
    property var current

    /// A choice was picked.
    signal chosen(var value)
    /// What to show in the header: a choice's name, pressed or picked.
    signal described(string text)

    spacing: Theme.paddingSmall
    visible: values.length > 0

    Repeater {
        model: column.values

        MouseArea {
            objectName: column.objectName + "Option" + index
            width: Theme.itemSizeSmall
            height: width
            readonly property bool selected: modelData === column.current

            onPressed: column.described(column.labels[index] || "")
            onClicked: {
                column.chosen(modelData)
                column.described(column.labels[index] || "")
            }

            Rectangle {
                anchors.centerIn: parent
                width: Theme.iconSizeMedium + Theme.paddingSmall
                height: width
                radius: width / 2
                color: Theme.highlightBackgroundColor
                opacity: parent.selected || parent.pressed ? 0.5 : 0
            }

            Image {
                anchors.centerIn: parent
                // The lists change one binding at a time when the choices
                // do, so for a moment an icon can be missing.
                source: index < column.icons.length
                        ? column.icons[index] + "?" + (parent.pressed ? Theme.highlightColor : "#ffffff")
                        : ""
            }
        }
    }
}
