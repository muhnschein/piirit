import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Media.js" as Media

/*
 * One chat media kind (gallery, audio, files, apps), from contact/group page rows:
 * PageHeader over its view.
 */
Page {
    id: page

    property int accountId
    property int chatId
    /// Gallery, audio, files or apps (ChatMedia.kind).
    property string kind: "gallery"
    property string errorMessage: ""

    /// App-wide `Transcriber`; null (tests, loaded alone): no transcripts.
    property QtObject transcriber: typeof appWindow !== "undefined" ? appWindow.transcriber : null
    /// See AttachmentPreview.
    property Component transcriptMaker: null
    /// Ms before message goes. Tests set low.
    property int pendingDelay: 4000

    // Grid of pictures: worth turning the phone.
    allowedOrientations: Orientation.All

    // Leaving mid-wait still deletes (conversation writes draft then too).
    onStatusChanged: {
        if (status === PageStatus.Deactivating) {
            view.flushDeletes()
        }
    }

    Component {
        id: titleHeader
        PageHeader {
            objectName: "pageHeader"
            title: Media.kindName(page.kind)
        }
    }

    ChatMediaView {
        id: view
        objectName: "view"
        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
            bottom: errorBanner.top
        }
        accountId: page.accountId
        chatId: page.chatId
        kind: page.kind
        host: page
        header: titleHeader
        transcriber: page.transcriber
        transcriptMaker: page.transcriptMaker
        pendingDelay: page.pendingDelay
        onNoticed: notice.show(text)
        onFailed: page.errorMessage = message
    }

    Banner {
        id: errorBanner
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            bottom: notice.top
        }
        text: page.errorMessage
        timeout: 8
        onDismissed: page.errorMessage = ""
    }

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
}
