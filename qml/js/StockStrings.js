// The text the core writes itself, in the reader's language: what a chat
// list says of a draft, the line a group shows when someone joins it, the
// errors it reports. The core has its own English for every one of these
// and takes a translation for each by number (its `StockMessage` ids,
// chatmail/core src/stock_str.rs); DeltaChatCore.set_stock_strings hands
// this table over.
//
// The English is Delta Chat's own, string for string, so the catalogs
// carry Delta Chat's translations of it (docs/PROJECT.md, "The words on
// screen"). Which string answers which id is Delta Chat Android's choice,
// in DcHelper.setStockTranslations, and so are its departures from the
// core's English. Where Android says "Tap to learn more", which nothing
// here offers, the plain string is used (170), or none at all (174).
// The calls (232 to 235) keep their direction, which the core's own
// English has and Android's shorter one does not.
//
// No `.pragma library`: qsTr() needs the context of a component, as in
// Format.js.

/// Every stock string, by the core's id.
function all() {
    return {
        //: Written by the core: Delta Chat's "chat_no_messages".
        1: qsTr("No messages."),
        //: Written by the core: Delta Chat's "self".
        2: qsTr("Me"),
        //: Written by the core: Delta Chat's "draft".
        3: qsTr("Draft"),
        //: Written by the core: Delta Chat's "voice_message".
        7: qsTr("Voice Message"),
        //: Written by the core: Delta Chat's "image".
        9: qsTr("Image"),
        //: Written by the core: Delta Chat's "video".
        10: qsTr("Video"),
        //: Written by the core: Delta Chat's "audio".
        11: qsTr("Audio"),
        //: Written by the core: Delta Chat's "file".
        12: qsTr("File"),
        //: Written by the core: Delta Chat's "gif".
        23: qsTr("GIF"),
        //: Written by the core: Delta Chat's "contact_verified".
        35: qsTr("%1$s introduced."),
        //: Written by the core: Delta Chat's "chat_archived_label".
        40: qsTr("Archived"),
        //: Written by the core: Delta Chat's "location".
        66: qsTr("Location"),
        //: Written by the core: Delta Chat's "sticker".
        67: qsTr("Sticker"),
        //: Written by the core: Delta Chat's "device_talk".
        68: qsTr("Device Messages"),
        //: Written by the core: Delta Chat's "saved_messages".
        69: qsTr("Saved Messages"),
        //: Written by the core: Delta Chat's "device_talk_explain".
        70: qsTr("Messages in this chat are generated on your device to inform about app updates and problems during usage."),
        //: Written by the core: Delta Chat's "device_talk_welcome_message2".
        71: qsTr("Get in contact!\n\n🙌 Tap \"QR code\" on the main screen of both devices. Choose \"Scan QR Code\" on one device, and point it at the other\n\n🌍 If not in the same room, scan via video call or share an invite link from \"Scan QR code\"\n\nThen: Enjoy your decentralized messenger experience. In contrast to other popular apps, without central control or tracking or selling you, friends, colleagues or family out to large organizations."),
        //: Written by the core: Delta Chat's "systemmsg_subject_for_new_contact".
        73: qsTr("Message from %1$s"),
        //: Written by the core: Delta Chat's "systemmsg_failed_sending_to".
        74: qsTr("Failed to send message to %1$s."),
        //: Written by the core: Delta Chat's "configuration_failed_with_error".
        84: qsTr("Configuration failed. Error: %1$s"),
        //: Written by the core: Delta Chat's "devicemsg_bad_time".
        85: qsTr("⚠️ Date or time on your device seems to be inaccurate (%1$s).\n\nAdjust your clock ⏰🔧 to ensure your messages are received correctly."),
        //: Written by the core: Delta Chat's "devicemsg_update_reminder".
        86: qsTr("⚠️ Your Delta Chat version might be outdated.\n\nThis may cause problems because your chat partners use newer versions - and you are missing the latest features 😳\nPlease check https://get.delta.chat or your app store for updates."),
        //: Written by the core: Delta Chat's "reply_noun".
        90: qsTr("Reply"),
        //: Written by the core: Delta Chat's "devicemsg_self_deleted".
        91: qsTr("You deleted the \"Saved messages\" chat.\n\nℹ️ To use the \"Saved messages\" feature again, create a new chat with yourself."),
        //: Written by the core: Delta Chat's "forwarded".
        97: qsTr("Forwarded"),
        //: Written by the core: Delta Chat's "incoming_messages".
        103: qsTr("Incoming Messages"),
        //: Written by the core: Delta Chat's "outgoing_messages".
        104: qsTr("Outgoing Messages"),
        //: Written by the core: Delta Chat's "connectivity_connected".
        107: qsTr("Connected"),
        //: Written by the core: Delta Chat's "connectivity_connecting".
        108: qsTr("Connecting…"),
        //: Written by the core: Delta Chat's "connectivity_updating".
        109: qsTr("Updating…"),
        //: Written by the core: Delta Chat's "sending".
        110: qsTr("Sending…"),
        //: Written by the core: Delta Chat's "last_msg_sent_successfully".
        111: qsTr("Last message sent successfully."),
        //: Written by the core: Delta Chat's "error_x".
        112: qsTr("Error: %1$s"),
        //: Written by the core: Delta Chat's "messages".
        114: qsTr("Messages"),
        //: Written by the core: Delta Chat's "part_of_total_used".
        116: qsTr("%1$s of %2$s used"),
        //: Written by the core: Delta Chat's "secure_join_started".
        117: qsTr("%1$s invited you to join this group.\n\nWaiting for the device of %2$s to reply…"),
        //: Written by the core: Delta Chat's "secure_join_replies".
        118: qsTr("%1$s replied, waiting for being added to the group…"),
        //: Written by the core: Delta Chat's "qrshow_join_contact_hint".
        119: qsTr("Scan to chat with %1$s"),
        //: Written by the core: Delta Chat's "qrshow_join_group_hint".
        120: qsTr("Scan to join group %1$s"),
        //: Written by the core: Delta Chat's "connectivity_not_connected".
        121: qsTr("Not connected"),
        //: Written by the core: Delta Chat's "group_name_changed_by_you".
        124: qsTr("You changed the group name from \"%1$s\" to \"%2$s\"."),
        //: Written by the core: Delta Chat's "group_name_changed_by_other".
        125: qsTr("Group name changed from \"%1$s\" to \"%2$s\" by %3$s."),
        //: Written by the core: Delta Chat's "group_image_changed_by_you".
        126: qsTr("You changed the group image."),
        //: Written by the core: Delta Chat's "group_image_changed_by_other".
        127: qsTr("Group image changed by %1$s."),
        //: Written by the core: Delta Chat's "add_member_by_you".
        128: qsTr("You added member %1$s."),
        //: Written by the core: Delta Chat's "add_member_by_other".
        129: qsTr("Member %1$s added by %2$s."),
        //: Written by the core: Delta Chat's "remove_member_by_you".
        130: qsTr("You removed member %1$s."),
        //: Written by the core: Delta Chat's "remove_member_by_other".
        131: qsTr("Member %1$s removed by %2$s."),
        //: Written by the core: Delta Chat's "group_left_by_you".
        132: qsTr("You left the group."),
        //: Written by the core: Delta Chat's "group_left_by_other".
        133: qsTr("Group left by %1$s."),
        //: Written by the core: Delta Chat's "group_image_deleted_by_you".
        134: qsTr("You deleted the group image."),
        //: Written by the core: Delta Chat's "group_image_deleted_by_other".
        135: qsTr("Group image deleted by %1$s."),
        //: Written by the core: Delta Chat's "location_enabled_by_you".
        136: qsTr("You enabled location streaming."),
        //: Written by the core: Delta Chat's "location_enabled_by_other".
        137: qsTr("Location streaming enabled by %1$s."),
        //: Written by the core: Delta Chat's "ephemeral_timer_disabled_by_you".
        138: qsTr("You disabled disappearing messages timer."),
        //: Written by the core: Delta Chat's "ephemeral_timer_disabled_by_other".
        139: qsTr("Disappearing messages timer disabled by %1$s."),
        //: Written by the core: Delta Chat's "ephemeral_timer_seconds_by_you".
        140: qsTr("You set disappearing messages timer to %1$s seconds"),
        //: Written by the core: Delta Chat's "ephemeral_timer_seconds_by_other".
        141: qsTr("Disappearing messages timer set to %1$s seconds by %2$s."),
        //: Written by the core: Delta Chat's "ephemeral_timer_1_hour_by_you".
        144: qsTr("You set disappearing messages timer to 1 hour."),
        //: Written by the core: Delta Chat's "ephemeral_timer_1_hour_by_other".
        145: qsTr("Disappearing messages timer set to 1 hour by %1$s."),
        //: Written by the core: Delta Chat's "ephemeral_timer_1_day_by_you".
        146: qsTr("You set disappearing messages timer to 1 day."),
        //: Written by the core: Delta Chat's "ephemeral_timer_1_day_by_other".
        147: qsTr("Disappearing messages timer set to 1 day by %1$s."),
        //: Written by the core: Delta Chat's "ephemeral_timer_1_week_by_you".
        148: qsTr("You set disappearing messages timer to 1 week."),
        //: Written by the core: Delta Chat's "ephemeral_timer_1_week_by_other".
        149: qsTr("Disappearing messages timer set to 1 week by %1$s."),
        //: Written by the core: Delta Chat's "ephemeral_timer_minutes_by_you".
        150: qsTr("You set disappearing messages timer to %1$s minutes."),
        //: Written by the core: Delta Chat's "ephemeral_timer_minutes_by_other".
        151: qsTr("Disappearing messages timer set to %1$s minutes by %2$s."),
        //: Written by the core: Delta Chat's "ephemeral_timer_hours_by_you".
        152: qsTr("You set disappearing messages timer to %1$s hours."),
        //: Written by the core: Delta Chat's "ephemeral_timer_hours_by_other".
        153: qsTr("Disappearing messages timer set to %1$s hours by %2$s."),
        //: Written by the core: Delta Chat's "ephemeral_timer_days_by_you".
        154: qsTr("You set disappearing messages timer to %1$s days."),
        //: Written by the core: Delta Chat's "ephemeral_timer_days_by_other".
        155: qsTr("Disappearing messages timer set to %1$s days by %2$s."),
        //: Written by the core: Delta Chat's "ephemeral_timer_weeks_by_you".
        156: qsTr("You set disappearing messages timer to %1$s weeks."),
        //: Written by the core: Delta Chat's "ephemeral_timer_weeks_by_other".
        157: qsTr("Disappearing messages timer set to %1$s weeks by %2$s."),
        //: Written by the core: Delta Chat's "ephemeral_timer_1_year_by_you".
        158: qsTr("You set disappearing messages timer to 1 year."),
        //: Written by the core: Delta Chat's "ephemeral_timer_1_year_by_other".
        159: qsTr("Disappearing messages timer set to 1 year by %1$s."),
        //: Written by the core: Delta Chat's "multidevice_qr_subtitle".
        162: qsTr("Scan to set up second device for %1$s"),
        //: Written by the core: Delta Chat's "multidevice_transfer_done_devicemsg".
        163: qsTr("ℹ️ Profile transferred to your second device."),
        //: Written by the core: Delta Chat's "messages_are_e2ee".
        170: qsTr("Messages are end-to-end encrypted."),
        //: Written by the core: Delta Chat's "chat_new_group_hint".
        172: qsTr("Others will only see this group after you sent a first message."),
        //: Written by the core: Delta Chat's "member_x_added".
        173: qsTr("Member %1$s added."),
        //: Written by the core: Delta Chat's "reaction_by_you".
        176: qsTr("You reacted %1$s to \"%2$s\""),
        //: Written by the core: Delta Chat's "reaction_by_other".
        177: qsTr("%1$s reacted %2$s to \"%3$s\""),
        //: Written by the core: Delta Chat's "member_x_removed".
        178: qsTr("Member %1$s removed."),
        //: Written by the core: Delta Chat's "remove_you_by_other".
        179: qsTr("You were removed by %1$s."),
        //: Written by the core: Delta Chat's "add_you_by_other".
        180: qsTr("You were added by %1$s."),
        //: Written by the core: Delta Chat's "member_you_removed".
        181: qsTr("You were removed."),
        //: Written by the core: Delta Chat's "member_you_added".
        182: qsTr("You were added."),
        //: Written by the core: Delta Chat's "secure_join_wait".
        190: qsTr("Establishing connection, please wait…"),
        //: Written by the core: Delta Chat's "donate_device_msg".
        193: qsTr("❤️ Seems you're enjoying Delta Chat!\n\nPlease consider donating to help ensure that Delta Chat stays free for everyone.\n\nWhile Delta Chat is free to use and open source, development costs money. Help us to keep Delta Chat independent and make it even more awesome in the future.\n\nhttps://delta.chat/donate"),
        //: Written by the core: Delta Chat's "declined_call".
        196: qsTr("Declined call"),
        //: Written by the core: Delta Chat's "canceled_call".
        197: qsTr("Canceled call"),
        //: Written by the core: Delta Chat's "missed_call".
        198: qsTr("Missed call"),
        //: Written by the core: Delta Chat's "channel_left_by_you".
        200: qsTr("You left the channel."),
        //: Written by the core: Delta Chat's "qrshow_join_channel_hint".
        201: qsTr("Scan to join channel \"%1$s\""),
        //: Written by the core: Delta Chat's "you_joined_the_channel".
        202: qsTr("You joined the channel."),
        //: Written by the core: Delta Chat's "secure_join_channel_started".
        203: qsTr("%1$s invited you to join this channel.\n\nWaiting for the device of %2$s to reply…"),
        //: Written by the core: Delta Chat's "channel_name_changed".
        204: qsTr("Channel name changed from \"%1$s\" to \"%2$s\"."),
        //: Written by the core: Delta Chat's "channel_image_changed".
        205: qsTr("Channel image changed."),
        //: Written by the core: Delta Chat's "stats_msg_body".
        210: qsTr("The attachment contains anonymous usage statistics, which helps us improve Delta Chat. See https://delta.chat/help#statssending for more information. Thank you!"),
        //: Written by the core: Delta Chat's "proxy_enabled".
        220: qsTr("Proxy Enabled"),
        //: Written by the core: Delta Chat's "proxy_enabled_hint".
        221: qsTr("You are using a proxy. If you're having trouble connecting, try a different proxy."),
        //: Written by the core: Delta Chat's "chat_unencrypted_explanation".
        230: qsTr("Messages in this chat use classic email and are not end-to-end encrypted."),
        //: Written by the core: Delta Chat's "outgoing_audio_call".
        232: qsTr("Outgoing audio call"),
        //: Written by the core: Delta Chat's "outgoing_video_call".
        233: qsTr("Outgoing video call"),
        //: Written by the core: Delta Chat's "incoming_audio_call".
        234: qsTr("Incoming audio call"),
        //: Written by the core: Delta Chat's "incoming_video_call".
        235: qsTr("Incoming video call"),
        //: Written by the core: Delta Chat's "chat_description_changed_by_you".
        240: qsTr("You changed the chat description."),
        //: Written by the core: Delta Chat's "chat_description_changed_by_other".
        241: qsTr("Chat description changed by %1$s."),
        //: Written by the core: Delta Chat's "messages_are_e2ee".
        242: qsTr("Messages are end-to-end encrypted."),
        //: Written by the core: Delta Chat's "message_pinned_by_you".
        243: qsTr("You pinned a message"),
        //: Written by the core: Delta Chat's "message_pinned_by_other".
        244: qsTr("Message pinned by %1$s")
    }
}
