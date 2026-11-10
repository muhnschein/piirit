import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piirit 1.0

/*
 * What core says about a contact's or group's encryption, laid out to compare:
 * status, fingerprints in core's order, relays (domain only). Group: each member's
 * key, "Key missing" where core says so. Core's shape not known: its text as is.
 * From contact and group page: pulley, and tap on encryption line.
 */
Page {
    id: page

    property int accountId
    /// Contact for one-to-one; 0 for group.
    property int contactId: 0
    /// Group; used when `contactId` is 0.
    property int chatId: 0
    /// Contact's or group's name: header's second line.
    property string name
    property string errorMessage: ""

    readonly property bool forGroup: page.contactId === 0

    EncryptionInfo {
        id: info
        objectName: "info"
        account_id: page.accountId
        contact_id: page.contactId
        chat_id: page.contactId === 0 ? page.chatId : 0
        onError: page.errorMessage = message
    }

    Connections {
        target: core
        onStatus_changed: {
            if (core.status === "ready") {
                info.reload()
            }
        }
    }

    SilicaListView {
        id: list
        objectName: "keyList"
        anchors.fill: parent
        model: info.parsed ? info.keys : []

        /// Reader scrolled: header changes leave place alone.
        property bool moved: false
        onMovementStarted: list.moved = true

        // Qt 5.6: header growing (core's answer in) keeps contentY; with rows past screen,
        // header's top stays above it. Back to top once header has its height.
        Timer {
            id: toTop
            interval: 0
            onTriggered: list.positionViewAtBeginning()
        }

        PullDownMenu {
            objectName: "encryptionMenu"
            visible: info.parsed && info.keys.length > 0

            MenuItem {
                objectName: "copyFingerprints"
                //: Pulley on the encryption page: copies names and fingerprints.
                text: qsTr("Copy fingerprints")
                onClicked: {
                    Clipboard.text = info.copy_text
                    notice.show(qsTr("Copied to clipboard"))
                }
            }
        }

        header: Column {
            width: list.width
            spacing: Theme.paddingMedium

            onHeightChanged: {
                if (!list.moved) {
                    toTop.restart()
                }
            }

            PageHeader {
                //: Heading of the page with the keys' fingerprints. Delta
                //: Chat's "Encryption Info".
                title: qsTr("Encryption info")
                description: page.name
            }

            // Core's own sentence, in reader's language (stock strings).
            Label {
                objectName: "statusLabel"
                visible: info.parsed
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                color: Theme.highlightColor
                textFormat: Text.PlainText
                text: info.status
            }

            // Shape not known: core's text as is.
            Label {
                objectName: "rawText"
                visible: info.loaded && !info.parsed
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: info.raw
            }

            SectionHeader {
                visible: page.forGroup && info.parsed && info.keys.length > 0
                //: Over the members' keys on a group's encryption page. %n is how
                //: many are listed.
                text: qsTr("%n member(s)", "", info.keys.length)
            }
        }

        delegate: Column {
            objectName: "key" + index
            width: list.width
            spacing: Theme.paddingSmall

            // Contact: name as heading over fingerprint. "Me" is core's word.
            SectionHeader {
                visible: !page.forGroup
                text: modelData.name
            }

            // Group: member, relays, missing key.
            NameLabel {
                objectName: "memberName"
                visible: page.forGroup
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                truncationMode: TruncationMode.Fade
                name: modelData.name
            }
            Label {
                objectName: "memberRelays"
                visible: page.forGroup && modelData.relays.length > 0
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                truncationMode: TruncationMode.Fade
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryColor
                textFormat: Text.PlainText
                text: modelData.relays
            }
            Label {
                objectName: "keyMissing"
                visible: modelData.key_missing
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.errorColor
                //: On a group's encryption page, under a member whose key this
                //: phone does not have.
                text: qsTr("Key missing")
            }
            // As core groups it; fixed width, so columns line up.
            Label {
                objectName: "fingerprint"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                font.family: "monospace"
                textFormat: Text.PlainText
                text: modelData.fingerprint
            }

            Item {
                width: 1
                height: page.forGroup ? Theme.paddingLarge : 0
            }
        }

        // Contact's relays, from their key.
        footer: Column {
            width: list.width
            visible: !page.forGroup && info.relays.length > 0

            SectionHeader {
                //: Over the relays a contact's key names. Delta Chat's "Relays".
                text: qsTr("Relays")
            }
            Label {
                objectName: "relays"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: info.relays.split(", ").join("\n")
            }
        }

        VerticalScrollDecorator {}
    }

    BusyIndicator {
        anchors.centerIn: parent
        size: BusyIndicatorSize.Large
        running: !info.loaded
    }

    Banner {
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            bottom: notice.top
        }
        text: page.errorMessage
        timeout: 8
        onDismissed: page.errorMessage = ""
    }

    Banner {
        id: notice
        objectName: "notice"
        labelObjectName: "noticeLabel"
        tone: "info"
        timeout: 2
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        onDismissed: notice.text = ""
    }
}
