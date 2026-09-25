// What a chat holds besides words, in the kinds the media pages list
// them by -- gallery, audio, files, apps, and a contact's calls -- named
// and drawn in one place, so the tile on a contact's or a group's page
// and the page behind it say the same word for the same thing.
//
// No `.pragma library`, for the reason Format.js gives: qsTr() translates
// in the context of the file it is written in, and a shared library has
// none.

/// The kinds, in the order the tiles stand in: apps first, as the sketch
/// this was built from has them, and only where apps are on; calls last,
/// and only where calls are on and the chat is one a call can be in.
function kinds(appsAvailable, callsAvailable) {
    return (appsAvailable ? ["apps", "gallery", "audio", "files"]
                          : ["gallery", "audio", "files"])
        .concat(callsAvailable ? ["calls"] : [])
}

/// What a kind is called: the tile's word, and the page's heading.
function kindName(kind) {
    if (kind === "gallery") {
        //: The pictures and videos of a chat: a tile, and a page heading.
        return qsTr("Gallery")
    }
    if (kind === "audio") {
        //: The voice messages and music of a chat.
        return qsTr("Audio")
    }
    if (kind === "files") {
        //: The documents and other files of a chat.
        return qsTr("Files")
    }
    if (kind === "apps") {
        //: The webxdc apps of a chat.
        return qsTr("Apps")
    }
    if (kind === "calls") {
        //: The calls made and received in a chat: a tile, and a page heading.
        return qsTr("Calls")
    }
    return ""
}

/// What the page says when the chat has none of a kind.
function emptyText(kind) {
    if (kind === "gallery") {
        return qsTr("No pictures or videos yet")
    }
    if (kind === "audio") {
        return qsTr("No voice messages or music yet")
    }
    if (kind === "files") {
        return qsTr("No files yet")
    }
    if (kind === "apps") {
        return qsTr("No apps yet")
    }
    if (kind === "calls") {
        return qsTr("No calls yet")
    }
    return ""
}

/// The theme icon a kind is drawn with. Empty for apps: the theme has
/// none for those, and AppMark draws one.
function kindIcon(kind) {
    if (kind === "gallery") {
        return "icon-m-file-image"
    }
    if (kind === "audio") {
        return "icon-m-file-audio"
    }
    if (kind === "files") {
        return "icon-m-file-document"
    }
    if (kind === "calls") {
        return "icon-m-call"
    }
    return ""
}

/// The theme's icon for a file of this MIME type, the way the platform's
/// own file browser picks one: by the family of the type, and by name
/// for the few the theme draws specially. `icon-m-file-other` for
/// whatever is left, which is the paperclip's job in the conversation.
function fileIcon(mime) {
    if (mime.indexOf("image/") === 0) {
        return "icon-m-file-image"
    }
    if (mime.indexOf("audio/") === 0) {
        return "icon-m-file-audio"
    }
    if (mime.indexOf("video/") === 0) {
        return "icon-m-file-video"
    }
    if (mime === "application/pdf") {
        return "icon-m-file-pdf"
    }
    if (mime === "text/vcard" || mime === "text/x-vcard") {
        return "icon-m-file-vcard"
    }
    if (mime === "application/zip" || mime === "application/x-tar"
            || mime === "application/gzip" || mime === "application/x-xz"
            || mime === "application/x-7z-compressed"
            || mime === "application/vnd.rar") {
        return "icon-m-file-archive"
    }
    if (mime.indexOf("spreadsheet") !== -1 || mime === "application/vnd.ms-excel"
            || mime === "text/csv") {
        return "icon-m-file-spreadsheet"
    }
    if (mime.indexOf("presentation") !== -1
            || mime === "application/vnd.ms-powerpoint") {
        return "icon-m-file-presentation"
    }
    if (mime.indexOf("wordprocessing") !== -1 || mime === "application/msword"
            || mime === "application/vnd.oasis.opendocument.text"
            || mime === "application/rtf") {
        return "icon-m-file-formatted"
    }
    if (mime.indexOf("text/") === 0) {
        return "icon-m-file-document"
    }
    return "icon-m-file-other"
}

/// A path the other end named, as a URL: encoded one segment at a time,
/// for the reason AttachmentPreview.fileUrl gives.
function fileUrl(path) {
    return "file://" + path.split("/").map(encodeURIComponent).join("/")
}
