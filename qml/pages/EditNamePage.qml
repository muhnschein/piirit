import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * Nickname for a contact, from their page's pulley. Swipe back saves, once, if
 * changed: no Save, no "Saved". Blank goes back to their own name (core's rule).
 */
Page {
    id: page

    /// Contact's chat (ChatInfo) that renames.
    property QtObject chat
    property int contactId
    /// Name given here, empty if none.
    property string givenName
    /// Name they chose.
    property string ownName
    /// Name shown now: header's second line.
    property string displayName

    function save() {
        var name = field.text.trim()
        if (page.chat && page.contactId > 0 && name !== page.givenName) {
            page.chat.rename_contact(page.contactId, name)
        }
    }

    onStatusChanged: {
        if (status === PageStatus.Deactivating) {
            field.focus = false
            page.save()
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height

        Column {
            id: column
            width: parent.width

            PageHeader {
                //: Heading of the page that gives a contact a nickname.
                title: qsTr("Edit name")
                description: page.displayName
            }

            TextField {
                id: field
                objectName: "nameField"
                width: parent.width
                text: page.givenName
                placeholderText: page.ownName
                //: Label of the nickname field. %1 is the name the contact
                //: chose for themselves.
                label: qsTr("Nickname for \"%1\"").arg(page.ownName)
            }

            Label {
                objectName: "nameHint"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("Leave blank to use their own name. Only you see this name. Swipe back to save.")
            }
        }
    }
}
