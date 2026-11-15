pragma Singleton
import QtQuick 2.0

// Stub for Sailfish.WebEngine WebEngineSettings. Records preferences app
// sets on WebEngine stub's `notified`, in order with its notifications.
QtObject {
    enum PreferenceType { UnknownPref, StringPref, IntPref, BoolPref }

    function setPreference(key, value, kind) {
        WebEngine.notified += "pref " + key + "=" + value + " kind=" + kind + "\n"
    }
}
