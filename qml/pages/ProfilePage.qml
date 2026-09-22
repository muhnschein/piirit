import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Format.js" as Format
import Piirit 1.0

/*
 * One profile, as everyone else sees it and as this device holds it: the
 * picture, the name on every message, the line under it, the relays it
 * is reached through and the one it writes from, whether the other end
 * is told when something has been read, and how the relay and the phone
 * are doing by it. Reached from the profile's row on the profiles page.
 * The settings that belong to no profile are on the settings page
 * instead (SettingsPage.qml).
 *
 * This page is a profile's settings, so the two things about a profile
 * that are not settings are not here: the invite code to show and the backup to
 * write are on the profile's row on the profiles page, which is where
 * the profile is picked in the first place.
 *
 * The editable parts are core config keys rather than a record of their
 * own, so this page owns a Profile object over get_config/set_config
 * instead of a model. Nothing is confirmed: edits apply a moment after
 * typing stops and again on the way out, and the switch applies on the
 * tap. A settings page that needs saving is a settings page that loses
 * what was typed when someone swipes back.
 *
 * The name sits under the picture, a field (EditableName.qml). The
 * storage section follows parla's profile dialog (github.com/trufae/
 * parla), per relay: for each, the dot the core's own report draws for
 * its connection with the core's own words beside it, and the bar for
 * its mailbox, always there, at nothing until the relay has said, with
 * what is used of what there is under it. Before the relays, what the
 * profile takes on the phone. What parla's "Details" dialog adds --
 * the storage per conversation, scanned message by message -- is not
 * here: on a phone that scan is what the reader would be waiting on.
 *
 * The relays are a list rather than one address: the core lets a profile
 * be reached through several, each with an address of its own, and sends
 * from one of them (Transports, transports.rs). The one it sends from is
 * first and says so. A row's menu is what can be done with that relay --
 * send from it instead, or remove it, with Silica's countdown to change
 * one's mind in -- and the plus under the last row adds one
 * (AddRelayPage.qml). Removing the last relay is not offered: the core
 * refuses it, and a profile with no relay is not a profile. The profiles
 * page under this one draws its rows off the core's account list, so a
 * change here reads that list again, the way a saved name does.
 *
 * The countdown lives beside the list rather than on the row
 * (PendingRemoval), as the profiles page's and a group's do: the rows are
 * rebuilt whenever the relays are read again, which the core asks for
 * whenever the connection changes, and a countdown on a row would go
 * with it.
 */
