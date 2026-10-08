import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Relays.js" as Relays

/*
 * Add a profile: a name, and nothing else unless the reader asks for it.
 * The relay mints the address and credentials; the keys are made on this
 * device (docs/PROJECT.md).
 *
 * A dialog rather than a form with a button, the way Silica asks a
 * question: what was typed is on this page, and accepting it goes to
 * ProfileSetupPage, which does the work and shows the progress. The shape
 * of the page follows parla's account dialog (github.com/trufae/parla).
 *
 * The relays are the core's to pick: it makes the profile on whichever of
 * its relays answers first and adds more in the background, so that one
 * relay going down does not cut the profile off (init_transports, core
 * 2.61). That is the whole of the page as it opens, a name and Create.
 * Choosing a relay is under "Advanced", shut until it is opened: the
 * list (Relays.js, shared with the page that adds a relay to a profile),
 * and "Other relay", which brings up a field for one that is not on it,
 * since anyone can run a relay. A relay chosen there is that relay alone,
 * and the section says so. Shut again, it still names the relay, so a
 * choice made there is never one the page does not show.
 */
Dialog {
    id: dialog

    /// The cover offers no quick actions while this is up: a profile is
    /// being made from it. See piirit.qml.
    readonly property bool pausesQuickActions: true

    /// The menu's entries: "Automatic", "Other relay", then the list.
    readonly property int otherIndex: 1
    readonly property int firstRelayIndex: 2

    /// "Advanced" is open.
    property bool advanced: false
    /// The core picks the relays: the first entry, "Automatic", picked.
    readonly property bool automatic: relayCombo.currentIndex === 0
    /// The relay chosen: the one typed for "Other relay", else the one
    /// picked from the list; empty for "Automatic".
    property string domain: relayCombo.currentIndex === otherIndex
                            ? customField.text.trim()
                            : (relayCombo.currentIndex >= firstRelayIndex
                               && relayCombo.currentIndex - firstRelayIndex < relays.length
                               ? relays[relayCombo.currentIndex - firstRelayIndex].domain : "")
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
                text: qsTr("Choose a name. Nothing else is needed.")
            }

            TextField {
                id: nameField
                objectName: "nameField"
                width: parent.width
                label: qsTr("Your name")
                placeholderText: label
            }

            // The section's header, drawn the way Silica draws one that
            // opens: its title on the right, an arrow saying which way it
            // goes. A relay chosen inside is named under the title, so a
            // shut section still shows what Create will do.
            BackgroundItem {
                objectName: "advancedToggle"
                width: parent.width
                height: Math.max(Theme.itemSizeSmall, advancedTitle.height + 2 * Theme.paddingMedium)
                onClicked: dialog.advanced = !dialog.advanced

                Column {
                    id: advancedTitle
                    anchors {
                        right: advancedArrow.left
                        rightMargin: Theme.paddingMedium
                        verticalCenter: parent.verticalCenter
                    }
                    width: parent.width - advancedArrow.width - Theme.horizontalPageMargin
                           - Theme.paddingMedium - Theme.horizontalPageMargin

                    Label {
                        width: parent.width
                        horizontalAlignment: Text.AlignRight
                        truncationMode: TruncationMode.Fade
                        color: Theme.highlightColor
                        //: The section of the add-profile dialog that holds
                        //: the choice of relay, shut until it is opened.
                        text: qsTr("Advanced")
                    }

                    Label {
                        objectName: "advancedSummary"
                        visible: !dialog.automatic && dialog.domain.length > 0
                        width: parent.width
                        horizontalAlignment: Text.AlignRight
                        truncationMode: TruncationMode.Fade
                        font.pixelSize: Theme.fontSizeExtraSmall
                        color: Theme.secondaryHighlightColor
                        textFormat: Text.PlainText
                        //: Under "Advanced" in the add-profile dialog: the
                        //: one relay the profile will be made on.
                        text: qsTr("Relay: %1").arg(dialog.domain)
                    }
                }

                Image {
                    id: advancedArrow
                    anchors {
                        right: parent.right
                        rightMargin: Theme.horizontalPageMargin
                        verticalCenter: parent.verticalCenter
                    }
                    source: "image://theme/icon-m-down"
                    rotation: dialog.advanced ? 180 : 0
                }
            }

            Column {
                objectName: "advancedSection"
                visible: dialog.advanced
                width: parent.width
                spacing: Theme.paddingLarge

                ComboBox {
                    id: relayCombo
                    objectName: "relayCombo"
                    width: parent.width
                    //: The relays the profile is made on: the core's
                    //: ("Automatic"), one from the list, or another.
                    label: qsTr("Relay")
                    // "Automatic": several relays, the core's choice.
                    currentIndex: 0
                    // What the choice means, under it: why several, while
                    // that is what Create will do; one alone otherwise.
                    description: dialog.automatic
                                 ? qsTr("Relays are used for sending and receiving messages. Having more than one keeps your connection reliable.")
                                 : qsTr("Only this relay. More can be added later on the profile page.")

                    menu: ContextMenu {
                        //: Delta Chat's word: the core picks the relays.
                        MenuItem {
                            objectName: "relayAutomatic"
                            text: qsTr("Automatic")
                        }

                        MenuItem {
                            objectName: "relayOther"
                            //: A relay that is not on the list, typed in.
                            text: qsTr("Other relay")
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

                TextField {
                    id: customField
                    objectName: "customField"
                    visible: relayCombo.currentIndex === dialog.otherIndex
                    width: parent.width
                    label: qsTr("Relay address")
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
}
