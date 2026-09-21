import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Relays.js" as Relays

/*
 * Add a profile: a name, and the chatmail relay it lives on. The relay
 * mints the address and credentials; the keys are made on this device
 * (docs/PROJECT.md).
 *
 * A dialog rather than a form with a button, the way Silica asks a
 * question: what was typed is on this page, and accepting it goes to
 * ProfileSetupPage, which does the work and shows the progress. The shape
 * of the page follows parla's account dialog (github.com/trufae/parla).
 * Anyone can run a relay, so a custom one can be typed, and takes over
 * from the list while it is. The list itself is shared with the page
 * that adds a relay to a profile (Relays.js).
 *
 * Nothing is picked to begin with, and the dialog cannot be accepted
 * until something is: a relay is where someone's address and their
 * mailbox live for as long as they keep the profile, so it is a choice
 * to make rather than one to be handed. An earlier version opened on a
 * relay of the list at random, which is a choice made for the reader by
 * a dialog they have not read yet.
 */
Dialog {
    id: dialog

    /// The relay chosen: the custom one if typed, else the picked one.
    property string domain: customField.text.trim().length > 0
                            ? customField.text.trim()
                            : (relayCombo.currentIndex >= 0 && relayCombo.currentIndex < relays.length
                               ? relays[relayCombo.currentIndex].domain : "")
    /// What the core is handed: `dcaccount:` and a relay, which it takes
    /// with or without the `https://.../new` around it. Empty until a
    /// relay is chosen: there is nothing to hand over before that.
    property string providerQr: domain.length > 0 ? "dcaccount:" + domain : ""

    // The public relays, as chatmail.at/relays lists them (Relays.js).
    readonly property var relays: Relays.list

    // A name, and a relay picked or typed: neither is guessed for the
    // reader.
    canAccept: nameField.text.trim().length > 0 && domain.length > 0

    // The setup page does the work, with what was typed here. Silica
    // makes that page as soon as this one is on screen, so what was
    // typed is handed over on accept rather than at its making.
    acceptDestination: Qt.resolvedUrl("ProfileSetupPage.qml")
    onAccepted: {
        dialog.acceptDestinationInstance.displayName = nameField.text.trim()
        dialog.acceptDestinationInstance.providerQr = dialog.providerQr
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height

        Column {
            id: column
            width: dialog.width
            spacing: Theme.paddingLarge

            DialogHeader {
                title: qsTr("Add profile")
                acceptText: qsTr("Create")
            }

            Label {
                objectName: "intro"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("Pick a name and a relay. There is nothing else to set up.")
            }

            TextField {
                id: nameField
                objectName: "nameField"
                width: parent.width
                label: qsTr("Your name")
                placeholderText: label
            }

            ComboBox {
                id: relayCombo
                objectName: "relayCombo"
                width: parent.width
                // Said as the action it is: with nothing picked, a bare
                // "Relay" above an empty value read as a line of text
                // rather than as a list to open.
                label: qsTr("Select a public chatmail relay")
                // Nothing to begin with: the reader picks. The label
                // above the empty value says what is being asked for,
                // and Create stays dim until it is answered, so the
                // dialog asks rather than answers for them.
                currentIndex: -1
                // A typed server is the one that counts.
                enabled: customField.text.trim().length === 0

                menu: ContextMenu {
                    Repeater {
                        model: dialog.relays

                        MenuItem {
                            objectName: "relayOption" + index
                            text: Relays.label(modelData)
                        }
                    }
                }
            }

            TextField {
                id: customField
                objectName: "customField"
                width: parent.width
                label: qsTr("Use a custom chatmail relay")
                placeholderText: label
                inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText | Qt.ImhUrlCharactersOnly
            }

            Label {
                objectName: "relaysHint"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                linkColor: Theme.highlightColor
                textFormat: Text.StyledText
                // What a relay is, for a reader who has an e-mail
                // account and wonders whether it will do: it will not,
                // and the page that explains why is the one to point at.
                text: qsTr("Piirit works only with chatmail relays. These are a particular kind of e-mail server; ordinary e-mail servers are not supported. For more, see <a href=\"https://chatmail.at\">chatmail.at</a>. A full list of public, free-to-use chatmail relays is at <a href=\"https://chatmail.at/relays\">chatmail.at/relays</a>.")
                onLinkActivated: Qt.openUrlExternally(link)
            }
        }
    }
}
