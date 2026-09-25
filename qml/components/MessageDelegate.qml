import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Format.js" as Format
import "../js/Calls.js" as Calls

/*
 * One message. Its own component so it can be loaded and measured on its
 * own, a row at a time, without the page and the model a conversation
 * needs around it.
 *
 * Every string here comes from whoever sent the message, so each one
 * that shows it is pinned to PlainText: the default detects markup and
 * switches to rich text, which would let a message body fetch a remote
 * image the moment its row is drawn. The one exception is deliberate: with
 * Markdown drawn, the body is the shim's own rendering of it
 * (markdown.rs), in which every character of the message is escaped and
 * the only tags are the ones the shim wrote -- and it is shown as
 * StyledText, never RichText, so nothing in it can load anything.
 *
 * Laid out by bindings rather than by a Column: a positioner sizes itself in
 * a polish pass, which never runs headlessly, so a row built from one cannot
 * be measured in a test.
 */
Item {
    id: root

    /// The reader asked to open the attachment. The URL rather than the
    /// path: the encoding it needs is already done once, in the preview.
    /// `previewWidth` is how wide the picture was drawn here, so a page
    /// opening it full screen can start from the same decode.
    signal openRequested(url fileUrl, string fileName, string viewType,
                         real previewWidth)
    /// The reader asked to read the message on a page of its own.
    signal fullTextRequested()
    /// The reader asked for the rest of a message the core holds only
    /// the header of.
    signal downloadRequested()
    /// The reader tapped a webxdc app. Running one is a page of its own,
    /// which is the conversation's to push.
    signal appRequested()
    /// The reader tapped a reaction chip: put that emoji on the message,
    /// or take it off again when it is already theirs. The model decides
    /// which; the row only says what was tapped.
    signal reactionRequested(string emoji)
    /// The reader tapped a call. What that means -- going back to one
    /// that is still ringing, or calling back -- is the page's to decide:
    /// it knows whether a call is up and whether this chat can take one.
    signal callRequested()
    /// A long press landed on a control that takes presses for itself --
    /// a chip, the download offer, the play button -- and the row's menu
    /// is what a long press means anywhere on a message.
    signal menuRequested()

    /// What a tap on the message does. The row's own tap, not one of the
    /// attachment's: a picture with a tap area of its own took the press
    /// away from the row, so a long press on it never reached the menu,
    /// and a tap just off it did nothing. The whole message is one
    /// surface now -- a tap opens what there is to open, a long press
    /// opens the menu -- and the two cannot fight over a pixel.
    function tapped() {
        if (root.isCall) {
            root.callRequested()
            return
        }
        // Null for a message with no file, which has nothing to open.
        var preview = attachment.item
        if (preview && preview.isApp) {
            // An app is run here rather than handed to whatever the
            // system thinks opens a .xdc, which is nothing.
            root.appRequested()
        } else if (preview && preview.openable) {
            root.openRequested(preview.fileUrl, root.fileName, root.viewType,
                               preview.contentWidth)
        } else if (root.canDownload) {
            root.downloadRequested()
        }
    }

    property string messageText: ""
    /// The reactions on this message, as the shim hands them over: a JSON
    /// array of {emoji, count, self}, most frequent first. Empty for none.
    property string reactions: ""
    /// The same, parsed once per change rather than once per chip.
    readonly property var reactionList: root.reactions.length > 0
                                        ? JSON.parse(root.reactions) : []
    /// The same text rendered as StyledText by the shim. Empty for a row
    /// that has no rendering, which is shown as written.
    property string styledText: ""
    /// 0 draws Markdown; anything else shows the message as written.
    property int markdownMode: 1
    /// The sender changed the text after sending it. Said in the footer,
    /// as the reference clients say it, so a reader who remembers the
    /// first wording knows why it is not there.
    property bool isEdited: false
    /// `downloadState` upstream: Done, Available, InProgress, Failure or
    /// Undecipherable. Anything but Done and empty is a message the core
    /// has only the header of.
    property string downloadState: ""
    property bool isOutgoing: false
    property bool isInfo: false
    property bool isForwarded: false
    /// This is the message a search sent the reader here for.
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
    // A real, not an int: QML has no 64-bit integer to hold a file size in.
    property real fileBytes: 0
    // Text, Image, Gif, Sticker, Audio, Voice, Video, File, Call, Webxdc,
    // Vcard. What the attachment is drawn as; AttachmentPreview decides.
    property string viewType: "Text"
    property int imageWidth: 0
    property int imageHeight: 0
    /// A message the reader has not seen before; see AttachmentPreview.
    property bool isNew: false
    /// `hasHtml` upstream: the text shown here is not the whole of what
    /// arrived, and an HTML part holds the rest -- which the page can
    /// fetch from the core and this row cannot. Being cut is only one of
    /// the things it means; see `wasCut`.
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
    /// Whether webxdc apps are on. Handed down to the attachment, which
    /// decides from it whether this row holds an app or a file; `tapped`
    /// asks the attachment rather than reading this itself, so there is
    /// one answer rather than two.
    property bool appsEnabled: false
    /// Where a call stands, as the core names it: Alerting, Active,
    /// Completed, Missed, Declined or Canceled. Empty for anything that
    /// is not a call, and for a call the core would say nothing about --
    /// which then draws as its text, the core's own sentence.
    property string callState: ""
    property bool callHasVideo: false
    /// Seconds, for a completed call.
    property int callDuration: 0

    /// This row is a call, drawn as one.
    readonly property bool isCall: root.viewType === "Call"
                                   && root.callState.length > 0
    /// What a call row says first, and the line under it; see Calls.js.
    readonly property string callTitle: Calls.title(root.callState, root.callHasVideo)
    readonly property string callDetail: Calls.detail(root.callState, root.callDuration,
                                                      root.isOutgoing)
    /// A call that did not happen, drawn in the colour that says so.
    readonly property bool callFailed: Calls.failed(root.callState)

    property bool hasFile: filePath.length > 0
    // A sticker is a picture with no bubble behind it, which is the whole
    // of what makes it one.
    readonly property bool isSticker: root.viewType === "Sticker" && root.hasFile

    /// Whether the body is drawn from the shim's rendering. Only with a
    /// rendering to draw: the raw text as StyledText would be the very
    /// thing the plain-text pinning is for.
    readonly property bool drawsStyled: root.markdownMode === 0
                                        && root.styledText.length > 0
    /// The body as the setting wants it shown.
    readonly property string shownText: root.drawsStyled
                                        ? root.styledText
                                        : root.messageText
    /// A message the core has only the header of, or is fetching, or
    /// could not fetch: something to say, and mostly something to tap.
    readonly property bool heldBack: root.downloadState.length > 0
                                     && root.downloadState !== "Done"
    /// The two states the rest of a message can be asked for in.
    readonly property bool canDownload: root.downloadState === "Available"
                                        || root.downloadState === "Failure"

    /// What the offer says. Here rather than only in the label because
    /// the bubble is sized from it: a bubble made narrow by short lines
    /// has to widen to hold it, or the offer hangs out of it.
    //: Opens the whole message on a page of its own.
    readonly property string fullText: qsTr("View full message")

    /// How wide the offer wants to be. Nothing when there is none, so an
    /// ordinary bubble is not held open by two words it does not show.
    readonly property real actionsWidth: root.showsFull
                                         ? fullMetric.implicitWidth : 0

    /// The footer's line: that the text was edited, whether it went
    /// unencrypted, the time, and for our own messages how far it got.
    /// Built once here, for the label and for the copy that measures it.
    readonly property string footerText: (root.isEdited ? qsTr("Edited") + " · " : "")
                                         + (root.showPadlock ? "" : "✉ ")
                                         + Qt.formatTime(new Date(root.sentAt * 1000), "hh:mm")
                                         + (root.isOutgoing
                                            ? " " + Format.stateMark(root.deliveryState) : "")

    /// How many lines of a body the bubble shows before sending the
    /// reader to the page. Enough for a paragraph, which is what most
    /// messages are.
    ///
    /// The cap does not lift here any more. Opening a body out in place
    /// was a second way to read the same words, and the worse of the
    /// two: it made one row taller than the view, which is a row that
    /// cannot be scrolled past, and folding it again had to put the
    /// reader back where they were by hand. The page shows the whole
    /// message and nothing else, and leaving it puts them back.
    property int collapsedLines: 12
    /// Whether this row is showing words at all, and has all of them to
    /// show.
    ///
    /// The offer is about a long body and nothing else. A picture or a
    /// document with no caption has no body to read on a page, and a
    /// message the core is still holding back has none of it yet -- what
    /// that row needs is Download, which it already offers. Neither was
    /// excluded before, and an attachment arriving in an open chat grew
    /// a View full message it had no use for.
    readonly property bool hasBody: root.messageText.length > 0 && !root.heldBack
                                    && !root.isCall
    /// Whether the sending core cut this message, leaving the rest of it
    /// in an HTML part and off this phone.
    ///
    /// `hasHtml` alone does not say that, though the name reads as if it
    /// does. The core raises it for any message whose text is not the
    /// whole of what arrived: an ordinary mail with an HTML alternative
    /// beside its plain part, a mail written only in HTML, a reply whose
    /// quoted history was stripped, a forward. That is very nearly
    /// everything written in a mail client rather than a chat app, and
    /// reading it as "cut" put a View full message under most of a
    /// mailbox -- two-word replies included -- each one offering a page
    /// with the same words already in the bubble.
    ///
    /// A message the core really did cut ends in the marker it writes,
    /// `[...]`, and only a cut one does: that marker goes on in the one
    /// place upstream shortens a body. So both are asked for. The flag
    /// says there is an HTML part to fetch the rest out of, and the
    /// marker says the rest is what is missing rather than a second
    /// rendering of what is here.
    readonly property bool wasCut: root.hasHtml
                                   && root.messageText.slice(-5) === "[...]"
    /// Whether to offer the page: anything the bubble is not showing
    /// whole, and every message the sending core cut -- for those the
    /// rest is not on this phone at all, and the page is the only thing
    /// that can go and get it.
    readonly property bool showsFull: root.hasBody
                                      && (root.wasCut || messageLabel.truncated)

    // A bubble is as wide as its content, up to most of the screen. The
    // widths come off unconstrained copies of the text: measuring the real
    // labels, whose width comes back from the bubble, is a binding loop.
    property real maxWidth: root.width * 0.78 - 2 * Theme.paddingMedium
    /// How wide everything but the offer wants the bubble, and the width
    /// the body is wrapped at.
    ///
    /// Held apart from the bubble's own width because the offer is
    /// decided from whether the body fits. Wrap the body at a width the
    /// offer has already widened, and the offer's appearing changes the
    /// wrapping that decides whether it appears: the body fits, so the
    /// offer goes, so the bubble narrows, so the body no longer fits.
    /// QML answers a circle like that by dropping a binding where it
    /// stands, which leaves a bubble at a width nothing on it asked for,
    /// and leaves it there until the row is built again -- so what a
    /// message looked like came down to the order its properties
    /// happened to arrive in, and looked different after leaving the
    /// chat and coming back.
    ///
    /// The offer cannot reach this width. The bubble's is this one
    /// widened to hold the offer, below.
    property real bodyWidth: Math.min(
        root.maxWidth,
        Math.max(textMetric.implicitWidth,
                 attachmentMetric.implicitWidth,
                 root.isCall ? callRow.wantedWidth : 0,
                 reactionRow.wantedWidth,
                 footerMetric.implicitWidth,
                 attachment.item && attachment.item.wantsFullWidth
                     ? root.maxWidth : 0,
                 Theme.itemSizeSmall))
    /// How wide the bubble is: what its content wants, widened to hold
    /// the offer when there is one. A long message of short lines makes
    /// a narrow bubble, and the offer under it would hang out of one
    /// sized from the lines alone.
    property real contentWidth: Math.min(root.maxWidth,
                                         Math.max(root.bodyWidth,
                                                  root.actionsWidth))

    // The chips hang below the bubble, and what hangs is the row's to
    // make room for: without it they draw over the next message.
    height: (root.isInfo ? infoLabel.height : bubble.height)
            + (reactionRow.shown ? reactionRow.height - root.chipOverlap : 0)
            + 2 * Theme.paddingSmall

    /// How far the chips reach up over the bubble's bottom edge: enough
    /// to read as hung on it, and no more, since the footer's time sits
    /// in that corner and the chips went over it at half their height.
    readonly property real chipOverlap: Theme.paddingMedium - Theme.paddingSmall / 2

    // Where the next part goes: right below the last one that is there,
    // with a gap only when both are.
    function below(previous, mine) {
        return previous.y + previous.height
               + (mine && previous.height > 0 ? Theme.paddingSmall : 0)
    }

    Text {
        id: textMetric
        visible: false
        font: messageLabel.font
        // Measured as it will be drawn: bold is wider than plain.
        textFormat: root.drawsStyled ? Text.StyledText : Text.PlainText
        // A call's text is not drawn, so it does not size the bubble.
        text: root.isCall ? "" : root.shownText
    }

    Text {
        id: attachmentMetric
        visible: false
        font.pixelSize: Theme.fontSizeMedium
        textFormat: Text.PlainText
        // Asked of the preview rather than read off its label: what the
        // fallback row says is the preview's business, and the bubble only
        // needs to know how wide it comes out.
        text: attachment.item ? attachment.item.genericText : ""
    }

    // The offer, measured where nothing constrains it: reading a width
    // off the label itself would come back through the bubble it is
    // sizing.
    Text {
        id: fullMetric
        visible: false
        font.pixelSize: Theme.fontSizeSmall
        textFormat: Text.PlainText
        text: root.fullText
    }

    // The footer, measured the same way: a one-word message with
    // "Edited" in its footer is a bubble the footer has to widen.
    Text {
        id: footerMetric
        visible: false
        font.pixelSize: Theme.fontSizeExtraSmall
        textFormat: Text.PlainText
        text: root.footerText
    }

    // The chips' text end to end, for how wide the strip wants to be and
    // how tall one line of it is. The chips themselves are measured one
    // by one below; this is the sum the bubble's width is decided from
    // before any of them exists.
    Text {
        id: reactionMetric
        visible: false
        font.pixelSize: Theme.fontSizeSmall
        textFormat: Text.PlainText
        text: {
            var parts = []
            for (var i = 0; i < root.reactionList.length; i++) {
                parts.push(root.chipText(root.reactionList[i]))
            }
            return parts.join(" ")
        }
    }

    /// What a chip says: the emoji, and how many when it is more than one
    /// person. "👍" reads as one; "👍 1" reads as a score.
    function chipText(reaction) {
        return reaction.count > 1 ? reaction.emoji + " " + reaction.count
                                  : reaction.emoji
    }

    // A core notice, not something anyone typed: centred and unadorned.
    Label {
        id: infoLabel
        objectName: "infoLabel"
        // Every part of a message that may or may not be there sizes
        // itself by its own reason to be, never by `visible`. That reads
        // the *effective* visibility, which goes false for everything on
        // a page the moment the platform hides the page under another --
        // and a row whose parts all measured `visible ? implicitHeight
        // : 0` collapsed to nothing while the page was away, so the list
        // rebuilt itself twice on every return: once to fill the void it
        // suddenly had, and again, on the way back, to undo that. See
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
        // A found message is lit rather than outlined: a border would
        // change the bubble's size, and every row below it would move.
        color: root.isFound
               ? Theme.rgba(Theme.highlightColor, 0.5)
               // A sticker is meant to sit on the conversation rather than
               // in a bubble; the rest of the row still lays out the same.
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

        // Marked the way the reference client marks it, and above the
        // quote: it describes the whole message, not the quoted part.
        // Not remote-supplied text, so it needs no PlainText pinning --
        // but it must not be folded into messageText, which is.
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
                textFormat: Text.PlainText
                text: root.quoteText
            }
        }

        // Whatever kind of attachment this is, drawn by the one component
        // that knows the difference. Reports rather than acts, so opening
        // stays the page's decision.
        //
        // Built only for a message that has a file. The preview holds a
        // renderer for every kind there is -- a picture and its animation,
        // the thumbnailer's poster, a sound player -- and a text message
        // carried all of them, unseen, for the height of one line. A
        // conversation is mostly text, and what a row costs to build is
        // what a flick costs per frame. The loader takes the preview's
        // size, so the rows below it sit where they did.
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
                // A long press on one of its own controls is the row's menu.
                onMenuRequested: root.menuRequested()
            }
        }

        // A call: what kind, or what became of it, and how long it
        // lasted. In place of the text, which is the same thing said by
        // the core in English.
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
            // Under the call when there is one -- nothing is, since a
            // call's text is not drawn, but the rows below chain from
            // here -- and under the attachment otherwise, which is what
            // the gap between the two is measured from.
            y: root.below(callRow.shown ? callRow : attachment, messageLabel.shown)
            // The body's width, not the bubble's: see `bodyWidth`. The
            // two differ only when the offer widened the bubble, and the
            // body is left-aligned in it either way.
            width: root.bodyWidth
            wrapMode: Text.Wrap
            // A bubble is a shape for a remark, not for a document. A
            // long body is cut to a readable few lines and the rest is
            // read on a page: a to-do list somebody sent otherwise fills
            // the screen and pushes the whole conversation out of it,
            // and a row taller than the view is one that cannot be
            // scrolled past. A cap on the lines rather than a cut in the
            // text, so nothing has to slice a rendering in half and
            // leave a tag open.
            maximumLineCount: root.collapsedLines
            truncationMode: TruncationMode.Elide
            color: Theme.primaryColor
            linkColor: Theme.highlightColor
            // Plain, unless the shim rendered it: see the note at the top.
            textFormat: root.drawsStyled ? Text.StyledText : Text.PlainText
            text: root.shownText
            // A link is followed on a tap and on nothing else.
            onLinkActivated: Qt.openUrlExternally(link)
        }

        // What to do about a body that does not fit: read it on a page of
        // its own. Two words rather than a button, in the highlight
        // colour the download offer uses -- Silica's Button inside a
        // bubble would be a box inside a box.
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
                // At the far end of the row. Placed rather than
                // anchored, as it was when it had something beside it to
                // be placed against.
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

        // A message the download limit held back: the core has its
        // header and this is where the rest is asked for.
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

        // Time, and for our own messages how far it got. A mail icon marks
        // anything that was not encrypted and signed, and "Edited" a text
        // the sender changed afterwards.
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

    // Who reacted with what, one chip per emoji, ours lit. Hung off the
    // bubble's bottom corner rather than laid out inside it -- the
    // inside corner, towards the middle of the screen, the way the
    // reference clients hang theirs: just over the bubble's edge and
    // mostly below it, so a reaction reads as something put on the
    // message rather than a line of it, and the time in that corner
    // stays readable. Laid out by hand rather than in a Row,
    // for the reason at the top of the file: each chip sits after the
    // ones before it, and reads their widths so a chip that grows
    // pushes the rest along. One line, clipped: a message with more
    // distinct reactions than fit is rare enough that the last of them
    // can go unseen.
    Item {
        id: reactionRow
        objectName: "reactionRow"
        readonly property bool shown: !root.isInfo && root.reactionList.length > 0
        visible: reactionRow.shown
        // Inside the bubble's own padding, at the corner nearest the
        // middle of the screen.
        x: root.isOutgoing ? bubble.x + Theme.paddingMedium
                           : bubble.x + bubble.width - width - Theme.paddingMedium
        y: bubble.height - root.chipOverlap
        // Never wider than the bubble, which sizes itself to fit the
        // chips where it can.
        width: Math.min(wantedWidth, root.contentWidth)
        // A line of the chip font plus the chip's own padding.
        height: reactionRow.shown ? reactionMetric.height + 2 * Theme.paddingSmall : 0
        clip: true

        /// The room the chips take in a row: their text, each one's
        /// padding, and the gaps between them.
        readonly property real wantedWidth:
            root.reactionList.length === 0 ? 0
            : reactionMetric.implicitWidth
              + root.reactionList.length * 2 * Theme.paddingMedium
              + (root.reactionList.length - 1) * Theme.paddingSmall

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
                // After every chip before it. Reading their widths is
                // what makes this follow them: the repeater builds
                // chips in order, so each one it asks for is there.
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
                width: chipLabel.implicitWidth + 2 * Theme.paddingMedium
                height: reactionRow.height
                radius: height / 2
                // Nearly solid: a chip straddles the bubble's edge, and
                // a translucent one would change colour halfway down.
                color: mine ? Theme.rgba(Theme.highlightBackgroundColor, 0.9)
                            : Theme.rgba(Theme.highlightDimmerColor, 0.9)

                Label {
                    id: chipLabel
                    objectName: "chipLabel"
                    anchors.centerIn: parent
                    font.pixelSize: Theme.fontSizeSmall
                    color: Theme.primaryColor
                    // The emoji is whatever the other end sent, and
                    // the core does not check that it is one.
                    textFormat: Text.PlainText
                    text: root.chipText(modelData)
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
