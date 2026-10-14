import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Relays.js" as Relays

/*
 * Add profile: name only unless asked. Relay mints address and credentials;
 * keys made on device (docs/PROJECT.md).
 *
 * Accept hands off to ProfileSetupPage. Shape from parla (github.com/trufae/parla).
 *
 * Core uses first answering relay, adds more later: one relay down does not
 * cut profile off (init_transports, core 2.61).
 * "Advanced" (shut by default) holds relay choice: Relays.js list or "Other
 * relay" field. Relay chosen there is used alone; header still names it when
 * shut.
 */
Dialog {
    id: dialog

    /// No quick actions while making profile. See piirit.qml.
    readonly property bool pausesQuickActions: true

    /// Menu: "Automatic", "Other relay", then list.
    readonly property int otherIndex: 1
    readonly property int firstRelayIndex: 2

    /// "Advanced" open.
    property bool advanced: false
    /// First entry, "Automatic", is core's pick.
    readonly property bool automatic: relayCombo.currentIndex === 0
    /// Typed for "Other relay", else picked; empty for "Automatic".
    property string domain: relayCombo.currentIndex === otherIndex
                            ? customField.text.trim()
                            : (relayCombo.currentIndex >= firstRelayIndex
                               && relayCombo.currentIndex - firstRelayIndex < relays.length
                               ? relays[relayCombo.currentIndex - firstRelayIndex].domain : "")
    /// Core gets `dcaccount:` + relay (takes it with or without
    /// `https://.../new`). Empty for "Automatic": shim passes no relay.
    property string providerQr: domain.length > 0 ? "dcaccount:" + domain : ""

    // Relays.js (chatmail.at/relays).
    readonly property var relays: Relays.list

    // Name and relays: core's, or picked/typed.
    canAccept: nameField.text.trim().length > 0 && (automatic || domain.length > 0)

    // Setup page does the work. Silica builds it as soon as this dialog shows,
    // so typed values handed over on accept, not at creation.
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

            // Silica-style header; chosen relay named while shut.
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
                    // "Automatic": several relays, core's choice.
                    currentIndex: 0
                    // Automatic: explain why several relays; else says one only.
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
                    // Why e-mail won't do: see explainer page.
                    text: qsTr("Piirit works only with chatmail relays. These are a particular kind of e-mail server; ordinary e-mail servers are not supported. For more, see <a href=\"https://chatmail.at\">chatmail.at</a>. A full list of public, free-to-use chatmail relays is at <a href=\"https://chatmail.at/relays\">chatmail.at/relays</a>.")
                    onLinkActivated: Qt.openUrlExternally(link)
                }
            }
        }
    }
}
