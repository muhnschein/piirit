import QtQuick 2.0

// Stands in for Nemo.DBus's DBusInterface, which on a device watches a
// remote object on the bus and calls the function named after each signal
// it hears. Nothing is connected here: it holds the address it would
// watch, and a test calls the component's own entry point instead of a
// bus nobody has in a test.
QtObject {
    property int bus
    property string service
    property string path
    property string iface
    property bool signalsEnabled
    property bool watchServiceStatus
    // `enum Status { Unknown, Unavailable, Available }` on the real type.
    // Left at Unknown: a test has no bus, so nothing it watches is ever
    // available, and the calls that would follow are driven directly.
    property int status: 0

    // What was called, one line per call: the method and its arguments
    // as JSON, for a test to read.
    property string called: ""
    // What a call answers with, when a test gives it something to: left
    // undefined, a call is answered by nothing, as a bus nobody is on.
    property var reply: undefined
    // A reply of several values, handed to the callback one argument
    // each, as the real one hands a reply's arguments.
    property var replies: undefined
    // An error the call fails with instead, as [name, message].
    property var failure: undefined
    // Answers held back until `answer()`, as a bus answers some time
    // after the call: what was asked meanwhile is answered then.
    property bool deferred: false
    property var held: []

    function answer() {
        var waiting = held
        held = []
        for (var i = 0; i < waiting.length; i++) {
            waiting[i]()
        }
    }

    // The real one puts the call on the bus and hands the reply to the
    // callback. Here there is no bus: a test either hands the reply in
    // above, or drives what it would have led to itself.
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
