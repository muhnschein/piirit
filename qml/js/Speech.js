// Spoken languages behind Piirit's catalogs, for transcription: whisper code, own name.
// FLEURS WER (CER for zh), shipped model as app runs it: <= 25 % `available`, else greyed;
// over 15 % `rough`, said under name.
// Order: own name, romanised. Library: nothing translated, one shared copy.
.pragma library

var languages = [
    { code: "en", name: "English", available: true },
    { code: "bn", name: "বাংলা", available: false },
    { code: "bg", name: "Български", available: false },
    { code: "cs", name: "Čeština", available: false },
    { code: "da", name: "Dansk", available: false },
    { code: "de", name: "Deutsch", available: true },
    { code: "et", name: "Eesti", available: false },
    { code: "el", name: "Ελληνικά", available: false },
    { code: "es", name: "Español", available: true },
    { code: "fr", name: "Français", available: true, rough: true },
    { code: "gu", name: "ગુજરાતી", available: false },
    { code: "hi", name: "हिन्दी", available: false },
    { code: "it", name: "Italiano", available: true },
    { code: "kn", name: "ಕನ್ನಡ", available: false },
    { code: "lv", name: "Latviešu", available: false },
    { code: "lt", name: "Lietuvių", available: false },
    { code: "hu", name: "Magyar", available: false },
    { code: "ml", name: "മലയാളം", available: false },
    { code: "mr", name: "मराठी", available: false },
    { code: "nl", name: "Nederlands", available: true, rough: true },
    { code: "no", name: "Norsk bokmål", available: false },
    { code: "pa", name: "ਪੰਜਾਬੀ", available: false },
    { code: "pl", name: "Polski", available: true, rough: true },
    { code: "pt", name: "Português", available: true },
    { code: "ro", name: "Română", available: false },
    { code: "ru", name: "Русский", available: true },
    { code: "sk", name: "Slovenčina", available: false },
    { code: "sl", name: "Slovenščina", available: false },
    { code: "fi", name: "Suomi", available: false },
    { code: "sv", name: "Svenska", available: false },
    { code: "ta", name: "தமிழ்", available: false },
    { code: "tt", name: "Татар", available: false },
    { code: "te", name: "తెలుగు", available: false },
    { code: "vi", name: "Tiếng Việt", available: true, rough: true },
    { code: "tr", name: "Türkçe", available: true, rough: true },
    { code: "uk", name: "Українська", available: true, rough: true },
    { code: "zh", name: "中文（普通话）", available: true, rough: true },
    // Whisper small has no Cantonese token (`yue` is large-v3's 100th language).
    { code: "yue", name: "粵語（廣東話）", available: false }
]

/// Whisper code for a Qt locale name ("fi_FI", "nb_NO", "zh_HK"). "C": no LANG, which
/// Sailfish's default English leaves (no locale.conf).
function codeOf(localeName) {
    var name = String(localeName)
    if (name === "zh_HK") {
        return "yue"
    }
    var language = name.split(/[_-]/)[0]
    if (language === "C" || language === "POSIX" || language === "") {
        return "en"
    }
    return language === "nb" ? "no" : language
}

/// Table row for code, or null.
function find(code) {
    for (var i = 0; i < languages.length; i++) {
        if (languages[i].code === code) {
            return languages[i]
        }
    }
    return null
}

/// Selectable.
function isAvailable(code) {
    var row = find(code)
    return row !== null && row.available
}

/// Own name; outside table: platform's, else code.
function nameOf(code, platformName) {
    var row = find(code)
    if (row !== null) {
        return row.name
    }
    return platformName && platformName.length > 0 ? platformName : code
}

/// Table, phone language first (if listed), rest in table order.
function ordered(phoneCode) {
    var first = []
    var rest = []
    for (var i = 0; i < languages.length; i++) {
        if (languages[i].code === phoneCode) {
            first.push(languages[i])
        } else {
            rest.push(languages[i])
        }
    }
    return first.concat(rest)
}

/// Comma list into codes; empties dropped.
function split(list) {
    return String(list || "").split(",").filter(function (code) {
        return code.length > 0
    })
}

/// Own names of comma list, joined.
function names(list) {
    return split(list).map(function (code) {
        return nameOf(code, "")
    }).join(", ")
}
