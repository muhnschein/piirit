pragma Singleton
import QtQuick 2.0
import Nemo.Configuration 1.0

/*
 * Profile-less settings: drawing, links, media, notifications, calls, cover,
 * camera; plus lens labels, read never written. dconf under app path (outlives
 * app). qml/pages/SettingsPage.qml writes; every page reads through this object,
 * each value follows its key, so changes reach open pages untold. Singleton: a
 * row three components deep, loaded alone in tests, needs the same answer.
 */
QtObject {
    /// 0 draws Markdown; else shows message as written (1 = old installs).
    property alias markdownMode: markdownValue.value
    /// Strip known tracking parameters from links before sending.
    property alias cleanLinks: cleanLinksValue.value
    /// Media compression on send: 0 balanced, 1 less data. Core's
    /// `media_quality`, all profiles; camera records video at it too.
    property alias mediaQuality: mediaQualityValue.value
    /// Attachments above this many bytes wait to be asked for; 0 = all. Core's
    /// `download_limit`, all profiles.
    property alias downloadLimit: downloadLimitValue.value
    /// Delete messages older than this many s from the phone, all chats, ignoring
    /// chat timers; 0 keeps. Core's `delete_device_after`, all profiles.
    property alias deleteDeviceAfter: deleteDeviceAfterValue.value
    /// Off: no notifications, whatever below says. Kept for re-enable.
    property alias notificationsEnabled: notificationsEnabledValue.value
    /// Notification detail: 0 who and what, 1 who, 2 only arrival. Lock screen
    /// shows it.
    property alias notificationDetail: notificationDetailValue.value
    /// Reply to own message announced even from muted group (on by default).
    property alias mentionNotifications: mentionNotificationsValue.value
    /// Last chat-list profile, 0 = none. Read before core starts (slow to
    /// answer).
    property alias lastAccountId: lastAccountValue.value
    /// Webxdc apps offered (tray, store, running). Off until asked.
    property alias webxdcEnabled: webxdcEnabledValue.value
    /// Contact calls ring here. Core's `who_can_call_me`, all profiles. Calls on for all
    /// since #119. Old key /apps/harbour-piirit/calls_enabled no longer read.
    property alias callsRing: callsRingValue.value
    /// Cover quick-action slots: 1 or 2. With 1, left is used; right kept for
    /// later.
    property alias quickActionCount: quickActionCountValue.value
    /// Cover quick actions, left and right. Kind: "" none, "chat", "search",
    /// "qr", "scan", "profiles", or camera "photo", "video", "selfie",
    /// "selfie-video". Chat: profile, chat, icon. Camera: send at once on chat
    /// pick. Read by qml/js/QuickActions.js.
    property alias quickActionLeft: quickActionLeftValue.value
    property alias quickActionLeftAccount: quickActionLeftAccountValue.value
    property alias quickActionLeftChat: quickActionLeftChatValue.value
    property alias quickActionLeftIcon: quickActionLeftIconValue.value
    property alias quickActionLeftSendNow: quickActionLeftSendNowValue.value
    property alias quickActionRight: quickActionRightValue.value
    property alias quickActionRightAccount: quickActionRightAccountValue.value
    property alias quickActionRightChat: quickActionRightChatValue.value
    property alias quickActionRightIcon: quickActionRightIconValue.value
    property alias quickActionRightSendNow: quickActionRightSendNowValue.value
    /// Recent reactions, newest first, offered before all. Picker only.
    property alias recentReactions: recentReactionsValue.value
    /// Camera settings, as platform camera keeps them: flash mode (see
    /// qml/js/Viewfinder.js), self-timer seconds, grid on.
    property alias cameraFlash: cameraFlashValue.value
    property alias cameraTimer: cameraTimerValue.value
    property alias cameraGrid: cameraGridValue.value
    /// Back lens labels platform camera writes ("1.0", "0.5"). Read-only; empty
    /// if none.
    readonly property var backLensLabels: backLensLabelsValue.value || []

    // Keys named here only: tests/qml_syntax.rs holds other files to reading
    // via this object.
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
        // Balanced: core's and reference clients' default.
        defaultValue: 0
    }

    property ConfigurationValue downloadLimitConfig: ConfigurationValue {
        id: downloadLimitValue
        key: "/apps/harbour-piirit/download_limit"
        // 1 MB, as parla: photo arrives, video waits.
        defaultValue: 1048576
    }

    property ConfigurationValue deleteDeviceAfterConfig: ConfigurationValue {
        id: deleteDeviceAfterValue
        key: "/apps/harbour-piirit/delete_device_after"
        // Kept forever until changed: core's default, loses nothing.
        defaultValue: 0
    }

    property ConfigurationValue notificationsEnabledConfig: ConfigurationValue {
        id: notificationsEnabledValue
        key: "/apps/harbour-piirit/notifications_enabled"
        // On: chat client silent on arrival is not doing its job.
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
        // Off until asked. Others' code, however sandboxed, is not for unasked
        // readers; our half is new and still finding bugs.
        defaultValue: false
    }

    property ConfigurationValue callsRingConfig: ConfigurationValue {
        id: callsRingValue
        key: "/apps/harbour-piirit/calls_ring"
        defaultValue: true
    }

    // One, and none on fresh phone: cover is faces alone until user adds
    // something.
    property ConfigurationValue quickActionCountConfig: ConfigurationValue {
        id: quickActionCountValue
        key: "/apps/harbour-piirit/quick_action_count"
        defaultValue: 1
    }

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

    property ConfigurationValue quickActionLeftSendNowConfig: ConfigurationValue {
        id: quickActionLeftSendNowValue
        key: "/apps/harbour-piirit/quick_action_left_send_now"
        defaultValue: false
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

    property ConfigurationValue quickActionRightSendNowConfig: ConfigurationValue {
        id: quickActionRightSendNowValue
        key: "/apps/harbour-piirit/quick_action_right_send_now"
        defaultValue: false
    }

    property ConfigurationValue recentReactionsConfig: ConfigurationValue {
        id: recentReactionsValue
        key: "/apps/harbour-piirit/recent_reactions"
        defaultValue: []
    }

    property ConfigurationValue cameraFlashConfig: ConfigurationValue {
        id: cameraFlashValue
        key: "/apps/harbour-piirit/camera_flash"
        // Automatic: platform camera default.
        defaultValue: 1
    }

    property ConfigurationValue cameraTimerConfig: ConfigurationValue {
        id: cameraTimerValue
        key: "/apps/harbour-piirit/camera_timer"
        defaultValue: 0
    }

    property ConfigurationValue cameraGridConfig: ConfigurationValue {
        id: cameraGridValue
        key: "/apps/harbour-piirit/camera_grid"
        defaultValue: false
    }

    property ConfigurationValue backLensLabelsConfig: ConfigurationValue {
        id: backLensLabelsValue
        key: "/apps/jolla-camera/backCameraLabels"
        defaultValue: []
    }
}
