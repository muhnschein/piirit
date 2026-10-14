import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Format.js" as Format
import Piirit 1.0

/*
 * One profile's settings: picture, name, status line, relays, read receipts,
 * relay and phone storage. From profile row on profiles page (invite code,
 * backup live there); profile-less settings on SettingsPage.qml. Core config
 * keys via Profile object (get_config/set_config), no model. No save button:
 * edits apply after pause and on exit, switch on tap.
 *
 * Storage follows parla's profile dialog (github.com/trufae/parla): phone use,
 * then per relay a connection dot with core's words and a mailbox bar (0 until
 * relay says). No per-chat storage scan: too slow on phone.
 * Relays: profile reachable via several, each own address (Transports,
 * transports.rs); one with profile's own (invite) address first. Outgoing
 * relay is core's per-connect choice, unreported: no row claims it. Menu
 * removes (countdown); plus adds (AddRelayPage.qml). Last relay not
 * removable: core refuses. Changes reread core account list for profiles
 * page. Countdown beside list (PendingRemoval): rows rebuild on every relay
 * read.
 */
Page {
    id: page

    property int accountId

    Profile {
        id: profile
        objectName: "profile"
        account_id: page.accountId
        onError: page.errorMessage = message
        // Filled from core, not while typing: a save reloads and would reset
        // cursor.
        onLoaded_changed: {
            if (profile.loaded && !page.edited) {
                // Assigning fires onTextChanged; else load counts as edit,
                // writes back.
                page.filling = true
                nameField.text = profile.display_name
                bioField.text = profile.status
                page.filling = false
            }
        }
    }

    Transports {
        id: transports
        objectName: "transports"
        account_id: page.accountId
        onError: page.errorMessage = message
        // One relay fewer: address (may have gone with it), storage and
        // profiles page row follow.
        onChanged: {
            profile.reload()
            profile.refresh_connectivity()
            core.refresh_accounts()
        }
    }

    /// Relay wait before removal, ms; tests lower it.
    property alias pendingDelay: doomedRelays.delay

    /// Relays asked to remove, undo window. Address is tag: row may be gone.
    PendingRemoval {
        id: doomedRelays
        onRemove: transports.remove(tag)
    }

    // Destroy as well as leave: "leaving" and "told it is leaving" differ
    // (see profiles page). Flushing twice is harmless.
    Component.onDestruction: doomedRelays.flush()

    Connections {
        target: core
        // Connection change: mailbox, storage, relay mailboxes re-read. Relay
        // change (here or other device) too.
        onCore_event: {
            if (kind === "ConnectivityChanged" && context_id === page.accountId) {
                connectivityRefresh.start()
                relayRefresh.restart()
            }
            // Other device changed relays: own-address relay may be gone; core
            // moves address.
            if (kind === "TransportsModified" && context_id === page.accountId) {
                profile.reload()
                core.refresh_accounts()
            }
            transports.handle_event(context_id, kind, payload_json)
        }
    }

    // Several connection events on IO restart: reread once when quiet, not per
    // event (a rebuild per event closes menu under finger).
    Timer {
        id: relayRefresh
        interval: 1000
        onTriggered: transports.reload()
    }

    // Same events for connection, mailbox, storage (four round trips, one sizes
    // every file): at most once a second. Started, not restarted: nothing under
    // finger, dot worth following meanwhile. First read on open.
    Timer {
        id: connectivityRefresh
        interval: 1000
        onTriggered: profile.refresh_connectivity()
    }

    /// Typed since load. Guards refill.
    property bool edited: false
    /// Refill writing fields, not edits.
    property bool filling: false
    property string errorMessage: ""

    // Pause, not keystroke: round trip per letter.
    Timer {
        id: autosave
        objectName: "autosave"
        interval: 1200
        onTriggered: page.applyEdits()
    }

    function applyEdits() {
        if (!profile.loaded || !page.edited) {
            return
        }
        page.edited = false
        profile.save(nameField.text, bioField.text)
    }

    function noteEdit() {
        if (profile.loaded && !page.filling) {
            page.edited = true
            autosave.restart()
        }
    }

    // Leaving saves too: back-swipe in pause drops typing. Cursor leaves field
    // so keyboard does not follow. Back from relay-add page rereads relays.
    onStatusChanged: {
        if (status === PageStatus.Deactivating) {
            page.applyEdits()
            nameField.done()
            // Pending removals go now; timer may not have fired.
            doomedRelays.flush()
        } else if (status === PageStatus.Active) {
            transports.reload()
        }
    }

    // Gallery via URL, as conversation attaches photo. Only Attach*Page files
    // name `Sailfish.Pickers`; missing type costs button. That page ignores a
    // cancelled pick, so core never gets `undefined` path.
    function pickPicture() {
        var picker = pageStack.push(Qt.resolvedUrl("AttachPhotoPage.qml"))
        if (picker) {
            picker.picked.connect(function(path) {
                // Core copies file to blob dir; picked file may vanish.
                profile.set_picture(path)
            })
        }
    }

    /// Relay dot in core report colours: green connected, yellow working, red
    /// not, grey unreported. Names from transports.rs.
    function dotColor(dot) {
        if (dot === "green") {
            return "#34c759"
        }
        if (dot === "yellow") {
            return "#fdc625"
        }
        if (dot === "red") {
            return "#f33b2d"
        }
        return Theme.secondaryColor
    }

    /// Size; zero shown as "0 B", not blank.
    function size(bytes) {
        return bytes > 0 ? Format.readableSize(bytes) : "0 B"
    }

    /// Text under relay bar: amounts, else core's sentence, else "not said
    /// yet".
    function quotaWords(usedBytes, limitBytes, text) {
        if (limitBytes > 0) {
            //: The mailbox on the relay. %1 used, %2 the whole, each a size such as "1.4 GB".
            return qsTr("%1 of %2 used")
                .arg(page.size(usedBytes))
                .arg(page.size(limitBytes))
        }
        if (text.length > 0) {
            return text
        }
        return qsTr("The relay has not reported its quota yet")
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        // Only with picture to remove; off the picture (no delete tap under
        // finger).
        PullDownMenu {
            visible: profile.avatar_path.length > 0

            MenuItem {
                objectName: "removePicture"
                text: qsTr("Remove picture")
                onClicked: profile.clear_picture()
            }
        }

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: qsTr("Profile")
            }

            // Picture is the control: tap opens gallery; badge says so.
            Item {
                width: parent.width
                height: bigAvatar.height + 2 * Theme.paddingLarge

                Avatar {
                    id: bigAvatar
                    objectName: "profileAvatar"
                    anchors.centerIn: parent
                    width: 2 * Theme.itemSizeExtraLarge
                    initial: nameField.text.length > 0
                             ? nameField.text
                             : profile.address
                    picturePath: profile.avatar_path
                }

                Rectangle {
                    id: editBadge
                    objectName: "editBadge"
                    anchors {
                        right: bigAvatar.right
                        bottom: bigAvatar.bottom
                    }
                    width: Theme.itemSizeExtraSmall
                    height: width
                    radius: width / 2
                    color: Theme.highlightBackgroundColor

                    Image {
                        anchors.centerIn: parent
                        source: "image://theme/icon-s-edit"
                    }
                }

                MouseArea {
                    objectName: "pictureTap"
                    anchors.fill: bigAvatar
                    onClicked: page.pickPicture()
                }
            }

            // Name under picture (EditableName.qml). No hint: heading says what
            // it is for.
            EditableName {
                id: nameField
                objectName: "profileNameControl"
                fieldObjectName: "profileNameField"
                hintObjectName: "nameHint"
                placeholderText: qsTr("Your name")
                onTextChanged: page.noteEdit()
            }

            TextField {
                id: bioField
                objectName: "profileBioField"
                width: parent.width
                label: qsTr("Bio")
                placeholderText: qsTr("A line about you")
                onTextChanged: page.noteEdit()
            }

            TextSwitch {
                objectName: "readReceiptsSwitch"
                text: qsTr("Send read receipts")
                // Core: mdns_enabled is two-way; off stops read marks coming
                // back.
                description: qsTr("If read receipts are disabled, you won't be able to see read receipts from others.")
                // Not held here: core decides. Tap must not flip it (Silica
                // would detach binding). Tap asks core; binding shows answer.
                automaticCheck: false
                checked: profile.read_receipts
                enabled: profile.loaded
                onClicked: profile.set_read_receipts(!checked)
            }

            // Relay addresses, minted by relays, and what tells profiles apart.
            // Not edited: change = add + remove.
            SectionHeader {
                text: qsTr("Relays")
            }

            Repeater {
                id: relayRows
                objectName: "relayRows"
                model: transports.rows

                ListItem {
                    id: relayRow
                    // Named by place: test finds first row.
                    objectName: "relayRow" + index
                    width: column.width
                    contentHeight: relayBody.height + 2 * Theme.paddingSmall

                    /// Address on this relay, for tests.
                    readonly property string addr: model.addr
                    /// Relay on its way out.
                    readonly property bool doomed: doomedRelays.pending(model.id)

                    Column {
                        id: relayBody
                        x: Theme.horizontalPageMargin
                        y: Theme.paddingSmall
                        width: parent.width - 2 * Theme.horizontalPageMargin

                        Label {
                            objectName: "relayDomain"
                            width: parent.width
                            wrapMode: Text.WrapAnywhere
                            color: relayRow.highlighted ? Theme.highlightColor
                                                        : Theme.primaryColor
                            // Relay string, pinned plain.
                            textFormat: Text.PlainText
                            text: model.domain
                        }

                        Label {
                            objectName: "relayAddress"
                            width: parent.width
                            wrapMode: Text.WrapAnywhere
                            font.pixelSize: Theme.fontSizeSmall
                            color: Theme.secondaryColor
                            textFormat: Text.PlainText
                            text: model.addr
                        }
                    }

                    /// Silica countdown over relay. Removal in `doomedRelays`:
                    /// rows rebuild on relay read. Draws, reports tap.
                    function raiseRemorse() {
                        //: What Silica's countdown says it is doing, over a
                        //: relay the reader has asked to remove.
                        relayRemorse.execute(
                            relayBody, qsTr("Removing relay"), function() {},
                            doomedRelays.countdownFor(model.id))
                    }

                    RemorseItem {
                        id: relayRemorse
                        objectName: "relayRemorse"
                        onCanceled: doomedRelays.spare(model.id)
                    }

                    // Row rebuilt mid-wait: no countdown.
                    Component.onCompleted: {
                        if (relayRow.doomed) {
                            relayRow.raiseRemorse()
                        }
                    }

                    menu: ContextMenu {
                        // Dim on last relay: core refuses.
                        MenuItem {
                            objectName: "removeRelayItem"
                            enabled: transports.count > 1
                            text: qsTr("Remove relay")
                            // Page told, not row: rebuilt on reload.
                            onClicked: {
                                doomedRelays.ask(model.id, model.addr)
                                relayRow.raiseRemorse()
                            }
                        }
                    }
                }
            }

            // One more relay.
            PlusRow {
                objectName: "addRelayButton"
                width: column.width
                text: qsTr("Add a relay")
                onClicked: pageStack.push(Qt.resolvedUrl("AddRelayPage.qml"),
                                          { accountId: page.accountId })
            }

            SectionHeader {
                text: qsTr("Storage and connectivity")
            }

            Label {
                objectName: "storageLabel"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                visible: profile.storage_bytes > 0
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                textFormat: Text.PlainText
                //: How much room the profile takes on the phone. %1 is a size such as "12.3 MB".
                text: qsTr("Piirit uses %1 of storage on this phone.")
                      .arg(Format.readableSize(profile.storage_bytes))
            }

            // Relay row: connection dot and mailbox bar. Drawn, not ProgressBar
            // (its track sits inside margins).
            Repeater {
                objectName: "relayReports"
                model: transports.rows

                Column {
                    id: relayReport
                    objectName: "relayReport" + index
                    x: Theme.horizontalPageMargin
                    width: column.width - 2 * Theme.horizontalPageMargin
                    spacing: Theme.paddingSmall

                    /// Relay reported on, for tests.
                    readonly property string domain: model.domain

                    Label {
                        objectName: "reportDomain"
                        width: parent.width
                        wrapMode: Text.WrapAnywhere
                        color: Theme.highlightColor
                        // Relay string, pinned plain.
                        textFormat: Text.PlainText
                        text: model.domain
                    }

                    Row {
                        width: parent.width
                        spacing: Theme.paddingMedium

                        Rectangle {
                            objectName: "reportDot"
                            anchors.verticalCenter: parent.verticalCenter
                            width: Theme.paddingLarge
                            height: width
                            radius: width / 2
                            color: page.dotColor(model.dot)
                        }

                        Label {
                            objectName: "reportStatus"
                            width: parent.width - Theme.paddingLarge - Theme.paddingMedium
                            wrapMode: Text.Wrap
                            font.pixelSize: Theme.fontSizeSmall
                            color: Theme.highlightColor
                            // Core's words when any. Not connected (IO
                            // stopped): core has none for any relay.
                            textFormat: Text.PlainText
                            text: model.status.length > 0 ? model.status
                                  : model.offline ? qsTr("Not connected")
                                  : qsTr("Checking the connection")
                        }
                    }

                    Item {
                        id: reportQuota
                        objectName: "reportQuota"
                        /// Percent used, 0 to 100.
                        property real value: Math.min(100, model.quota_percent)
                        /// Text under bar.
                        property string label: page.quotaWords(model.quota_used_bytes,
                                                               model.quota_limit_bytes,
                                                               model.quota_text)
                        width: parent.width
                        height: track.height + Theme.paddingSmall + quotaLabel.height

                        Rectangle {
                            id: track
                            width: parent.width
                            height: Theme.paddingSmall
                            radius: height / 2
                            color: Theme.rgba(Theme.primaryColor, 0.2)

                            Rectangle {
                                objectName: "quotaFill"
                                width: Math.round(parent.width * reportQuota.value / 100)
                                height: parent.height
                                radius: height / 2
                                color: Theme.highlightColor
                            }
                        }

                        Label {
                            id: quotaLabel
                            objectName: "quotaLabel"
                            anchors {
                                top: track.bottom
                                topMargin: Theme.paddingSmall
                            }
                            width: parent.width
                            wrapMode: Text.Wrap
                            font.pixelSize: Theme.fontSizeSmall
                            color: Theme.secondaryHighlightColor
                            // Relay's own words when shown.
                            textFormat: Text.PlainText
                            text: reportQuota.label
                        }
                    }
                }
            }
        }
    }

    Banner {
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
        timeout: 2
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        onDismissed: notice.text = ""
    }

    Connections {
        target: profile
        // Quiet confirmation: something reached core.
        onSaved: {
            notice.show(qsTr("Saved"))
            // Profiles page rows come from core account list, which doesn't
            // signal name/picture change: reread so swipe back shows it.
            // Reconciled in place (core.rs), no flicker.
            core.refresh_accounts()
        }
    }
}
