import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Media.js" as Media

/*
 * Chat media, from contact/group page rows. One kind (apps): PageHeader over its
 * view. Several (Media and files: gallery, audio, files): Vuo's tab strip over a
 * PagedView, one view and model per tab, all alive (cacheSize) so a swipe shows rows.
 */
Page {
    id: page

    property int accountId
    property int chatId
    /// One kind: gallery, audio, files or apps (ChatMedia.kind).
    property string kind: "gallery"
    /// Kinds as tabs; one kind: no strip.
    property var kinds: [page.kind]
    /// Tab shown first.
    property int startIndex: 0
    property string errorMessage: ""

    readonly property bool tabbed: page.kinds.length > 1

    /// App-wide `Transcriber`; null (tests, loaded alone): no transcripts.
    property QtObject transcriber: typeof appWindow !== "undefined" ? appWindow.transcriber : null
    /// See AttachmentPreview.
    property Component transcriptMaker: null
    /// Ms before message goes, every tab. Tests set low.
    property int pendingDelay: 4000

    // Grid of pictures: worth turning the phone.
    allowedOrientations: Orientation.All

    /// Tab's view, or the one view.
    function viewAt(index) {
        return page.tabbed ? pager.itemAt(index) : single.item
    }

    // Leaving mid-wait still deletes (conversation writes draft then too).
    onStatusChanged: {
        if (status === PageStatus.Deactivating) {
            for (var i = 0; i < page.kinds.length; i++) {
                var view = page.viewAt(i)
                if (view) {
                    view.flushDeletes()
                }
            }
        }
    }

    Component {
        id: titleHeader
        PageHeader {
            title: Media.kindName(page.kind)
        }
    }

    // Room the strip stands in, at list top: scrolls away under it.
    Component {
        id: stripRoom
        Item {
            width: parent ? parent.width : 0
            height: page.stripBand
        }
    }

    Loader {
        id: single
        active: !page.tabbed
        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
            bottom: errorBanner.top
        }
        sourceComponent: ChatMediaView {
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
    }

    /// Current tab's scroll past its top; negative while pulley pulled.
    readonly property real yOffset: page.tabbed && pager.currentItem
                                    && pager.currentItem.flickable
                                    ? pager.currentItem.flickable.contentY
                                      - pager.currentItem.flickable.originY
                                    : 0
    readonly property real stripBand: page.tabbed ? tabs.height : 0

    /// Strip tap: go there; tapping shown tab goes back to top.
    function selectTab(index) {
        if (index === pager.currentIndex) {
            if (pager.currentItem && pager.currentItem.flickable) {
                pager.currentItem.flickable.scrollToTop()
            }
            return
        }
        pager.moveTo(index)
    }

    // Paints nothing behind words: ambience shows through (Vuo). Rides down with a
    // pull and drops behind it, so the menu draws over (Silica's TabView).
    ScopeTabBar {
        id: tabs
        objectName: "tabs"
        visible: page.tabbed
        anchors {
            left: parent.left
            right: parent.right
        }
        y: Math.max(0, -page.yOffset)
        height: visible ? implicitHeight : 0
        z: page.yOffset < 0 ? -1 : 1
        hostPage: page
        titles: page.tabbed ? page.kinds.map(function(kind) { return Media.kindName(kind) })
                            : []
        currentIndex: pager.currentIndex
        onTabClicked: page.selectTab(index)
    }

    // Page less strip band, clipped only while scrolled: rows never pass behind the
    // strip; at rest and mid-pull the band holds header and pulley.
    Item {
        id: viewport
        visible: page.tabbed
        width: page.width
        y: page.stripBand
        height: errorBanner.y - page.stripBand
        clip: page.yOffset > 0

        PagedView {
            id: pager
            objectName: "pager"
            // Back up by the band: list tops at screen top, pulley above it.
            y: -page.stripBand
            width: viewport.width
            height: viewport.height + page.stripBand
            model: page.tabbed ? page.kinds.length : 0
            cacheSize: page.kinds.length

            Component.onCompleted: {
                if (page.startIndex > 0) {
                    pager.currentIndex = page.startIndex
                }
            }

            // Delegate compares `pager.currentIndex`: attached `PagedView.isCurrentItem`
            // can't be stubbed.
            delegate: ChatMediaView {
                objectName: "tabView" + index
                width: pager.width
                height: pager.height
                accountId: page.accountId
                chatId: page.chatId
                kind: page.kinds[index]
                host: page
                header: stripRoom
                transcriber: page.transcriber
                transcriptMaker: page.transcriptMaker
                pendingDelay: page.pendingDelay
                onNoticed: notice.show(text)
                onFailed: page.errorMessage = message
            }
        }
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
