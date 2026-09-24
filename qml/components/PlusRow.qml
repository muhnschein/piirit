import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * The row that adds one more of something -- a profile, a member, a
 * relay, someone to block -- where the next one would be listed: under
 * the last of them, shaped like their rows, with a plus where their
 * picture stands. Under the last row rather than in a pulley, which is
 * where a reader who has just read the list is already looking.
 *
 * The words are handed in, so each stays in the catalog of the page
 * that says it.
 */
ListItem {
    id: row

    /// What the row adds, such as "Add profile".
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
