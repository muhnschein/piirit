import QtQuick 2.0
import Sailfish.Silica 1.0
import Piirit 1.0
import "../components"

/*
 * One message, whole, with nothing else on the screen.
 *
 * A conversation is a column of bubbles, and a bubble is the wrong shape
 * for a page of prose: the row collapses a long body so the chat stays
 * scrollable, and the rest of it is here. A message the sending core cut
 * in two is here as well -- what the bubble holds is only its beginning,
 * and the whole of it comes from the core (`full_text.rs`).
 *
 * The reader's Markdown setting applies as it does in the conversation,
 * and for the same reason the body is pinned to plain text unless the
 * shim rendered it: see the note at the top of MessageDelegate.qml.
 *
 * A Label cannot be selected in, so "Select text" in the pull-down puts
 * the message as written in a field that can, and copies each selection
 * as it is made. The label stays until then because the formatting and
 * the links are its own, and a field draws neither.
 */
Page {
    id: page

    /// Which account the message belongs to.
    property int accountId: 0
    /// Which message to show whole.
    property int messageId: 0
    /// Who wrote it, for under the heading.
    property string senderName: ""
    /// 0 draws Markdown; anything else shows the message as written.
    property int markdownMode: 1

    /// What went wrong, when something did. Kept on the screen rather
    /// than shown and cleared: a page with nothing on it and no reason
    /// for it is the very thing this page exists to stop being.
    property string errorMessage: ""

    /// Whether the body is the selectable field rather than the label.
    property bool selecting: false

    allowedOrientations: Orientation.All

    FullText {
        id: whole
        objectName: "fullText"
        account_id: page.accountId
        message_id: page.messageId
        onError: page.errorMessage = message
    }

    /// Whether the body is drawn from the shim's rendering; see
    /// MessageDelegate.
    readonly property bool drawsStyled: page.markdownMode === 0
                                        && whole.styled_text.length > 0
    /// The body as the setting wants it shown.
    readonly property string shownText: page.drawsStyled
                                        ? whole.styled_text
                                        : whole.text

    /// Swap the label for the selectable field, focused and with all of
    /// it selected, so the handles are there to be moved.
    ///
    /// Selected from the end back to the start, which leaves the cursor
    /// at the start: a focused field keeps its cursor in view, and one at
    /// the end of a long message would scroll the page down to it.
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
                // Once the field is up there is nothing left to switch.
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
                    // The text as written, never the rendering: what is
                    // pasted elsewhere should be what was typed here.
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

            // Who wrote it. Its own label rather than the header's
            // `description`, which draws whatever it is given as markup:
            // see ConversationHeader.
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
                // Plain, unless the shim rendered it: see MessageDelegate.
                textFormat: page.drawsStyled ? Text.StyledText : Text.PlainText
                text: page.shownText
                onLinkActivated: Qt.openUrlExternally(link)
            }

            // The message as written, as Copy copies it, in a field whose
            // text can be selected. Silica's read-only field takes no
            // focus and cannot be selected in without it: `focusOnClick`
            // is its own gallery's "read only, but focusable". Plain text,
            // since a TextEdit reads no markup unless told to.
            TextArea {
                id: selectableBody
                objectName: "selectableBody"
                visible: page.selecting
                width: parent.width
                readOnly: true
                focusOnClick: true
                labelVisible: false
                color: Theme.primaryColor
                text: whole.text
                // Each selection goes on the clipboard as it is made, as
                // Silica does with one, and the notice says so.
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

    // Why there is nothing to read, or that the text was copied.
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

    // A failure stays until the page is left: it is the reason the page
    // is empty, and four seconds is not long enough to explain that.
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
