import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Group member row: tap opens them (MemberOpener), hold for Send message and,
 * where reader may, Remove from group (`removeAsked`: wait and countdown are the
 * page's, rows die on reload). Reader's own row: "You", no menu.
 */
ListItem {
    id: row

    property int contactId
    property string displayName
    property string ownColor
    property string picturePath
    property bool isKeyContact: true
    property bool isSelf: false
    /// Search text, marked in name.
    property string match
    /// Remove offered.
    property bool canRemove: false
    /// Waiting to be removed: faded, untappable.
    property bool doomed: false
    property QtObject opener
    /// Page's countdown on this row, once raised.
    property Item remorse: null
    /// What the countdown covers.
    readonly property Item body: body

    signal removeAsked()

    contentHeight: body.height

    onClicked: {
        if (!row.isSelf) {
            row.opener.open(row, row.contactId, row.displayName)
        }
    }

    // Built on first open (see ConversationList). None for reader.
    menu: row.isSelf ? null : memberMenu

    Component {
        id: memberMenu
        ContextMenu {
            MenuItem {
                objectName: "sendMessage"
                //: Menu on a group member: opens the chat with them, made if
                //: needed. Delta Chat's "Send Message".
                text: qsTr("Send message")
                onClicked: row.opener.sendMessage(row.contactId, row.displayName)
            }
            MenuItem {
                objectName: "removeItem"
                visible: row.canRemove
                text: qsTr("Remove from group")
                onClicked: row.removeAsked()
            }
        }
    }

    ContactRow {
        id: body
        // Remorse fades via own `opacity: 0.0`.
        enabled: !row.doomed
        width: parent.width
        displayName: row.displayName
        ownColor: row.ownColor
        picturePath: row.picturePath
        isKeyContact: row.isKeyContact
        match: row.match
        //: Under the reader's own name in a group's member list.
        address: row.isSelf ? qsTr("You") : ""
        showAddress: row.isSelf
    }
}
