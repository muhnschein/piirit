import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Setting as stacked icons; the one in force lit. Choice name shows in panel
 * header on press and pick.
 */
Column {
    id: column

    /// Choices, icons, labels: same order.
    property var values: []
    property var icons: []
    property var labels: []
    property var current

    /// Choice picked.
    signal chosen(var value)
    /// Header: pressed or picked choice name.
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
                // Icon briefly missing while lists update.
                source: index < column.icons.length
                        ? column.icons[index] + "?" + (parent.pressed ? Theme.highlightColor : "#ffffff")
                        : ""
            }
        }
    }
}
