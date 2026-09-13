import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Add a profile: a name, and the chatmail relay it lives on. The relay
 * mints the address and credentials; the keys are made on this device
 * (docs/PROJECT.md).
 *
 * A dialog rather than a form with a button, the way Silica asks a
 * question: what was typed is on this page, and accepting it goes to
 * ProfileSetupPage, which does the work and shows the progress. The relay
 * list and the shape of the page follow parla's account dialog
 * (github.com/trufae/parla), whose curated list of public relays is
 * copied here; one of them is picked at random each time. Anyone can run
 * a relay, so a custom one can be typed, and takes over from the list
 * while it is.
 */
Dialog {
    id: dialog

    /// The relay chosen: the custom one if typed, else the picked one.
    property string domain: customField.text.trim().length > 0
                            ? customField.text.trim()
                            : (relayCombo.currentIndex >= 0 && relayCombo.currentIndex < relays.length
                               ? relays[relayCombo.currentIndex].domain : "")
    /// What the core is handed: `dcaccount:` and a relay, which it takes
    /// with or without the `https://.../new` around it.
    property string providerQr: "dcaccount:" + domain

    // From chatmail.at/relays, as parla curates it.
    readonly property var relays: [
        { domain: "nine.testrun.org", location: "Germany" },
        { domain: "mehl.cloud", location: "German" },
        { domain: "mailchat.pl", location: "Poland" },
        { domain: "chatmail.woodpeckersnest.space", location: "Italy" },
        { domain: "chatmail.culturanerd.it", location: "Italy" },
        { domain: "chat.adminforge.de", location: "Falkenstein, Germany" },
        { domain: "chika.aangat.lahat.computer", location: "Santa Clara, USA" },
        { domain: "tarpit.fun", location: "Nuremberg, Germany" },
        { domain: "d.gaufr.es", location: "Roubaix, France" },
        { domain: "chtml.ca", location: "Quebec, Canada" },
        { domain: "chatmail.au", location: "Melbourne, Australia" },
        { domain: "e2ee.wang", location: "Johannesburg, South Africa" },
        { domain: "chat.privittytech.com", location: "Bangalore, India" },
        { domain: "e2ee.im", location: "Orastie, Romania" },
        { domain: "chatmail.email", location: "Warsaw, Poland" },
        { domain: "danneskjold.de", location: "Helsinki, Finland" },
        { domain: "chat.in-the.eu", location: "Falkenstein, Germany" },
        { domain: "chat.nuvon.app", location: "Prague, Czechia" },
        { domain: "nibblehole.com", location: "Zug, Switzerland" },
        { domain: "chat.zashm.org", location: "Lviv, Ukraine" },
        { domain: "chat.sus.fr", location: "Iceland/Japan/Kenya/South Africa" },
        { domain: "delta.thelab.uno", location: "Gravelines, France" },
        { domain: "chat.vim.wtf", location: "Frankfurt, Germany" },
        { domain: "uninterest.ing", location: "Elk Grove Village, USA" },
        { domain: "sweetfern.net", location: "Ashburn, USA" },
        { domain: "delta.disobey.net", location: "Roon, Netherlands" }
    ]

    canAccept: nameField.text.trim().length > 0 && domain.length > 0

    // The setup page does the work, with what was typed here. Silica
    // makes that page as soon as this one is on screen, so what was
    // typed is handed over on accept rather than at its making.
    acceptDestination: Qt.resolvedUrl("ProfileSetupPage.qml")
    onAccepted: {
        dialog.acceptDestinationInstance.displayName = nameField.text.trim()
        dialog.acceptDestinationInstance.providerQr = dialog.providerQr
    }

    // Any relay, each time: no one of them is the default, so the
    // profiles made here spread over the list rather than all landing on
    // its first entry.
    Component.onCompleted: relayCombo.currentIndex = Math.floor(Math.random() * relays.length)

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
                text: qsTr("The relay gives you an address. The keys are made on this phone.")
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
                label: qsTr("Relay")
                currentIndex: 0
                // A typed server is the one that counts.
                enabled: customField.text.trim().length === 0

                menu: ContextMenu {
                    Repeater {
                        model: dialog.relays

                        MenuItem {
                            objectName: "relayOption" + index
                            text: modelData.domain + " (" + modelData.location + ")"
                        }
                    }
                }
            }

            TextField {
                id: customField
                objectName: "customField"
                width: parent.width
                label: qsTr("Custom server")
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
                text: qsTr("See <a href=\"https://chatmail.at/relays\">chatmail.at/relays</a> for the full list.")
                onLinkActivated: Qt.openUrlExternally(link)
            }
        }
    }
}
