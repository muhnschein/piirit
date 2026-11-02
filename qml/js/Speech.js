// Spoken languages behind Piirit's catalogs, for transcription: whisper code, own name.
// `available` false: greyed, unselectable (accuracy too low for now). Provisional: change here.
// Order: own name, romanised. Library: nothing translated, one shared copy.
.pragma library

var languages = [
    { code: "en", name: "English", available: true },
    { code: "bn", name: "বাংলা", available: false },
    { code: "bg", name: "Български", available: true },
    { code: "cs", name: "Čeština", available: true },
    { code: "da", name: "Dansk", available: true },
    { code: "de", name: "Deutsch", available: true },
    { code: "et", name: "Eesti", available: true },
    { code: "el", name: "Ελληνικά", available: true },
    { code: "es", name: "Español", available: true },
    { code: "fr", name: "Français", available: true },
    { code: "gu", name: "ગુજરાતી", available: false },
    { code: "hi", name: "हिन्दी", available: true },
    { code: "it", name: "Italiano", available: true },
    { code: "kn", name: "ಕನ್ನಡ", available: false },
    { code: "lv", name: "Latviešu", available: true },
    { code: "lt", name: "Lietuvių", available: false },
    { code: "hu", name: "Magyar", available: true },
    { code: "ml", name: "മലയാളം", available: false },
    { code: "mr", name: "मराठी", available: false },
    { code: "nl", name: "Nederlands", available: true },
    { code: "no", name: "Norsk bokmål", available: true },
    { code: "pa", name: "ਪੰਜਾਬੀ", available: false },
    { code: "pl", name: "Polski", available: true },
    { code: "pt", name: "Português", available: true },
    { code: "ro", name: "Română", available: true },
    { code: "ru", name: "Русский", available: true },
    { code: "sk", name: "Slovenčina", available: true },
    { code: "sl", name: "Slovenščina", available: true },
    { code: "fi", name: "Suomi", available: true },
    { code: "sv", name: "Svenska", available: true },
    { code: "ta", name: "தமிழ்", available: true },
    { code: "tt", name: "Татар", available: false },
    { code: "te", name: "తెలుగు", available: false },
    { code: "vi", name: "Tiếng Việt", available: true },
    { code: "tr", name: "Türkçe", available: true },
    { code: "uk", name: "Українська", available: true },
    { code: "zh", name: "中文（普通话）", available: true },
    // Whisper small has no Cantonese token (`yue` is large-v3's 100th language).
    { code: "yue", name: "粵語（廣東話）", available: false }
]

/// Whisper code for a Qt locale name ("fi_FI", "nb_NO", "zh_HK").
function codeOf(localeName) {
    var name = String(localeName)
    if (name === "zh_HK") {
        return "yue"
    }
    var language = name.split(/[_-]/)[0]
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
