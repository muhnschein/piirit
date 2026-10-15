import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * Bring existing profile here, two ways (as other apps):
 *
 * - `from: "device"`: other device offers over LAN, shows code; read here,
 *   transfer via `get_backup`. Both keep profile.
 * - `from: "file"`: backup via `import_backup`.
 *
 * Core transfers; page is bar + stop. One at a time (shim attempt, like making
 * a profile): Cancel is `cancel_ongoing`; second orphans first. App words
 * refusals (core's are protocol talk). Profile already here refused: two
 * accounts on one address fetch mailbox twice.
 *
 * Hand-over via PendingNavigation: file browser still animates during small
 * import; mid-transition move was refused, stranding reader here.
 */
Page {
    id: page

    /// No quick actions while up: jump leaves profile half over. piirit.qml.
    readonly property bool pausesQuickActions: true

    allowedOrientations: Orientation.All

    /// Source: `device` or `file`.
    property string from: "device"
    readonly property bool fromDevice: page.from !== "file"

    /// True while core transfers.
    property bool busy: false
    /// Progress, permille.
    property int permille: 0
    property string errorMessage: ""
    /// Undismissed failure. Page state, not strip: live viewfinder reads as carrying on.
    readonly property bool failed: page.errorMessage.length > 0
    /// App in front. Writable: tests set it.
    property bool appActive: Qt.application.state === Qt.ApplicationActive

    // No back mid-transfer (drops listener, core goes on) or while hand-over waits (swipe takes
    // page and move).
    backNavigation: !page.busy && !navigation.pending

    /// Hand-over to arrived profile, held until stack can make it.
    PendingNavigation {
        id: navigation
        stack: pageStack
        // Silica won't move from covered or animating page.
        ready: page.status === PageStatus.Active
    }

    /// Start transfer from camera code or browser path.
    function begin(text) {
        // Browser may report choice twice; second import = second account.
        if (page.busy) {
            return
        }
        page.errorMessage = ""
        page.permille = 0
        page.busy = true
        if (page.fromDevice) {
            core.restore_from_device(text)
        } else {
            core.restore_from_file(text)
        }
    }

    /// Scanner reset after failure. View stops on code (no duplicates); reset it before clearing
    /// failure: activation starts camera only if holding no code.
    function rescan() {
        if (scanLoader.item) {
            scanLoader.item.done = false
        }
    }

    /// File browser, pushed by URL and connected, as conversation's pickers.
    function chooseFile() {
        var picker = pageStack.push(Qt.resolvedUrl("BackupFilePage.qml"), {})
        if (picker) {
            picker.picked.connect(page.begin)
        }
    }

    /// Refusal reason in words.
    function reasonText(reason) {
        if (reason === "too-new") {
            return qsTr("The profile is from a newer Delta Chat version than this app can read.")
        }
        if (reason === "stalled") {
            return qsTr("The transfer stopped. Both devices must stay on the same network with this page open.")
        }
        if (reason === "already-here") {
            return qsTr("That profile is already on this phone. Open it from the profiles list.")
        }
        return qsTr("This is not a second-device code. Use the code shown by the other device.")
    }

    // Qt 5.6 handler syntax; see WelcomePage.qml.
    Connections {
        target: core

        onRestore_progress: {
            if (page.busy) {
                page.permille = permille
            }
        }

        // Profile here, IO started: open on it, nothing below (nothing worth going back to).
        onProfile_restored: {
            if (!page.busy) {
                return
            }
            page.busy = false
            navigation.replaceAbove(null, Qt.resolvedUrl("ChatListPage.qml"),
                                    { accountId: account_id })
        }

        onRestore_refused: {
            if (!page.busy) {
                return
            }
            page.busy = false
            page.errorMessage = page.reasonText(reason)
        }

        onRestore_failed: {
            if (!page.busy) {
                return
            }
            page.busy = false
            page.errorMessage = message
        }
    }

    // Device half: camera, picture gets page. File half: ordinary page.
    Item {
        id: scanArea
        objectName: "scanArea"
        anchors.fill: parent
        visible: page.fromDevice && !page.busy && !page.failed

        // By URL as QR page: no camera costs scanner, not page.
        Loader {
            id: scanLoader
            objectName: "scanLoader"
            anchors.fill: parent
            active: page.fromDevice
            source: Qt.resolvedUrl("../components/ScanView.qml")
            onLoaded: {
                // Code string typed or pasted, for camera that won't read it.
                scanLoader.item.linkButtonText = qsTr("Enter the code instead")
                scanLoader.item.linkLabel = qsTr("Code from the other device")
                scanLoader.item.linkPlaceholder = "DCBACKUP5:..."
                scanLoader.item.linkActionText = qsTr("Copy the profile")
                // Digit: core transfer version (pinned 5); meaning is core's (`check_qr`).
                scanLoader.item.linkPrefixes = ["dcbackup"]
                scanLoader.item.scanned.connect(page.begin)
                scanLoader.item.failed.connect(function(message) {
                    page.errorMessage = message
                })
            }
        }

        // Camera on screen while idle; not background or screen-off (page stays active).
        Binding {
            target: scanLoader.item
            property: "active"
            value: page.status === PageStatus.Active && page.appActive && page.fromDevice
                   && !page.busy && !page.failed
        }

        // Zoom indicator under page's own strip.
        Binding {
            target: scanLoader.item
            property: "topInset"
            value: deviceChrome.height
        }

        // Page turns with phone; picture turned back.
        Binding {
            target: scanLoader.item
            property: "pageOrientation"
            value: page.orientation
        }

        Label {
            objectName: "noCamera"
            visible: scanLoader.status === Loader.Error
            anchors.centerIn: parent
            width: parent.width - 2 * Theme.horizontalPageMargin
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            color: Theme.secondaryHighlightColor
            text: qsTr("No camera on this device. You can restore from a backup file instead.")
        }

        // Over picture: video runs behind strip, not starts under block.
        Rectangle {
            id: deviceChrome
            objectName: "deviceChrome"
            anchors {
                left: parent.left
                right: parent.right
                top: parent.top
            }
            height: deviceWords.height + 2 * Theme.paddingLarge
            color: Theme.rgba(Theme.highlightDimmerColor, 0.8)

            Column {
                id: deviceWords
                anchors {
                    left: parent.left
                    right: parent.right
                    verticalCenter: parent.verticalCenter
                    leftMargin: Theme.horizontalPageMargin
                    rightMargin: Theme.horizontalPageMargin
                }
                spacing: Theme.paddingSmall

                Label {
                    objectName: "deviceTitle"
                    width: parent.width
                    textFormat: Text.PlainText
                    font.family: Theme.fontFamilyHeading
                    font.pixelSize: Theme.fontSizeLarge
                    color: Theme.highlightColor
                    text: qsTr("Add as second device")
                }

                Label {
                    objectName: "instructions"
                    width: parent.width
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    font.pixelSize: Theme.fontSizeSmall
                    color: Theme.secondaryHighlightColor
                    text: qsTr("On the other device, go to Settings and tap \"Add second device\". Both devices must be on the same network.")
                }
            }
        }
    }

    // File half and transfer: both end here.
    SilicaFlickable {
        id: flickable
        anchors.fill: parent
        visible: !scanArea.visible
        contentHeight: Math.max(height, column.height + Theme.paddingLarge)

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingLarge

            PageHeader {
                objectName: "header"
                title: page.fromDevice ? qsTr("Add as second device")
                                       : qsTr("Restore from a backup")
            }

            Label {
                objectName: "fileInstructions"
                visible: !page.fromDevice && !page.busy && !page.failed
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("Make a backup on the other device, copy the file here, then choose it.")
            }

            Button {
                objectName: "chooseFileButton"
                anchors.horizontalCenter: parent.horizontalCenter
                visible: !page.fromDevice && !page.busy && !page.failed
                text: qsTr("Choose a backup file")
                onClicked: page.chooseFile()
            }

            ProgressBar {
                objectName: "transferProgress"
                width: parent.width
                visible: page.busy
                minimumValue: 0
                maximumValue: 1000
                value: page.permille
                label: qsTr("Transferring…")
            }

            Button {
                objectName: "cancelButton"
                anchors.horizontalCenter: parent.horizontalCenter
                visible: page.busy
                text: qsTr("Cancel")
                onClicked: {
                    page.busy = false
                    core.cancel_ongoing()
                    pageStack.pop()
                }
            }

            // In progress bar's place: what failed, retry.
            Column {
                objectName: "failureBlock"
                visible: page.failed
                width: parent.width
                spacing: Theme.paddingLarge

                Label {
                    objectName: "failureTitle"
                    x: Theme.horizontalPageMargin
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    font.family: Theme.fontFamilyHeading
                    font.pixelSize: Theme.fontSizeLarge
                    color: Theme.errorColor
                    text: qsTr("That did not work")
                }

                Label {
                    objectName: "errorLabel"
                    x: Theme.horizontalPageMargin
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    color: Theme.highlightColor
                    text: page.errorMessage
                }

                Button {
                    objectName: "retryButton"
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: page.fromDevice ? qsTr("Try the code again")
                                          : qsTr("Choose another file")
                    onClicked: {
                        if (page.fromDevice) {
                            page.rescan()
                            page.errorMessage = ""
                        } else {
                            page.errorMessage = ""
                            page.chooseFile()
                        }
                    }
                }
            }
        }
    }
}
