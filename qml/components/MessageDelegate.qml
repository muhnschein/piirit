import QtQuick 2.0
import Sailfish.Silica 1.0
import "."
import "../js/Format.js" as Format
import "../js/Calls.js" as Calls
import "../js/Emoji.js" as Emoji

/*
 * One message: loads and measures alone, no page.
 *
 * Sender's strings pinned to PlainText (default goes rich: remote images on draw).
 * Deliberate exceptions, StyledText, never RichText (nothing can load): Markdown body
 * is shim's rendering (markdown.rs), every message char escaped, only shim's tags; body
 * or quote with emoji is escaped, only tag added an <img> of a shipped picture
 * (Emoji.js).
 *
 * Bindings, not Column: positioners size in a polish pass, which never runs headless.
 */
Item {
    id: root

    /// Attachment open asked: URL, not path (encoding done in preview).
    /// `previewWidth`: picture width here, so full view decodes the same.
    signal openRequested(url fileUrl, string fileName, string viewType,
                         real previewWidth)
    /// Read the message on its own page.
    signal fullTextRequested()
    /// Rest of a message the core holds only the header of.
    signal downloadRequested()
    /// Webxdc app tapped; the page runs it.
    signal appRequested()
    /// Reaction chip tapped: model decides add or remove.
    signal reactionRequested(string emoji)
    /// Call tapped: page decides ringing or call back.
    signal callRequested()
    /// Long press on a control: the row menu, anywhere on the message.
    signal menuRequested()

    /// One surface: tap opens, long press opens the menu. Attachments take no tap of their own.
    function tapped() {
        if (root.isCall) {
            root.callRequested()
            return
        }
        // Null for no file: nothing to open.
        var preview = attachment.item
        if (preview && preview.isApp) {
            // Apps run here; a .xdc handed to the system opens nothing.
            root.appRequested()
        } else if (preview && preview.openable) {
            root.openRequested(preview.fileUrl, root.fileName, root.viewType,
                               preview.contentWidth)
        } else if (root.canDownload) {
            root.downloadRequested()
        }
    }

    property string messageText: ""
    /// Reactions JSON from the shim: {emoji, count, self}, most frequent first.
    property string reactions: ""
    /// Parsed once per change, not per chip.
    readonly property var reactionList: root.reactions.length > 0
                                        ? JSON.parse(root.reactions) : []
    /// Shim StyledText rendering; empty: show as written.
    property string styledText: ""
    /// 0 draws Markdown; anything else shows as written.
    property int markdownMode: 1
    /// Sender edited the text: footer says so.
    property bool isEdited: false
    /// downloadState: Done, Available, InProgress, Failure, Undecipherable.
    /// Else (not Done, not empty): header only.
    property string downloadState: ""
    property bool isOutgoing: false
    property bool isInfo: false
    property bool isForwarded: false
    /// Search sent the reader here: flash it.
    property bool isFound: false
    property bool showPadlock: true
    // DC_STATE_*: 20 pending, 24 failed, 26 delivered, 28 read.
    property int deliveryState: 0
    // Unix seconds.
    property double sentAt: 0
    property string senderName: ""
    property string senderColor: ""
    // Only groups need to say who is speaking.
    property bool showSender: false
    property string quoteText: ""
    property string quoteAuthor: ""
    property string filePath: ""
    property string fileName: ""
    property string fileMime: ""
    // A real: QML has no 64-bit int for file size.
    property real fileBytes: 0
    /// Sound length in ms; 0 when the core does not say.
    property int durationMs: 0
    // Text, Image, Gif, Sticker, Audio, Voice, Video, File, Call, Webxdc, Vcard.
    property string viewType: "Text"
    property int imageWidth: 0
    property int imageHeight: 0
    /// Not seen before; see AttachmentPreview.
    property bool isNew: false
    /// `hasHtml`: text shown is partial; the rest is an HTML part only the core can fetch.
    property bool hasHtml: false
    // A shared contact, parsed by the core.
    property string vcardName: ""
    property string vcardAddr: ""
    property string vcardColor: ""
    // A webxdc app, read out of the app by the core.
    property string webxdcName: ""
    property string webxdcDocument: ""
    property string webxdcSummary: ""
    property string webxdcIcon: ""
    /// Webxdc apps on. Attachment decides app or file; `tapped` asks it, so one answer.
    property bool appsEnabled: false
    /// The list is being dragged or flicked; see AttachmentPreview.
    property bool scrolling: false
    /// Call state from the core (Alerting, Active, Completed, Missed, Declined, Canceled).
    /// Empty or unknown: the core's own sentence draws.
    property string callState: ""
    property bool callHasVideo: false
    /// Seconds, for a completed call.
    property int callDuration: 0

    /// This row is a call, drawn as one.
    readonly property bool isCall: root.viewType === "Call"
                                   && root.callState.length > 0
    /// Call row headline and sub-line; see Calls.js.
    readonly property string callTitle: Calls.title(root.callState, root.callHasVideo)
    readonly property string callDetail: Calls.detail(root.callState, root.callDuration,
                                                      root.isOutgoing)
    /// Missed, declined or canceled: drawn in the "bad" colour.
    readonly property bool callFailed: Calls.failed(root.callState)

    property bool hasFile: filePath.length > 0
    // Sticker: a picture with no bubble.
    readonly property bool isSticker: root.viewType === "Sticker" && root.hasFile

    /// Body from the shim rendering, if any. Raw text as StyledText is what pinning prevents.
    readonly property bool drawsStyled: root.markdownMode === 0
                                        && root.styledText.length > 0
    /// Body as the setting wants it shown.
    readonly property string shownText: root.drawsStyled
                                        ? root.styledText
                                        : root.messageText

    /// Body with emoji as pictures (EmojiGlyph), or "" to show as written.
    readonly property string emojiBody: !root.isCall
                                        ? Emoji.inText(root.shownText, root.drawsStyled,
                                                       messageLabel.font.pixelSize)
                                        : ""
    /// The body as drawn, and whether that is StyledText.
    readonly property string bodyText: root.emojiBody.length > 0
                                       ? root.emojiBody : root.shownText
    readonly property bool bodyStyled: root.drawsStyled || root.emojiBody.length > 0
    /// Quote: always as written.
    readonly property string emojiQuote: Emoji.inText(root.quoteText, false,
                                                      Theme.fontSizeExtraSmall)
    /// Header only, fetching, or failed: something to tap.
    readonly property bool heldBack: root.downloadState.length > 0
                                     && root.downloadState !== "Done"
    /// Download states that offer the rest.
    readonly property bool canDownload: root.downloadState === "Available"
                                        || root.downloadState === "Failure"

    /// Offer text, here: the bubble is sized from it.
    //: Opens the whole message on a page of its own.
    readonly property string fullText: qsTr("View full message")

    /// Offer width; none when absent.
    readonly property real actionsWidth: root.showsFull
                                         ? fullMetric.implicitWidth : 0

    /// Footer: edited, unencrypted, time, progress. Built once.
    readonly property string footerText: (root.isEdited ? qsTr("Edited") + " · " : "")
                                         + (root.showPadlock ? "" : "✉ ")
                                         + Qt.formatTime(new Date(root.sentAt * 1000), "hh:mm")
                                         + (root.isOutgoing
                                            ? " " + Format.stateMark(root.deliveryState) : "")

    /// Body lines shown before the page takes over (enough for a paragraph).
    ///
    /// Not lifted in place: an open body made the row taller than the view, unscrollable.
    property int collapsedLines: 12
    /// Offer for long bodies only; not captionless pictures, nor messages still held back.
    readonly property bool hasBody: root.messageText.length > 0 && !root.heldBack
                                    && !root.isCall
    /// Whether the core cut this message: the rest is in an HTML part, off this phone.
    ///
    /// `hasHtml` alone is not enough: core sets it whenever text isn't all that arrived
    /// (HTML alternative, HTML-only, stripped quote, forward), i.e. most mail. Only a real
    /// cut ends in core's `[...]` marker. Flag: HTML part holds the rest; marker: rest is
    /// missing, not a second rendering.
    readonly property bool wasCut: root.hasHtml
                                   && root.messageText.slice(-5) === "[...]"
    /// Offer the page: anything not shown whole, and every cut message.
    readonly property bool showsFull: root.hasBody
                                      && (root.wasCut || messageLabel.truncated)

    // Bubble as wide as content. Widths come off unconstrained copies (a real label would loop).
    property real maxWidth: root.width * 0.78 - 2 * Theme.paddingMedium
    /// Width everything but the offer wants, and what the body wraps at.
    ///
    /// Wrapping at a width the offer widened loops (fits, offer goes, bubble narrows, no
    /// longer fits); QML drops a binding, leaving width up to property arrival order. The
    /// offer never reaches this width; bubble's is this widened for it.
    property real bodyWidth: Math.min(
        root.maxWidth,
        Math.max(textMetric.implicitWidth,
                 attachmentMetric.implicitWidth,
                 root.isCall ? callRow.wantedWidth : 0,
                 reactionRow.wantedWidth,
                 footerMetric.implicitWidth,
                 root.attachmentFillsBubble ? root.maxWidth : 0,
                 Theme.itemSizeSmall))
    /// Attachment fills the bubble: the preview's `wantsFullWidth`, computed here (preview is null
    /// while loading, so asking it sized and decoded a picture twice). Keep the two in step;
    /// qml_bubble_width.rs checks.
    readonly property bool attachmentFillsBubble: root.hasFile
        && (root.viewType === "Image" || root.viewType === "Sticker"
            || root.viewType === "Gif" || root.viewType === "Video"
            || root.viewType === "Audio" || root.viewType === "Voice"
            || root.viewType === "Vcard"
            || (root.viewType === "Webxdc" && root.appsEnabled
                && root.webxdcName.length > 0))
    /// Bubble width: content, widened to hold the offer (short lines would hang it out).
    property real contentWidth: Math.min(root.maxWidth,
                                         Math.max(root.bodyWidth,
                                                  root.actionsWidth))

    // Chips hang below the bubble; the row makes room, or they draw over the next message.
    height: (root.isInfo ? infoLabel.height : bubble.height)
            + (reactionRow.shown ? reactionRow.height - root.chipOverlap : 0)
            + 2 * Theme.paddingSmall

    /// How far chips reach up the bottom edge: enough to read as hung on it, no more (footer
    /// time sits in that corner).
    readonly property real chipOverlap: Theme.paddingMedium - Theme.paddingSmall / 2

    // Next part goes right below the last one there; gap only when both are there.
    function below(previous, mine) {
        return previous.y + previous.height
               + (mine && previous.height > 0 ? Theme.paddingSmall : 0)
    }

    Text {
        id: textMetric
        visible: false
        font: messageLabel.font
        // Measured as it will be drawn: bold is wider than plain.
        textFormat: root.bodyStyled ? Text.StyledText : Text.PlainText
        // A call's text is not drawn, so it does not size the bubble.
        text: root.isCall ? "" : root.bodyText
        // Body lines unwrapped, as many as the label shows: the widest sets the body width.
        // Unwrapped, so a line wider than the bubble still widens it.
        maximumLineCount: root.collapsedLines
    }

    Text {
        id: attachmentMetric
        visible: false
        font.pixelSize: Theme.fontSizeMedium
        textFormat: Text.PlainText
        // Asked of the preview, not its label.
        text: attachment.item ? attachment.item.genericText : ""
    }

    // Offer measured unconstrained, or its label width would loop through the bubble.
    Text {
        id: fullMetric
        visible: false
        font.pixelSize: Theme.fontSizeSmall
        textFormat: Text.PlainText
        text: root.fullText
    }

    // Footer measured the same way: "Edited" can widen a one-word bubble.
    Text {
        id: footerMetric
        visible: false
        font.pixelSize: Theme.fontSizeExtraSmall
        textFormat: Text.PlainText
        text: root.footerText
    }

    // Chip counts end to end, for the strip width (an emoji square each).
    Text {
        id: reactionMetric
        visible: false
        font.pixelSize: Theme.fontSizeSmall
        textFormat: Text.PlainText
        text: {
            var parts = []
            for (var i = 0; i < root.reactionList.length; i++) {
                parts.push(root.chipCount(root.reactionList[i]))
            }
            return parts.join("")
        }
    }

    // One line of chip font: chip height; the emoji is a line high, picture or text.
    Text {
        id: chipLineMetric
        visible: false
        font.pixelSize: Theme.fontSizeSmall
        textFormat: Text.PlainText
        text: "0"
    }

    /// Count shown for more than one person only: "👍" alone, not "👍 1".
    function chipCount(reaction) {
        return reaction.count > 1 ? "" + reaction.count : ""
    }

    // A core notice, not something anyone typed: centred and unadorned.
    Label {
        id: infoLabel
        objectName: "infoLabel"
        // Optional parts size by their own reason, never `visible` (effective visibility goes false
        // for a whole covered page). Rows that measured it collapsed while away. See
        // qml_hidden_rows.rs.
        readonly property bool shown: root.isInfo
        visible: infoLabel.shown
        height: infoLabel.shown ? implicitHeight : 0
        anchors.centerIn: parent
        width: parent.width - 2 * Theme.horizontalPageMargin
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        textFormat: Text.PlainText
        text: root.messageText
    }

    Rectangle {
        id: bubble
        objectName: "bubble"
        readonly property bool shown: !root.isInfo
        visible: bubble.shown
        x: root.isOutgoing
           ? root.width - width - Theme.horizontalPageMargin
           : Theme.horizontalPageMargin
        width: root.contentWidth + 2 * Theme.paddingMedium
        height: bubble.shown ? footerLabel.y + footerLabel.height + Theme.paddingMedium : 0
        radius: Theme.paddingMedium
        // Found message lit, not outlined: a border would resize it and move the rows.
        color: root.isFound
               ? Theme.rgba(Theme.highlightColor, 0.5)
               // Sticker sits on the conversation, not in a bubble.
               : root.isSticker ? "transparent"
               : root.isOutgoing ? Theme.rgba(Theme.highlightBackgroundColor, 0.25)
                                 : Theme.rgba(Theme.secondaryColor, 0.12)
        Behavior on color { ColorAnimation { duration: 400 } }

        Label {
            id: senderLabel
            objectName: "senderLabel"
            readonly property bool shown: root.showSender && !root.isOutgoing
                                          && root.senderName.length > 0
            visible: senderLabel.shown
            height: senderLabel.shown ? implicitHeight : 0
            x: Theme.paddingMedium
            y: Theme.paddingMedium
            width: root.contentWidth
            truncationMode: TruncationMode.Fade
            font.pixelSize: Theme.fontSizeExtraSmall
            color: root.senderColor.length > 0 ? root.senderColor : Theme.highlightColor
            textFormat: Text.PlainText
            text: root.senderName
        }

        // Marked as the reference client does, above the quote (describes the whole message).
        // No PlainText pinning needed (not remote text), but kept out of messageText.
        Label {
            id: forwardedLabel
            objectName: "forwardedLabel"
            readonly property bool shown: root.isForwarded
            visible: forwardedLabel.shown
            height: forwardedLabel.shown ? implicitHeight : 0
            x: Theme.paddingMedium
            y: root.below(senderLabel, forwardedLabel.shown)
            width: root.contentWidth
            wrapMode: Text.Wrap
            font.pixelSize: Theme.fontSizeExtraSmall
            font.italic: true
            color: Theme.secondaryColor
            textFormat: Text.PlainText
            text: qsTr("Forwarded")
        }

        // The quoted message, marked off by a bar down its left.
        Item {
            id: quoteRow
            objectName: "quoteRow"
            readonly property bool shown: root.quoteText.length > 0
            visible: quoteRow.shown
            x: Theme.paddingMedium
            y: root.below(forwardedLabel, quoteRow.shown)
            width: root.contentWidth
            height: quoteRow.shown ? quoteLabel.y + quoteLabel.height : 0

            Rectangle {
                width: 2
                height: parent.height
                color: Theme.highlightColor
            }

            Label {
                id: quoteAuthorLabel
                x: 2 + Theme.paddingSmall
                width: parent.width - 2 - Theme.paddingSmall
                truncationMode: TruncationMode.Fade
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.highlightColor
                textFormat: Text.PlainText
                text: root.quoteAuthor
            }

            Label {
                id: quoteLabel
                objectName: "quoteLabel"
                x: quoteAuthorLabel.x
                y: quoteAuthorLabel.height
                width: quoteAuthorLabel.width
                maximumLineCount: 2
                wrapMode: Text.Wrap
                truncationMode: TruncationMode.Elide
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                // Plain, unless it has emoji to draw: see the top.
                textFormat: root.emojiQuote.length > 0 ? Text.StyledText : Text.PlainText
                text: root.emojiQuote.length > 0 ? root.emojiQuote : root.quoteText
            }
        }

        // Any attachment kind, drawn by the one component that knows the difference. Reports, never
        // acts. Built only for a message with a file, so text messages do not carry every renderer.
        Loader {
            id: attachment
            x: Theme.paddingMedium
            y: root.below(quoteRow, height > 0)
            active: root.hasFile
            sourceComponent: AttachmentPreview {
                objectName: "attachment"
                contentWidth: root.contentWidth
                filePath: root.filePath
                fileName: root.fileName
                fileMime: root.fileMime
                fileBytes: root.fileBytes
                durationMs: root.durationMs
                viewType: root.viewType
                imageWidth: root.imageWidth
                imageHeight: root.imageHeight
                isNew: root.isNew
                vcardName: root.vcardName
                vcardAddr: root.vcardAddr
                vcardColor: root.vcardColor
                webxdcName: root.webxdcName
                webxdcDocument: root.webxdcDocument
                webxdcSummary: root.webxdcSummary
                webxdcIcon: root.webxdcIcon
                appsEnabled: root.appsEnabled
                scrolling: root.scrolling
                // A long press on one of its own controls is the row's menu.
                onMenuRequested: root.menuRequested()
            }
        }

        // Call: kind, outcome and length, in place of the text.
        Item {
            id: callRow
            objectName: "callRow"
            readonly property bool shown: root.isCall
            /// Icon, gap and the wider of the two lines.
            readonly property real wantedWidth:
                callIcon.width + Theme.paddingMedium
                + Math.max(callTitleMetric.implicitWidth, callDetailMetric.implicitWidth)
            visible: callRow.shown
            x: Theme.paddingMedium
            y: root.below(attachment, callRow.shown)
            width: root.contentWidth
            height: callRow.shown
                    ? Math.max(callIcon.height, callTitleLabel.height + callDetailLabel.height)
                    : 0

            Text {
                id: callTitleMetric
                visible: false
                font.pixelSize: Theme.fontSizeMedium
                textFormat: Text.PlainText
                text: root.callTitle
            }

            Text {
                id: callDetailMetric
                visible: false
                font.pixelSize: Theme.fontSizeExtraSmall
                textFormat: Text.PlainText
                text: root.callDetail
            }

            Image {
                id: callIcon
                objectName: "callIcon"
                width: Theme.iconSizeMedium
                height: Theme.iconSizeMedium
                anchors.verticalCenter: parent.verticalCenter
                // Theme icons take their colour after the `?`.
                source: (root.callHasVideo ? "image://theme/icon-m-video?"
                                           : "image://theme/icon-m-call?")
                        + (root.callFailed ? Theme.errorColor : Theme.highlightColor)
            }

            Label {
                id: callTitleLabel
                objectName: "callTitle"
                x: callIcon.width + Theme.paddingMedium
                y: callDetailLabel.height > 0
                   ? Math.floor((callRow.height - height - callDetailLabel.height) / 2)
                   : Math.floor((callRow.height - height) / 2)
                width: callRow.width - x
                truncationMode: TruncationMode.Fade
                color: root.callFailed ? Theme.errorColor : Theme.primaryColor
                textFormat: Text.PlainText
                text: root.callTitle
            }

            Label {
                id: callDetailLabel
                objectName: "callDetail"
                x: callTitleLabel.x
                y: callTitleLabel.y + callTitleLabel.height
                width: callTitleLabel.width
                height: root.callDetail.length > 0 ? implicitHeight : 0
                truncationMode: TruncationMode.Fade
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                textFormat: Text.PlainText
                text: root.callDetail
            }
        }

        Label {
            id: messageLabel
            objectName: "messageLabel"
            readonly property bool shown: root.messageText.length > 0 && !root.isCall
            visible: messageLabel.shown
            height: messageLabel.shown ? implicitHeight : 0
            x: Theme.paddingMedium
            // Under the call if there is one, else the attachment; rows below chain from here.
            y: root.below(callRow.shown ? callRow : attachment, messageLabel.shown)
            // Body width, not the bubble's: they differ only when the offer widened it.
            width: root.bodyWidth
            wrapMode: Text.Wrap
            // Bubble is for a remark, not a document: a long body is cut to a few lines; the rest
            // is on a page (a row taller than the view cannot be scrolled past). Line cap, not
            // text cut: slicing a rendering could leave a tag open.
            maximumLineCount: root.collapsedLines
            truncationMode: TruncationMode.Elide
            color: Theme.primaryColor
            linkColor: Theme.highlightColor
            // Plain, unless the shim rendered it: see the note at the top.
            textFormat: root.bodyStyled ? Text.StyledText : Text.PlainText
            text: root.bodyText
            // A link is followed on a tap and on nothing else.
            onLinkActivated: Qt.openUrlExternally(link)
        }

        // Body that does not fit: two-word tap in the download offer's colour.
        Item {
            id: bodyActions
            objectName: "bodyActions"
            readonly property bool shown: root.showsFull
            visible: bodyActions.shown
            x: Theme.paddingMedium
            y: root.below(messageLabel, bodyActions.shown)
            width: root.contentWidth
            height: bodyActions.shown ? fullLabel.implicitHeight + Theme.paddingSmall : 0

            Label {
                id: fullLabel
                objectName: "fullButton"
                visible: root.showsFull
                // Far end of the row. Placed, not anchored.
                x: parent.width - width
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.highlightColor
                textFormat: Text.PlainText
                text: root.fullText

                MouseArea {
                    anchors.fill: parent
                    onClicked: root.fullTextRequested()
                    onPressAndHold: root.menuRequested()
                }
            }
        }

        // Held back by the download limit: header here; rest is asked for.
        Label {
            id: downloadLabel
            objectName: "downloadButton"
            readonly property bool shown: root.heldBack
            visible: downloadLabel.shown
            height: downloadLabel.shown ? implicitHeight + Theme.paddingSmall : 0
            x: Theme.paddingMedium
            y: root.below(bodyActions, downloadLabel.shown)
            width: root.contentWidth
            wrapMode: Text.Wrap
            font.pixelSize: Theme.fontSizeSmall
            color: Theme.highlightColor
            // Translated literals, chosen by the core's state.
            textFormat: Text.PlainText
            text: root.downloadState === "InProgress"
                  ? qsTr("Downloading…")
                  : root.downloadState === "Failure"
                    ? qsTr("⬇ Download failed, tap to try again")
                    : root.downloadState === "Undecipherable"
                      ? qsTr("Cannot be decrypted")
                      //: Fetches a message the auto-download limit held back.
                      : qsTr("⬇ Download")

            MouseArea {
                anchors.fill: parent
                enabled: root.canDownload
                onClicked: root.downloadRequested()
                onPressAndHold: root.menuRequested()
            }
        }

        // Time; own messages: progress. Mail icon: not encrypted. "Edited": changed later.
        Label {
            id: footerLabel
            objectName: "footerLabel"
            x: Theme.paddingMedium
            y: root.below(downloadLabel, true)
            width: root.contentWidth
            horizontalAlignment: Text.AlignRight
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryColor
            textFormat: Text.PlainText
            text: root.footerText
        }
    }

    // One chip per emoji, ours lit, hung off the bubble's inside corner, mostly below its edge
    // so the time there stays readable. Hand-laid: each chip reads the widths before it. One line,
    // clipped: the last of many reactions can go unseen.
    Item {
        id: reactionRow
        objectName: "reactionRow"
        readonly property bool shown: !root.isInfo && root.reactionList.length > 0
        visible: reactionRow.shown
        // Inside bubble padding, at the corner nearest screen middle.
        x: root.isOutgoing ? bubble.x + Theme.paddingMedium
                           : bubble.x + bubble.width - width - Theme.paddingMedium
        y: bubble.height - root.chipOverlap
        // Never wider than the bubble.
        width: Math.min(wantedWidth, root.contentWidth)
        // A line of the chip font plus the chip's own padding.
        height: reactionRow.shown ? chipLineMetric.height + 2 * Theme.paddingSmall : 0
        clip: true

        /// How big an emoji is drawn on a chip: a line of its text.
        readonly property real glyphSize: chipLineMetric.height

        /// Room the chips take in a row: emoji, counts, gaps and padding.
        readonly property real wantedWidth: {
            var n = root.reactionList.length
            if (n === 0) {
                return 0
            }
            var counted = 0
            for (var i = 0; i < n; i++) {
                if (root.chipCount(root.reactionList[i]).length > 0) {
                    counted++
                }
            }
            return n * reactionRow.glyphSize + reactionMetric.implicitWidth
                   + counted * Theme.paddingSmall
                   + n * 2 * Theme.paddingMedium
                   + (n - 1) * Theme.paddingSmall
        }

        Repeater {
            id: reactionRepeater
            objectName: "reactionRepeater"
            model: root.reactionList

            Rectangle {
                objectName: "reactionChip"
                /// Whether this is the reader's own reaction.
                readonly property bool mine: modelData.self === true
                /// The emoji, for the tap to name.
                readonly property string emoji: modelData.emoji
                // After the chips before it: the repeater builds in order.
                x: {
                    var at = 0
                    for (var i = 0; i < index; i++) {
                        var earlier = reactionRepeater.itemAt(i)
                        if (earlier) {
                            at += earlier.width + Theme.paddingSmall
                        }
                    }
                    return at
                }
                width: chipEmoji.width
                       + (chipCountLabel.text.length > 0
                          ? Theme.paddingSmall + chipCountLabel.implicitWidth : 0)
                       + 2 * Theme.paddingMedium
                height: reactionRow.height
                radius: height / 2
                // Nearly solid: a chip straddles the bubble edge; translucent would change colour
                // mid-way.
                color: mine ? Theme.rgba(Theme.highlightBackgroundColor, 0.9)
                            : Theme.rgba(Theme.highlightDimmerColor, 0.9)

                // Picture where shipped (Sailfish draws few emoji). Other end's text is not
                // checked; EmojiGlyph draws non-emoji as plain text.
                EmojiGlyph {
                    id: chipEmoji
                    objectName: "chipEmoji"
                    x: Theme.paddingMedium
                    anchors.verticalCenter: parent.verticalCenter
                    size: reactionRow.glyphSize
                    emoji: modelData.emoji
                }

                Label {
                    id: chipCountLabel
                    objectName: "chipCount"
                    x: chipEmoji.x + chipEmoji.width + Theme.paddingSmall
                    anchors.verticalCenter: parent.verticalCenter
                    font.pixelSize: Theme.fontSizeSmall
                    color: Theme.primaryColor
                    textFormat: Text.PlainText
                    text: root.chipCount(modelData)
                }

                MouseArea {
                    anchors.fill: parent
                    onClicked: root.reactionRequested(emoji)
                    onPressAndHold: root.menuRequested()
                }
            }
        }
    }
}
