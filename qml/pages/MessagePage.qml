import QtQuick 2.0
import Sailfish.Silica 1.0
import Piirit 1.0
import "../components"
import "../js/Emoji.js" as Emoji

/*
 * Whole message: bubble collapses long bodies, sender-cut ones hold only a start.
 * Full text from core (`full_text.rs`); Markdown per setting (MessageDelegate.qml).
 * "Select text" swaps label (formatting, links) for selectable field.
 */
Page {
    id: page

    /// Owning account.
    property int accountId: 0
    /// Message to show.
    property int messageId: 0
    /// Author, under heading.
    property string senderName: ""
    /// 0 draws Markdown; else as written.
    property int markdownMode: 1

    /// Error, kept on screen: no unexplained blank page.
    property string errorMessage: ""

    /// Body is selectable field.
    property bool selecting: false

    allowedOrientations: Orientation.All

    FullText {
        id: whole
        objectName: "fullText"
        account_id: page.accountId
        message_id: page.messageId
        onError: page.errorMessage = message
    }

    /// Body from shim rendering; see MessageDelegate.
    readonly property bool drawsStyled: page.markdownMode === 0
                                        && whole.styled_text.length > 0
    /// Body as setting wants.
    readonly property string shownText: page.drawsStyled
                                        ? whole.styled_text
                                        : whole.text
    /// Same, emoji as pictures; "" if none.
    readonly property string emojiText: Emoji.inText(page.shownText, page.drawsStyled,
                                                     bodyLabel.font.pixelSize)

    /// Swap in selectable field, focused, all selected end-to-start (cursor at start, no scroll).
    function beginSelecting() {
        page.selecting = true
        selectableBody.forceActiveFocus()
        selectableBody.select(selectableBody.text.length, 0)
    }

    SilicaFlickable {
        id: flickable
        objectName: "messageFlickable"
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        PullDownMenu {
            MenuItem {
                objectName: "selectItem"
                // Field up: nothing to switch.
                visible: !page.selecting
                enabled: whole.text.length > 0
                //: Pull-down entry on the page showing one whole message:
                //: shows it as text that can be selected, and copied, a
                //: part at a time.
                text: qsTr("Select text")
                onClicked: page.beginSelecting()
            }
            MenuItem {
                objectName: "copyItem"
                enabled: whole.text.length > 0
                text: qsTr("Copy")
                onClicked: {
                    // As written, not rendered.
                    Clipboard.text = whole.text
                    notice.show(qsTr("Copied"))
                }
            }
        }

        VerticalScrollDecorator {}

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                objectName: "messageHeader"
                //: Heading of the page showing one message on its own.
                title: qsTr("Message")
            }

            // Own label: header `description` draws markup (see ConversationHeader).
            Label {
                objectName: "senderLabel"
                visible: page.senderName.length > 0
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                truncationMode: TruncationMode.Fade
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                textFormat: Text.PlainText
                text: page.senderName
            }

            BusyIndicator {
                objectName: "loadingIndicator"
                anchors.horizontalCenter: parent.horizontalCenter
                size: BusyIndicatorSize.Medium
                running: whole.loading
                visible: running
            }

            Label {
                id: bodyLabel
                objectName: "bodyLabel"
                visible: page.shownText.length > 0 && !page.selecting
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                color: Theme.primaryColor
                linkColor: Theme.highlightColor
                // Plain unless shim rendered it or emoji.
                textFormat: page.drawsStyled || page.emojiText.length > 0
                            ? Text.StyledText : Text.PlainText
                text: page.emojiText.length > 0 ? page.emojiText : page.shownText
                onLinkActivated: Qt.openUrlExternally(link)
            }

            // As written, as Copy copies. focusOnClick lets read-only field focus.
            TextArea {
                id: selectableBody
                objectName: "selectableBody"
                visible: page.selecting
                width: parent.width
                readOnly: true
                focusOnClick: true
                labelVisible: false
                color: Theme.primaryColor
                // Empty until shown: hidden field still lays out.
                text: page.selecting ? whole.text : ""
                // Each selection copied as made.
                onSelectedTextChanged: {
                    if (selectableBody.selectedText.length > 0) {
                        Clipboard.text = selectableBody.selectedText
                        notice.show(qsTr("Copied"))
                    }
                }
            }

            Label {
                objectName: "emptyLabel"
                visible: whole.loaded && page.shownText.length === 0
                         && page.errorMessage.length === 0
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryColor
                //: Shown on the full-message page for a message that
                //: turns out to have no words in it at all.
                text: qsTr("This message has no text")
            }
        }
    }

    // Why nothing to read, or copied.
    Banner {
        id: notice
        objectName: "notice"
        labelObjectName: "noticeLabel"
        tone: "info"
        timeout: 4
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        onDismissed: notice.text = ""
    }

    // Failure stays: 4 s toast can't explain empty page.
    Banner {
        objectName: "failure"
        labelObjectName: "errorLabel"
        tone: "error"
        timeout: 0
        text: page.errorMessage
        anchors {
            left: parent.left
            right: parent.right
            bottom: notice.top
        }
    }
}
