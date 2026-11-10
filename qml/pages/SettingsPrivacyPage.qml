import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piirit 1.0

/*
 * What gives the reader away: link data sent, who is not heard from. From
 * SettingsPage. Block list is the chat list's profile's (core's list per
 * account); with several profiles the heading names it.
 */
Page {
    id: page

    /// Profile whose block list shows.
    property int accountId

    /// Block list answered: count shown only then, not a false 0.
    property bool blockListLoaded: false

    // Count for the row; BlockedContactsPage loads its own.
    ContactList {
        id: blockList
        account_id: page.accountId
        blocked: true
        // Emitted after rows set.
        onRows_changed: page.blockListLoaded = true
    }

    // Back from block list: count may have moved.
    onStatusChanged: {
        if (status === PageStatus.Active) {
            blockList.reload()
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: qsTr("Privacy and security")
            }

            TextSwitch {
                objectName: "cleanLinksSwitch"
                text: qsTr("Remove tracking from links")
                description: qsTr("Removes click IDs and campaign tags from links you send.")
                // Bound to setting: switch can't drift.
                automaticCheck: false
                checked: Settings.cleanLinks === true
                onClicked: Settings.cleanLinks = !checked
            }

            // Profile's own name, drawn plain: SectionHeader can't be told (as ConversationHeader).
            Repeater {
                id: profiles
                model: core.account_list

                Label {
                    objectName: "blockedProfileLabel"
                    visible: profiles.count > 1 && model.account_id === page.accountId
                    x: Theme.horizontalPageMargin
                    width: column.width - 2 * Theme.horizontalPageMargin
                    height: visible ? Theme.itemSizeExtraSmall : 0
                    verticalAlignment: Text.AlignVCenter
                    horizontalAlignment: Text.AlignRight
                    truncationMode: TruncationMode.Fade
                    font.pixelSize: Theme.fontSizeSmall
                    color: Theme.highlightColor
                    textFormat: Text.PlainText
                    //: Heading over the block list when there are several profiles. %1 is
                    //: the profile's name.
                    text: qsTr("Profile: %1").arg(model.display_name.length > 0
                                                  ? model.display_name : model.addr)
                }
            }

            ValueButton {
                objectName: "blockedContactsEntry"
                width: parent.width
                label: qsTr("Blocked contacts")
                value: page.blockListLoaded ? qsTr("%n contact(s)", "", blockList.count) : ""
                onClicked: pageStack.push(Qt.resolvedUrl("BlockedContactsPage.qml"), {
                    accountId: page.accountId
                })
            }
        }

        VerticalScrollDecorator {}
    }
}
