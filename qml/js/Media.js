// Chat's non-text kinds (gallery, audio, files, apps, calls, map): names, icons.
// No `.pragma library`: qsTr() needs importing file's context.

/// Kinds in tile order. Apps first if on; calls, if chat can hold one; map last, if on.
function kinds(appsAvailable, callsAvailable, mapAvailable) {
    return (appsAvailable ? ["apps", "gallery", "audio", "files"]
                          : ["gallery", "audio", "files"])
        .concat(callsAvailable ? ["calls"] : [])
        .concat(mapAvailable ? ["map"] : [])
}

/// Kind's name: tile word and page heading.
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
    if (kind === "map") {
        //: Tile opening the map of a chat's shared locations.
        return qsTr("Map")
    }
    return ""
}

/// Page text when chat has none of a kind.
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

/// Theme icon per kind. Empty for apps: AppMark draws one.
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
    if (kind === "map") {
        return "icon-m-location"
    }
    return ""
}

/// Theme icon for MIME type, as file browser: by family, a few by name; else
/// `icon-m-file-other`.
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

/// Peer's path as URL, encoded per segment (see AttachmentPreview.fileUrl).
function fileUrl(path) {
    return "file://" + path.split("/").map(encodeURIComponent).join("/")
}
