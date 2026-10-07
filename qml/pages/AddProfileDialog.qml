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
 * The list opens on "Automatic": the core makes the profile on whichever
 * of its relays answers first and adds more in the background, so that
 * one relay going down does not cut the profile off (init_transports,
 * core 2.61). That is not a relay picked for the reader by this dialog,
 * which an earlier version did, at random from the list: no relay is
 * named, and which ones the profile is on is the core's to decide and the
 * profile page's to show. A relay picked from the list or typed is that
 * relay alone, as before.
 */
Dialog {
    id: dialog

    /// The cover offers no quick actions while this is up: a profile is
    /// being made from it. See piirit.qml.
    readonly property bool pausesQuickActions: true

    /// The core picks the relays: nothing typed, and the first entry of
    /// the list, "Automatic", picked.
    readonly property bool automatic: customField.text.trim().length === 0
                                      && relayCombo.currentIndex === 0
    /// The relay chosen: the custom one if typed, else the picked one.
    /// The list's entries are one down from the menu's, which opens on
    /// "Automatic".
    property string domain: customField.text.trim().length > 0
                            ? customField.text.trim()
                            : (relayCombo.currentIndex >= 1 && relayCombo.currentIndex <= relays.length
                               ? relays[relayCombo.currentIndex - 1].domain : "")
    /// What the core is handed: `dcaccount:` and a relay, which it takes
    /// with or without the `https://.../new` around it. Empty for
    /// "Automatic", which the shim hands over as no relay at all.
    property string providerQr: domain.length > 0 ? "dcaccount:" + domain : ""

    // The public relays, as chatmail.at/relays lists them (Relays.js).
    readonly property var relays: Relays.list

    // A name, and relays: the core's, or one picked or typed.
    canAccept: nameField.text.trim().length > 0 && (automatic || domain.length > 0)

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
                text: qsTr("Choose a name and a relay. Nothing else is needed.")
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
                // "Automatic": several relays, the core's choice.
                currentIndex: 0
                // A typed server is the one that counts.
                enabled: customField.text.trim().length === 0

                menu: ContextMenu {
                    //: Delta Chat's word: the core picks the relays.
                    MenuItem {
                        objectName: "relayAutomatic"
                        text: qsTr("Automatic")
                    }

                    Repeater {
                        model: dialog.relays

                        MenuItem {
                            objectName: "relayOption" + index
                            text: Relays.label(modelData)
                        }
                    }
                }
            }

            // Why several, said while that is what Create will do. Delta
            // Chat's own sentence, for its translations.
            Label {
                objectName: "automaticHint"
                visible: dialog.automatic
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                text: qsTr("Relays are used for sending and receiving messages. Having more than one keeps your connection reliable.")
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
