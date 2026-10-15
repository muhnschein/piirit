import QtQuick 2.0

// Stub: watches remote bus object, calls fn per signal. Connects nothing.
QtObject {
    property int bus
    property string service
    property string path
    property string iface
    property bool signalsEnabled
    property bool watchServiceStatus
    // `enum Status` as real. Stays Unknown: no bus.
    property int status: 0

    // Calls made, one per line: method and args as JSON.
    property string called: ""
    // Call's answer if given. Undefined: nothing answers.
    property var reply: undefined
    // Multi-value reply: one callback argument per value.
    property var replies: undefined
    // Error call fails with: [name, message].
    property var failure: undefined
    // Answers held until `answer()`.
    property bool deferred: false
    property var held: []

    function answer() {
        var waiting = held
        held = []
        for (var i = 0; i < waiting.length; i++) {
            waiting[i]()
        }
    }

    // Real: puts call on bus, reply to callback. Here: test hands reply in.
    function typedCall(method, args, onSuccess, onError) {
        called += method + " " + JSON.stringify(args) + "\n"
        var failed = failure
        var answers = replies
        var answered = reply
        var deliver = function () {
            if (failed !== undefined) {
                if (onError) {
                    onError(failed[0], failed[1])
                }
            } else if (answers !== undefined && onSuccess) {
                onSuccess.apply(null, answers)
            } else if (answered !== undefined && onSuccess) {
                onSuccess(answered)
            }
        }
        if (deferred) {
            held = held.concat([deliver])
        } else {
            deliver()
        }
    }
}
