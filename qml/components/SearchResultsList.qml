import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Chats, contacts, messages under count headings. Flat `kind` model for
 * sections (nested flickables fight drag). Own file for tests/qml_syntax.rs.
 */
SilicaListView {
    id: root

    /// Source SearchResults object.
    property QtObject search

    /// `messageId` 0 = chat; else open at that message.
    signal chatActivated(int chatId, string chatName, int messageId)
    /// Contact tapped; carries name as chat may not exist.
    signal contactActivated(int contactId, string contactName)

    // Else delegates draw outside box.
    clip: true
    model: root.search ? root.search.rows : null

    section {
        property: "kind"
        delegate: SectionHeader {
            objectName: "searchSection"
            text: section === "chat"
                  ? qsTr("Chats (%1)").arg(root.search.chat_count)
                  : section === "contact"
                    ? qsTr("Contacts (%1)").arg(root.search.contact_count)
                    // Big piles listed in part: show listed of matched.
                    : root.search.message_total > root.search.message_count
                      ? qsTr("Messages (%1 of %2)").arg(root.search.message_count)
                                                   .arg(root.search.message_total)
                      : qsTr("Messages (%1)").arg(root.search.message_count)
        }
    }

    delegate: ListItem {
        contentHeight: resultBody.height

        SearchResultRow {
            id: resultBody
            width: parent.width
            title: model.title
            subtitle: model.subtitle
            ownColor: model.color
            picturePath: model.avatar_path
            timestamp: model.timestamp
        }

        onClicked: {
            if (model.kind === "contact") {
                root.contactActivated(model.contact_id, model.title)
            } else {
                // Message result opens its chat at the match.
                root.chatActivated(model.chat_id, model.title,
                                   model.kind === "message" ? model.message_id : 0)
            }
        }
    }

    ViewPlaceholder {
        objectName: "noResultsPlaceholder"
        // `loaded`: hide between keystroke and answer.
        enabled: root.search && root.search.loaded && root.search.count === 0
        text: qsTr("Nothing found")
    }
}
