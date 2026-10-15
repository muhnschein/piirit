import QtQuick 2.0
import Sailfish.Silica 1.0

// Silica SectionHeader insets from page margin both sides, right-aligns text, x
// at left margin. Modelled: `width: parent.width` leaves x while right edge
// moves a margin past screen. Device screenshot showed heading chars cut at
// edge.
Item {
    id: header

    property alias text: label.text

    x: Theme.horizontalPageMargin
    width: parent ? parent.width - 2 * Theme.horizontalPageMargin : 0
    implicitHeight: 60
    height: implicitHeight

    Label {
        id: label
        objectName: "sectionHeaderText"
        anchors.right: parent.right
        horizontalAlignment: Text.AlignRight
    }
}
