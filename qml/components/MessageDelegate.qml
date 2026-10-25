import QtQuick 2.0
import Sailfish.Silica 1.0
import "."
import "../js/Format.js" as Format
import "../js/Calls.js" as Calls
import "../js/Emoji.js" as Emoji

/*
 * One message: loads and measures alone, no page.
 *
 * Sender's strings pinned PlainText (default goes rich: remote images on draw). Exceptions,
 * StyledText never RichText (nothing loads): Markdown body is shim's rendering (markdown.rs),
 * message chars escaped, only shim's tags; emoji body or quote escaped, only added tag an
 * <img> of shipped picture (Emoji.js).
 *
 * Bindings, not Column: positioners size in polish pass, never run headless.
 */
Item {
    id: root

    /// Attachment open: URL, not path (preview encodes). `previewWidth`: picture width here, so
    /// full view decodes same.
    signal openRequested(url fileUrl, string fileName, string viewType,
                         real previewWidth)
    /// Read message on own page.
    signal fullTextRequested()
    /// Fetch rest of header-only message.
    signal downloadRequested()
    /// Webxdc app tapped; page runs it.
    signal appRequested()
    /// Reaction chip tapped: model decides add or remove.
    signal reactionRequested(string emoji)
    /// Call tapped: page decides ring or call back.
    signal callRequested()
    /// Long press on a control: row menu.
    signal menuRequested()

    /// One surface: tap opens, long press menu. Attachments take no tap of own.
    function tapped() {
        if (root.isCall) {
            root.callRequested()
            return
        }
        // Null for no file: nothing to open.
        var preview = attachment.item
        if (preview && preview.isApp) {
            // Apps run here; system opens nothing for .xdc.
            root.appRequested()
        } else if (preview && preview.openable) {
            root.openRequested(preview.fileUrl, root.fileName, root.viewType,
                               preview.contentWidth)
        } else if (root.canDownload) {
            root.downloadRequested()
        }
    }

    property string messageText: ""
    /// Shim's reactions JSON: {emoji, count, self}, most frequent first.
    property string reactions: ""
    /// Parsed once per change, not per chip.
    readonly property var reactionList: root.reactions.length > 0
                                        ? JSON.parse(root.reactions) : []
    /// Shim StyledText rendering; empty: as written.
    property string styledText: ""
    /// 0 draws Markdown; else as written.
    property int markdownMode: 1
    /// Sender edited: footer says so.
    property bool isEdited: false
    /// downloadState: Done, Available, InProgress, Failure, Undecipherable. Not Done, not empty:
    /// header only.
    property string downloadState: ""
    property bool isOutgoing: false
    property bool isInfo: false
    property bool isForwarded: false
    /// Search led here: flash.
    property bool isFound: false
    property bool showPadlock: true
    // DC_STATE_*: 20 pending, 24 failed, 26 delivered, 28 read.
    property int deliveryState: 0
    // Unix seconds.
    property double sentAt: 0
    property string senderName: ""
    property string senderColor: ""
    // Only groups name speaker.
    property bool showSender: false
    property string quoteText: ""
    property string quoteAuthor: ""
    property string filePath: ""
    property string fileName: ""
    property string fileMime: ""
    // Real: QML has no 64-bit int.
    property real fileBytes: 0
    /// Sound ms; 0 if core silent.
    property int durationMs: 0
    // Text, Image, Gif, Sticker, Audio, Voice, Video, File, Call, Webxdc, Vcard.
    property string viewType: "Text"
    property int imageWidth: 0
    property int imageHeight: 0
    /// Not seen before; see AttachmentPreview.
    property bool isNew: false
    /// `hasHtml`: text partial; rest is HTML part only core fetches.
    property bool hasHtml: false
    // Shared contact, parsed by core.
    property string vcardName: ""
    property string vcardAddr: ""
    property string vcardColor: ""
    // Webxdc app, read out by core.
    property string webxdcName: ""
    property string webxdcDocument: ""
    property string webxdcSummary: ""
    property string webxdcIcon: ""
    /// Webxdc on. Attachment decides app or file; `tapped` asks it: one answer.
    property bool appsEnabled: false
    /// List dragged/flicked; see AttachmentPreview.
    property bool scrolling: false
    /// Core call state (Alerting, Active, Completed, Missed, Declined, Canceled). Empty/unknown:
    /// core's sentence.
    property string callState: ""
    property bool callHasVideo: false
    /// Seconds, completed call.
    property int callDuration: 0

    /// Row is a call.
    readonly property bool isCall: root.viewType === "Call"
                                   && root.callState.length > 0
    /// Call headline, sub-line; see Calls.js.
    readonly property string callTitle: Calls.title(root.callState, root.callHasVideo)
    readonly property string callDetail: Calls.detail(root.callState, root.callDuration,
                                                      root.isOutgoing)
    /// Missed, declined, canceled: "bad" colour.
    readonly property bool callFailed: Calls.failed(root.callState)

    property bool hasFile: filePath.length > 0
    // Sticker: picture, no bubble.
    readonly property bool isSticker: root.viewType === "Sticker" && root.hasFile

    /// Shim's rendering, if any. Raw text as StyledText is what pinning prevents.
    readonly property bool drawsStyled: root.markdownMode === 0
                                        && root.styledText.length > 0
    /// Body per setting.
    readonly property string shownText: root.drawsStyled
                                        ? root.styledText
                                        : root.messageText

    /// Few emoji, nothing else: drawn twice text size. Not a file's caption.
    readonly property bool bigEmoji: !root.hasFile && Emoji.onlyEmoji(root.messageText)
    /// Body with emoji pictures (EmojiGlyph), or "" for as written.
    readonly property string emojiBody: !root.isCall
                                        ? Emoji.inText(root.shownText, root.drawsStyled,
                                                       messageLabel.font.pixelSize)
                                        : ""
    /// Body as drawn, and whether StyledText.
    readonly property string bodyText: root.emojiBody.length > 0
                                       ? root.emojiBody : root.shownText
    readonly property bool bodyStyled: root.drawsStyled || root.emojiBody.length > 0
    /// Quote: as written.
    readonly property string emojiQuote: Emoji.inText(root.quoteText, false,
                                                      Theme.fontSizeExtraSmall)
    /// Header only, fetching, or failed: tappable.
    readonly property bool heldBack: root.downloadState.length > 0
                                     && root.downloadState !== "Done"
    /// Download states offering rest.
    readonly property bool canDownload: root.downloadState === "Available"
                                        || root.downloadState === "Failure"

    /// Offer text here: bubble sizes from it.
    //: Opens the whole message on a page of its own.
    readonly property string fullText: qsTr("View full message")

    /// Offer width; 0 when absent.
    readonly property real actionsWidth: root.showsFull
                                         ? fullMetric.implicitWidth : 0

    /// Footer: edited, unencrypted, time, progress. Built once.
    readonly property string footerText: (root.isEdited ? qsTr("Edited") + " · " : "")
                                         + (root.showPadlock ? "" : "✉ ")
                                         + Qt.formatTime(new Date(root.sentAt * 1000), "hh:mm")
                                         + (root.isOutgoing
                                            ? " " + Format.stateMark(root.deliveryState) : "")

    /// Body lines before page takes over (a paragraph). Not lifted in place: open body made row
    /// taller than view, unscrollable.
    property int collapsedLines: 12
    /// Long bodies only; not captionless pictures nor held-back messages.
    readonly property bool hasBody: root.messageText.length > 0 && !root.heldBack
                                    && !root.isCall
    /// Core cut message: rest in HTML part, off phone. `hasHtml` alone too broad: core sets it
    /// for most mail (HTML alternative/only, stripped quote, forward). Only real cut ends in
    /// core's `[...]` marker: rest missing, not a second rendering.
    readonly property bool wasCut: root.hasHtml
                                   && root.messageText.slice(-5) === "[...]"
    /// Offer page: anything not shown whole, every cut message.
    readonly property bool showsFull: root.hasBody
                                      && (root.wasCut || messageLabel.truncated)

    // Bubble as wide as content, measured off unconstrained copies (real label would loop).
    property real maxWidth: root.width * 0.78 - 2 * Theme.paddingMedium
    /// Width all but offer wants; body wraps here. Wrapping at offer-widened width loops (fits,
    /// offer goes, bubble narrows, no fit); QML drops a binding, width then rides arrival order.
    /// Offer never reaches this; bubble's is this widened for it.
    property real bodyWidth: Math.min(
        root.maxWidth,
        Math.max(textMetric.implicitWidth,
                 attachmentMetric.implicitWidth,
                 root.isCall ? callRow.wantedWidth : 0,
                 reactionRow.wantedWidth,
                 footerMetric.implicitWidth,
                 root.attachmentFillsBubble ? root.maxWidth : 0,
                 Theme.itemSizeSmall))
    /// Attachment fills bubble: preview's `wantsFullWidth`, computed here (preview null while
    /// loading; asking it decoded a picture twice). Keep in step; qml_bubble_width.rs checks.
    readonly property bool attachmentFillsBubble: root.hasFile
        && (root.viewType === "Image" || root.viewType === "Sticker"
            || root.viewType === "Gif" || root.viewType === "Video"
            || root.viewType === "Audio" || root.viewType === "Voice"
            || root.viewType === "Vcard"
            || (root.viewType === "Webxdc" && root.appsEnabled
                && root.webxdcName.length > 0))
    /// Bubble width: content, widened to hold offer.
    property real contentWidth: Math.min(root.maxWidth,
                                         Math.max(root.bodyWidth,
                                                  root.actionsWidth))

    // Chips hang below bubble; row makes room, else they overlap next message.
    height: (root.isInfo ? infoLabel.height : bubble.height)
            + (reactionRow.shown ? reactionRow.height - root.chipOverlap : 0)
            + 2 * Theme.paddingSmall

    /// How far chips reach up bottom edge: hung on it, no more (footer time in that corner).
    readonly property real chipOverlap: Theme.paddingMedium - Theme.paddingSmall / 2

    // Next part right below last present; gap only when both present.
    function below(previous, mine) {
        return previous.y + previous.height
               + (mine && previous.height > 0 ? Theme.paddingSmall : 0)
    }

    Text {
        id: textMetric
        visible: false
        font: messageLabel.font
        // Measured as drawn: bold wider.
        textFormat: root.bodyStyled ? Text.StyledText : Text.PlainText
        // Call text not drawn: doesn't size bubble.
        text: root.isCall ? "" : root.bodyText
        // Body lines unwrapped, as many as label shows: widest sets body width, even past bubble.
        maximumLineCount: root.collapsedLines
    }

    Text {
        id: attachmentMetric
        visible: false
        font.pixelSize: Theme.fontSizeMedium
        textFormat: Text.PlainText
        // Asked of preview, not its label.
        text: attachment.item ? attachment.item.genericText : ""
    }

    // Offer measured unconstrained, else width loops through bubble.
    Text {
        id: fullMetric
        visible: false
        font.pixelSize: Theme.fontSizeSmall
        textFormat: Text.PlainText
        text: root.fullText
    }

    // Footer likewise: "Edited" can widen one-word bubble.
    Text {
        id: footerMetric
        visible: false
        font.pixelSize: Theme.fontSizeExtraSmall
        textFormat: Text.PlainText
        text: root.footerText
    }

    // Chip counts end to end, for strip width (emoji square each).
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

    // One chip-font line: chip height; emoji a line high, picture or text.
    Text {
        id: chipLineMetric
        visible: false
        font.pixelSize: Theme.fontSizeSmall
        textFormat: Text.PlainText
        text: "0"
    }

    /// Count only for more than one: "👍", not "👍 1".
    function chipCount(reaction) {
        return reaction.count > 1 ? "" + reaction.count : ""
    }

    // Core notice, not typed: centred, unadorned.
    Label {
        id: infoLabel
        objectName: "infoLabel"
        // Optional parts size by own reason, never `visible` (false for whole covered page:
        // measuring rows collapsed while away). See qml_hidden_rows.rs.
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
        // Found message lit, not outlined: border would resize, move rows.
        color: root.isFound
               ? Theme.rgba(Theme.highlightColor, 0.5)
               // Sticker on conversation, no bubble.
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

        // Marked as reference client, above quote (covers whole message). Not remote text: no
        // pinning, but kept out of messageText.
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

        // Quote, bar down left.
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
                // Elided only without pictures: see body.
                truncationMode: root.emojiQuote.length > 0 ? TruncationMode.None
                                                           : TruncationMode.Elide
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                // Plain unless emoji: see top.
                textFormat: root.emojiQuote.length > 0 ? Text.StyledText : Text.PlainText
                text: root.emojiQuote.length > 0 ? root.emojiQuote : root.quoteText
            }
        }

        // Any attachment kind, one component knows them. Reports, never acts. Built only with file:
        // text rows skip renderers.
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
                // Long press on own control: row menu.
                onMenuRequested: root.menuRequested()
            }
        }

        // Call: kind, outcome, length, instead of text.
        Item {
            id: callRow
            objectName: "callRow"
            readonly property bool shown: root.isCall
            /// Icon, gap, wider of two lines.
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
                // Theme icons take colour after `?`.
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
            // Under call, else attachment; rows below chain from here.
            y: root.below(callRow.shown ? callRow : attachment, messageLabel.shown)
            // Body width, not bubble's: differ only when offer widened it.
            width: root.bodyWidth
            font.pixelSize: root.bigEmoji ? 2 * Theme.fontSizeMedium : Theme.fontSizeMedium
            wrapMode: Text.Wrap
            // Long body cut to a few lines; rest on page (row taller than view can't be scrolled
            // past). Line cap, not text cut: slicing a rendering could leave tag open.
            maximumLineCount: root.collapsedLines
            // Cut, not elided, with pictures: Qt lays elided last line out on its own, puts there
            // every picture whose place in whole text falls inside that line's length (one at
            // start drew over last line, its gap left on first). Offer below says it's cut.
            truncationMode: root.emojiBody.length > 0 ? TruncationMode.None
                                                      : TruncationMode.Elide
            color: Theme.primaryColor
            linkColor: Theme.highlightColor
            // Plain unless shim rendered: see top.
            textFormat: root.bodyStyled ? Text.StyledText : Text.PlainText
            text: root.bodyText
            // Link followed on tap only.
            onLinkActivated: Qt.openUrlExternally(link)
        }

        // Body not fitting: two-word tap in download offer's colour.
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
                // Row's far end. Placed, not anchored.
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

        // Held by download limit: header here; rest asked for.
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
            // Translated literals, by core's state.
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

        // Time; own: progress. Mail icon: unencrypted. "Edited": changed later.
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

    // Chip per emoji, ours lit, off bubble's inside corner, mostly below edge so time stays
    // readable. Hand-laid: each reads widths before it. One clipped line: last of many can hide.
    Item {
        id: reactionRow
        objectName: "reactionRow"
        readonly property bool shown: !root.isInfo && root.reactionList.length > 0
        visible: reactionRow.shown
        // Inside bubble padding, corner nearest screen middle.
        x: root.isOutgoing ? bubble.x + Theme.paddingMedium
                           : bubble.x + bubble.width - width - Theme.paddingMedium
        y: bubble.height - root.chipOverlap
        // Never wider than bubble.
        width: Math.min(wantedWidth, root.contentWidth)
        // Chip-font line plus chip padding.
        height: reactionRow.shown ? chipLineMetric.height + 2 * Theme.paddingSmall : 0
        clip: true

        /// Emoji size on chip: a text line.
        readonly property real glyphSize: chipLineMetric.height

        /// Chip room in row: emoji, counts, gaps, padding.
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
                /// Reader's own reaction.
                readonly property bool mine: modelData.self === true
                /// Emoji, for tap to name.
                readonly property string emoji: modelData.emoji
                // After chips before it: repeater builds in order.
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
                // Nearly solid: chip straddles bubble edge; translucent changes colour mid-way.
                color: mine ? Theme.rgba(Theme.highlightBackgroundColor, 0.9)
                            : Theme.rgba(Theme.highlightDimmerColor, 0.9)

                // Picture where shipped (Sailfish draws few emoji). Peer text unchecked; EmojiGlyph
                // draws non-emoji plain.
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
