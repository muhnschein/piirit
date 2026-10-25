import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * Every chat in common with a contact, from contact page's Show all. Its
 * list, not own: no second load, stays live by its events. Tap: as there.
 */
Page {
    id: page

    /// Contact page's ChatList.
    property QtObject chats
    /// Contact page: opens tapped chat.
    property Item contactPage

    SilicaListView {
        id: listView
        objectName: "commonChatsList"
        anchors.fill: parent
        clip: true
        model: page.chats ? page.chats.rows : null

        header: PageHeader {
            //: Over the list of all chats a contact is in too, opened from
            //: their page. Delta Chat's "Chats in Common".
            title: qsTr("Chats in common")
        }

        delegate: ListItem {
            objectName: "allCommonChatRow" + model.chat_id
            contentHeight: body.height

            ContactRow {
                id: body
                width: parent.width
                displayName: model.name
                ownColor: model.color
                picturePath: model.avatar_path
                isKeyContact: model.is_encrypted
            }

            onClicked: page.contactPage.openChat(model.chat_id, model.name)
        }

        VerticalScrollDecorator {}
    }
}
