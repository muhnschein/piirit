pragma Singleton
import QtQuick 2.0
import Nemo.Configuration 1.0

/*
 * The settings that belong to no profile: whether the return key sends,
 * how a message is drawn, what goes out with a link, how much of an
 * attachment arrives unasked, how long a message is kept, how much a
 * notification gives away and whether a muted group can still raise one,
 * and whether webxdc apps are offered at all.
 *
 * They live in dconf under the app's own path, and every page reads
 * them through this one object: the settings page (qml/pages/
 * SettingsPage.qml) writes it, and each value follows its key, so a
 * change made there reaches every open page without either side being
 * told. dconf rather than a property of the window, so the values
 * outlive the app the way a setting should.
 *
 * A singleton rather than a property handed down page by page: a message
 * row three components deep wants the same answer as the page that
 * sends, and the row is loaded on its own in a test.
 */
QtObject {
    /// Whether the return key sends the message. Off, it puts in a line
    /// break and the send button sends, which is what every other client
    /// on the phone does with a message longer than a remark; on, the
    /// key that would break a line sends instead, so a message written
    /// here is one line by construction.
    property alias enterSends: enterSendsValue.value
    /// 0 draws Markdown; anything else shows a message as written, the 1
    /// an older install may still hold included.
    property alias markdownMode: markdownValue.value
    /// Take known tracking parameters out of links before sending.
    property alias cleanLinks: cleanLinksValue.value
    /// Attachments bigger than this many bytes wait to be asked for; 0
    /// fetches everything. The core's own `download_limit`, applied to
    /// every profile.
    property alias downloadLimit: downloadLimitValue.value
    /// Messages older than this many seconds are deleted from the phone,
    /// in every chat of every profile, whatever a chat's own disappearing
    /// messages timer says; 0 keeps them. The core's own
    /// `delete_device_after`, applied to every profile the way the
    /// download limit is.
    property alias deleteDeviceAfter: deleteDeviceAfterValue.value
    /// How much a notification says: 0 who wrote and what, 1 who wrote,
    /// 2 only that something arrived. What the lock screen shows to
    /// whoever is looking at it.
    property alias notificationDetail: notificationDetailValue.value
    /// Whether a reply to one of the reader's own messages is announced
    /// even from a muted group. What the reference clients call mention
    /// notifications, and on by default as they have it.
    property alias mentionNotifications: mentionNotificationsValue.value
    /// The profile the chat list was last on, or 0 for a phone that has
    /// never had one.
    ///
    /// The core remembers this as well, and better -- it is the one that
    /// knows whether the profile still exists. But it only answers once
    /// it has started, and starting it is a process spawn and a round
    /// trip: long enough that a phone with a profile sat on a blank
    /// first screen waiting to be told what it already knew. This is
    /// read from dconf before any of that, so the app opens where it
    /// belongs and the core catches up behind it.
    property alias lastAccountId: lastAccountValue.value
    /// Whether webxdc apps are offered: the tray's app entry, the store
    /// behind it, and running one somebody sent. Off until it is asked
    /// for, and every one of those three reads this rather than deciding
    /// for itself.
    property alias webxdcEnabled: webxdcEnabledValue.value

    // The keys, named here and nowhere else: tests/qml_syntax.rs holds
    // every other file to reading them through this object.
    property ConfigurationValue enterSendsConfig: ConfigurationValue {
        id: enterSendsValue
        key: "/apps/harbour-postivene/enter_sends"
        // A line break, until the reader says otherwise: the field took
        // the return key for one before there was a choice, and a key
        // that sends by surprise sends half a message.
        defaultValue: false
    }

    property ConfigurationValue markdownConfig: ConfigurationValue {
        id: markdownValue
        key: "/apps/harbour-postivene/markdown_mode"
        defaultValue: 0
    }

    property ConfigurationValue cleanLinksConfig: ConfigurationValue {
        id: cleanLinksValue
        key: "/apps/harbour-postivene/clean_links"
        defaultValue: false
    }

    property ConfigurationValue downloadLimitConfig: ConfigurationValue {
        id: downloadLimitValue
        key: "/apps/harbour-postivene/download_limit"
        // One megabyte, as parla defaults it: a photo arrives, a video
        // waits to be asked for.
        defaultValue: 1048576
    }

    property ConfigurationValue deleteDeviceAfterConfig: ConfigurationValue {
        id: deleteDeviceAfterValue
        key: "/apps/harbour-postivene/delete_device_after"
        // Kept for good until the reader says otherwise, which is the
        // core's own default and the only one that loses nothing.
        defaultValue: 0
    }

    property ConfigurationValue notificationDetailConfig: ConfigurationValue {
        id: notificationDetailValue
        key: "/apps/harbour-postivene/notification_detail"
        defaultValue: 0
    }

    property ConfigurationValue mentionNotificationsConfig: ConfigurationValue {
        id: mentionNotificationsValue
        key: "/apps/harbour-postivene/mention_notifications"
        defaultValue: true
    }

    property ConfigurationValue lastAccountConfig: ConfigurationValue {
        id: lastAccountValue
        key: "/apps/harbour-postivene/last_account"
        // No profile until a chat list has been on one.
        defaultValue: 0
    }

    property ConfigurationValue webxdcEnabledConfig: ConfigurationValue {
        id: webxdcEnabledValue
        key: "/apps/harbour-postivene/webxdc_enabled"
        // Off on a phone that has never been asked. Running somebody
        // else's code, however sandboxed the engine is, is not a thing
        // to switch on for a reader who did not ask for it -- and the
        // half of it that is ours is new enough to still be finding out
        // what it gets wrong.
        defaultValue: false
    }
}