Page {
    id: page

    property int accountId

    Profile {
        id: profile
        objectName: "profile"
        account_id: page.accountId
        onError: page.errorMessage = message
        // The fields are only filled from the core, never re-filled from
        // it while someone is typing into them: a save reloads, and that
        // would otherwise reach in and reset the cursor.
        onLoaded_changed: {
            if (profile.loaded && !page.edited) {
                // Assigning to a field fires onTextChanged, which is the
                // same signal the reader typing produces. Without this
                // the load itself would count as an edit, and leaving
                // the page would write the profile back over itself.
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
        // The profile sends from another relay now, or has one fewer:
        // its address, what the relay takes and the profiles page's row
        // follow.
        onChanged: {
            profile.reload()
            profile.refresh_connectivity()
            core.refresh_accounts()
        }
    }

    /// How long a relay waits before it goes, in milliseconds. Nothing
    /// sets it; a test turns it down rather than waiting.
    property alias pendingDelay: doomedRelays.delay

    /// The relays the reader has asked to remove, waiting out the moment
    /// in which they can say they did not mean it. The address travels
    /// as the tag: the row it was asked on may be gone by the time the
    /// wait is up.
    PendingRemoval {
        id: doomedRelays
        onRemove: transports.remove(tag)
    }

    // And as the page is actually destroyed, for the reason the profiles
    // page gives: "leaving" and "told it is leaving" are not the same
    // moment. Flushing twice costs nothing.
    Component.onDestruction: doomedRelays.flush()

    Connections {
        target: core
        // The core says when the connection changes; the mailbox, the
        // storage and each relay's own mailbox are re-read with it. It
        // says when the relays change too -- from here, or from another
        // device the profile is on -- and the list follows.
        onCore_event: {
            if (kind === "ConnectivityChanged" && context_id === page.accountId) {
                profile.refresh_connectivity()
                relayRefresh.restart()
            }
            // Changed on another device the profile is on: the row on
            // the profiles page names the relay sent from, and follows.
            if (kind === "TransportsModified" && context_id === page.accountId) {
                core.refresh_accounts()
            }
            transports.handle_event(context_id, kind, payload_json)
        }
    }

    // The core says the connection changed several times over when IO
    // restarts -- which is what sending from another relay does -- and
    // each time the rows are worth reading again for their mailboxes.
    // Once, when it has gone quiet: the rows are rebuilt on a read, and a
    // rebuild per event is a menu closed under the reader's finger per
    // event.
    Timer {
        id: relayRefresh
        interval: 1000
        onTriggered: transports.reload()
    }

    /// Someone has typed since the load. Guards the refill above.
    property bool edited: false
    /// The refill is writing to the fields, so the changes are not edits.
    property bool filling: false
    property string errorMessage: ""

    // A pause, not a keystroke: a round trip per letter would be four
    // calls to write "Ada".
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

    // Leaving is the other moment worth saving at: a back-swipe within
    // the pause above would otherwise drop what was typed. The cursor
    // leaves the field on the way out, so the keyboard does not follow
    // the page. Coming back -- from the page that adds a relay -- is
    // when the relays are worth reading again: the core announces the
    // addition as well, and this is the cheap belt to its braces.
    onStatusChanged: {
        if (status === PageStatus.Deactivating) {
            page.applyEdits()
            nameField.done()
            // Anything still waiting goes now: leaving is exactly when
            // a timer has not fired yet.
            doomedRelays.flush()
        } else if (status === PageStatus.Active) {
            transports.reload()
        }
    }

    // The gallery, pushed by URL and connected to, the way the
    // conversation attaches a photo: the Attach*Page files are the only
    // ones that name a `Sailfish.Pickers` type, so a type that is not
    // there costs this button rather than the page. That page also
    // ignores a cancelled pick, so the core is never handed an
    // `undefined` path.
    function pickPicture() {
        var picker = pageStack.push(Qt.resolvedUrl("AttachPhotoPage.qml"))
        if (picker) {
            picker.picked.connect(function(path) {
                // The core copies the file into its own blob directory,
                // so the picked one may go away afterwards.
                profile.set_picture(path)
            })
        }
    }

    /// The dot beside a relay's words, in the colours the core's own
    /// report draws its dots: green connected, yellow on the way, red
    /// not, and grey for a relay it has not reported on yet. The dot's
    /// name is the report's (transports.rs); the colours are the ones
    /// its stylesheet gives them.
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

    /// A size, with nothing at all said as "0 B" rather than as nothing.
    function size(bytes) {
        return bytes > 0 ? Format.readableSize(bytes) : "0 B"
    }

    /// What is under a relay's bar: the amounts when the relay gave
    /// them, the core's own sentence when it gave something this could
    /// not read, and the fact that it has not said yet otherwise.
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

        // Only there when there is a picture to remove, and kept off the
        // picture itself: a tap that might delete is not a tap you want
        // under a finger reaching for the gallery. The connection needs
        // no entry: the core says when it changes, and the page re-reads.
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

            // The picture is the control, not a preview of one: tapping
            // it opens the gallery, and the badge says so without a row
            // of its own.
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

            // The name on every message, under the picture. The field
            // is what is read and written; see EditableName.qml.
            // No hint under it: what the name is for is what the
            // heading says, and a line under the field read as a
            // subtitle to the reader rather than as help.
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
                // The core's own words for mdns_enabled are "should be
                // sent and requested", so this is not one-way: with it
                // off nothing is asked for either, and read marks stop
                // coming back from the people who would have sent them.
                description: qsTr("If read receipts are disabled, you won't be able to see read receipts from others.")
                // Bound to the profile, not held here: the core is what
                // decides, and a switch that drifts from it is a lie. So
                // the tap must not flip it either -- Silica does that by
                // default, which detaches the binding on the first tap and
                // leaves a refused change looking like it took. The tap
                // asks the core for the other state, and the binding
                // shows whatever the core then says.
                automaticCheck: false
                checked: profile.read_receipts
                enabled: profile.loaded
                onClicked: profile.set_read_receipts(!checked)
            }

            // The relays the profile is reached through: the reader's
            // own addresses, what the relays minted, and what tells two
            // profiles apart. The one sent from first. Shown, not edited
            // -- an address is a relay's, and changing one is adding a
            // relay and sending from it, which the rows and the plus
            // under them offer.
            SectionHeader {
                text: qsTr("Relays")
            }

            Repeater {
                id: relayRows
                objectName: "relayRows"
                model: transports.rows

                ListItem {
                    id: relayRow
                    // Named by place, so a test can find the first row
                    // whichever relay stands there now.
                    objectName: "relayRow" + index
                    width: column.width
                    contentHeight: relayBody.height + 2 * Theme.paddingSmall

                    /// The address on this relay, for a test to read.
                    readonly property string addr: model.addr
                    readonly property bool sendsFrom: model.is_primary
                    /// This relay is on its way out.
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
                            // The relay's string, pinned to plain.
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

                        // What sets the relay sent from apart. What the
                        // relays hold and how they are doing is in the
                        // section below, relay by relay.
                        Label {
                            objectName: "relayDetail"
                            width: parent.width
                            wrapMode: Text.Wrap
                            visible: model.is_primary
                            font.pixelSize: Theme.fontSizeSmall
                            color: Theme.secondaryHighlightColor
                            textFormat: Text.PlainText
                            text: qsTr("Sends from this relay")
                        }
                    }

                    /// Silica's own countdown, drawn over the relay
                    /// about to go. The removal is not its business --
                    /// that belongs to `doomedRelays`, because a remorse
                    /// item lives in the row it covers and the rows are
                    /// rebuilt whenever the relays are read again. So it
                    /// draws, and reports the tap.
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

                    // A row rebuilt mid-wait comes back with no countdown
                    // on it.
                    Component.onCompleted: {
                        if (relayRow.doomed) {
                            relayRow.raiseRemorse()
                        }
                    }

                    menu: ContextMenu {
                        // Not on the relay already sent from: there is
                        // nothing to switch to.
                        MenuItem {
                            objectName: "sendFromItem"
                            visible: !model.is_primary
                            text: qsTr("Send from this relay")
                            onClicked: transports.set_primary(model.addr)
                        }

                        // Dim on the last relay: the core refuses to
                        // remove it, and a menu item that asks anyway
                        // is a menu item that fails.
                        MenuItem {
                            objectName: "removeRelayItem"
                            enabled: transports.count > 1
                            text: qsTr("Remove relay")
                            // The page is told, not this row: the row is
                            // rebuilt whenever the list reloads, and a
                            // wait living on it would go too.
                            onClicked: {
                                doomedRelays.ask(model.id, model.addr)
                                relayRow.raiseRemorse()
                            }
                        }
                    }
                }
            }

            // One more relay, from the plus under the last row, where
            // the profiles page puts "add profile".
            ListItem {
                id: addRelayRow
                objectName: "addRelayButton"
                width: column.width
                contentHeight: Theme.itemSizeSmall + 2 * Theme.paddingMedium

                PlusMark {
                    id: plus
                    x: Theme.horizontalPageMargin
                    y: Theme.paddingMedium
                }

                Label {
                    x: plus.x + plus.width + Theme.paddingMedium
                    width: parent.width - x - Theme.horizontalPageMargin
                    anchors.verticalCenter: plus.verticalCenter
                    wrapMode: Text.Wrap
                    color: addRelayRow.highlighted ? Theme.highlightColor
                                                   : Theme.primaryColor
                    text: qsTr("Add a relay")
                }

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

            // Then each relay, in the order the rows above have them:
            // the connection, as a dot in the colour the core's own
            // report draws it and the core's own words beside it; and
            // the mailbox on the relay, a bar always there, at nothing
            // until the relay has said how full it is, with the words
            // under it saying what it said. Drawn here rather than
            // Silica's ProgressBar, whose track sits well inside its own
            // margins: this one runs the width of the text above it.
            Repeater {
                objectName: "relayReports"
                model: transports.rows

                Column {
                    id: relayReport
                    objectName: "relayReport" + index
                    x: Theme.horizontalPageMargin
                    width: column.width - 2 * Theme.horizontalPageMargin
                    spacing: Theme.paddingSmall

                    /// The relay reported on, for a test to read.
                    readonly property string domain: model.domain

                    Label {
                        objectName: "reportDomain"
                        width: parent.width
                        wrapMode: Text.WrapAnywhere
                        color: Theme.highlightColor
                        // The relay's string, pinned to plain.
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
                            // The core's own words, when it has said any.
                            textFormat: Text.PlainText
                            text: model.status.length > 0 ? model.status
                                                          : qsTr("Checking the connection")
                        }
                    }

                    Item {
                        id: reportQuota
                        objectName: "reportQuota"
                        /// Percent used, 0 to 100.
                        property real value: Math.min(100, model.quota_percent)
                        /// What is said under the bar.
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
                            // The relay's own words, when they are what is shown.
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
        // Quiet confirmation that something reached the core, since
        // nothing else on this page says so any more.
        onSaved: {
            notice.show(qsTr("Saved"))
            // The profiles page under this one draws its rows off the
            // core's account list, and the core does not say when a
            // name or a picture changes: it is re-read here, so the row
            // shows the new name on the swipe back rather than on the
            // next visit. Reconciled in place (core.rs), so the rows do
            // not flicker.
            core.refresh_accounts()
        }
    }
}
