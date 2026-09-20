import QtQuick 2.0
import Sailfish.Silica 1.0

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
 * from the list while it is.
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

    // The public relays chatmail.at/relays listed on 2026-09-20, in its
    // order, with what it says about each. Anyone may run a relay and
    // the page is the list that is kept up to date, so this one is a
    // starting point rather than the whole of it -- the hint under the
    // field points at the page itself.
    readonly property var relays: [
        { domain: "nine.testrun.org", location: "Default for many chatmail apps" },
        { domain: "mehl.cloud", location: "German speakers" },
        { domain: "mailchat.pl", location: "Polish speakers" },
        { domain: "chatmail.woodpeckersnest.space", location: "Italian speakers" },
        { domain: "chatmail.culturanerd.it", location: "Italian speakers" },
        { domain: "chat.adminforge.de", location: "Falkenstein, Germany" },
        { domain: "chika.aangat.lahat.computer", location: "Santa Clara, USA" },
        { domain: "tarpit.fun", location: "Nuremberg, Germany" },
        { domain: "d.gaufr.es", location: "Roubaix, France" },
        { domain: "chtml.ca", location: "Quebec, Canada" },
        { domain: "e2ee.wang", location: "Johannesburg, South Africa" },
        { domain: "chat.privittytech.com", location: "Bangalore, India" },
        { domain: "e2ee.im", location: "Orastie, Romania" },
        { domain: "chatmail.email", location: "Warsaw, Poland" },
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
