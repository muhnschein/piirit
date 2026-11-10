import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Add row: row shape, plus where picture stands. Sits under last row.
 *
 * Words handed in, so each stays in the page's catalog.
 */
ListItem {
    id: row

    /// Row text, such as "Add profile".
    property alias text: label.text

    contentHeight: Theme.itemSizeSmall + 2 * Theme.paddingMedium

    PlusMark {
        id: plus
        x: Theme.horizontalPageMargin
        y: Theme.paddingMedium
    }

    Label {
        id: label
        x: plus.x + plus.width + Theme.paddingMedium
        width: parent.width - x - Theme.horizontalPageMargin
        anchors.verticalCenter: plus.verticalCenter
        wrapMode: Text.Wrap
        color: row.highlighted ? Theme.highlightColor : Theme.primaryColor
    }
}
