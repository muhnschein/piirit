pragma Singleton
import QtQuick 2.0
import Nemo.Configuration 1.0

/*
 * The settings that belong to no profile: how a message is drawn, what
 * goes out with a link, how much of a picture or a video leaves with it,
 * how much of an attachment arrives unasked, how long a message is kept,
 * whether anything is announced at all and if so how much a notification
 * gives away and whether a muted group can still raise one, whether
 * webxdc apps are offered at all, and which quick actions the cover
 * offers.
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
    /// 0 draws Markdown; anything else shows a message as written, the 1
    /// an older install may still hold included.
    property alias markdownMode: markdownValue.value
    /// Take known tracking parameters out of links before sending.
    property alias cleanLinks: cleanLinksValue.value
    /// How much a picture or a video is compressed on its way out: 0
    /// balanced, 1 worse quality and less data. The core's own
    /// `media_quality`, applied to every profile, and what the camera
    /// records a video at. The reference clients' setting, with their
    /// two choices and their default.
    property alias mediaQuality: mediaQualityValue.value
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
    /// Whether anything is announced at all. Off, no message raises a
    /// notification whatever the two settings below say; they are kept
    /// for when it is on again.
    property alias notificationsEnabled: notificationsEnabledValue.value
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
    /// The cover's quick actions, the left one and the right: what each
    /// does -- "" for nothing, "chat", "search", "qr" or "scan" -- and, for
    /// a chat, the profile and chat it opens and the icon it wears.
    /// qml/js/QuickActions.js reads them.
    property alias quickActionLeft: quickActionLeftValue.value
    property alias quickActionLeftAccount: quickActionLeftAccountValue.value
    property alias quickActionLeftChat: quickActionLeftChatValue.value
    property alias quickActionLeftIcon: quickActionLeftIconValue.value
    property alias quickActionRight: quickActionRightValue.value
    property alias quickActionRightAccount: quickActionRightAccountValue.value
    property alias quickActionRightChat: quickActionRightChatValue.value
    property alias quickActionRightIcon: quickActionRightIconValue.value

    // The keys, named here and nowhere else: tests/qml_syntax.rs holds
    // every other file to reading them through this object.
    property ConfigurationValue markdownConfig: ConfigurationValue {
        id: markdownValue
        key: "/apps/harbour-piirit/markdown_mode"
        defaultValue: 0
    }

    property ConfigurationValue cleanLinksConfig: ConfigurationValue {
        id: cleanLinksValue
        key: "/apps/harbour-piirit/clean_links"
        defaultValue: false
    }

    property ConfigurationValue mediaQualityConfig: ConfigurationValue {
        id: mediaQualityValue
        key: "/apps/harbour-piirit/media_quality"
        // Balanced, which is the core's own default and the reference
        // clients'.
        defaultValue: 0
    }

    property ConfigurationValue downloadLimitConfig: ConfigurationValue {
        id: downloadLimitValue
        key: "/apps/harbour-piirit/download_limit"
        // One megabyte, as parla defaults it: a photo arrives, a video
        // waits to be asked for.
        defaultValue: 1048576
    }

    property ConfigurationValue deleteDeviceAfterConfig: ConfigurationValue {
        id: deleteDeviceAfterValue
        key: "/apps/harbour-piirit/delete_device_after"
        // Kept for good until the reader says otherwise, which is the
        // core's own default and the only one that loses nothing.
        defaultValue: 0
    }

    property ConfigurationValue notificationsEnabledConfig: ConfigurationValue {
        id: notificationsEnabledValue
        key: "/apps/harbour-piirit/notifications_enabled"
        // On: a chat client that says nothing when a message arrives is
        // not doing its job until it is asked to stop.
        defaultValue: true
    }

    property ConfigurationValue notificationDetailConfig: ConfigurationValue {
        id: notificationDetailValue
        key: "/apps/harbour-piirit/notification_detail"
        defaultValue: 0
    }

    property ConfigurationValue mentionNotificationsConfig: ConfigurationValue {
        id: mentionNotificationsValue
        key: "/apps/harbour-piirit/mention_notifications"
        defaultValue: true
    }

    property ConfigurationValue lastAccountConfig: ConfigurationValue {
        id: lastAccountValue
        key: "/apps/harbour-piirit/last_account"
        // No profile until a chat list has been on one.
        defaultValue: 0
    }

    property ConfigurationValue webxdcEnabledConfig: ConfigurationValue {
        id: webxdcEnabledValue
        key: "/apps/harbour-piirit/webxdc_enabled"
        // Off on a phone that has never been asked. Running somebody
        // else's code, however sandboxed the engine is, is not a thing
        // to switch on for a reader who did not ask for it -- and the
        // half of it that is ours is new enough to still be finding out
        // what it gets wrong.
        defaultValue: false
    }

    // None on a phone that has never been asked: the cover is the faces
    // alone until the reader puts something on it.
    property ConfigurationValue quickActionLeftConfig: ConfigurationValue {
        id: quickActionLeftValue
        key: "/apps/harbour-piirit/quick_action_left"
        defaultValue: ""
    }

    property ConfigurationValue quickActionLeftAccountConfig: ConfigurationValue {
        id: quickActionLeftAccountValue
        key: "/apps/harbour-piirit/quick_action_left_account"
        defaultValue: 0
    }

    property ConfigurationValue quickActionLeftChatConfig: ConfigurationValue {
        id: quickActionLeftChatValue
        key: "/apps/harbour-piirit/quick_action_left_chat"
        defaultValue: 0
    }

    property ConfigurationValue quickActionLeftIconConfig: ConfigurationValue {
        id: quickActionLeftIconValue
        key: "/apps/harbour-piirit/quick_action_left_icon"
        defaultValue: ""
    }

    property ConfigurationValue quickActionRightConfig: ConfigurationValue {
        id: quickActionRightValue
        key: "/apps/harbour-piirit/quick_action_right"
        defaultValue: ""
    }

    property ConfigurationValue quickActionRightAccountConfig: ConfigurationValue {
        id: quickActionRightAccountValue
        key: "/apps/harbour-piirit/quick_action_right_account"
        defaultValue: 0
    }

    property ConfigurationValue quickActionRightChatConfig: ConfigurationValue {
        id: quickActionRightChatValue
        key: "/apps/harbour-piirit/quick_action_right_chat"
        defaultValue: 0
    }

    property ConfigurationValue quickActionRightIconConfig: ConfigurationValue {
        id: quickActionRightIconValue
        key: "/apps/harbour-piirit/quick_action_right_icon"
        defaultValue: ""
    }
}
