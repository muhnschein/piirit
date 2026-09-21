// The public chatmail relays, for the two pages that offer a list of them
// to pick from: the dialog that makes a profile on one, and the page that
// adds one to a profile that exists. One list rather than two, so the two
// cannot drift.
//
// chatmail.at/relays as it stood on 2026-09-20, in its order, with what
// it says about each. Anyone may run a relay and the page is the list
// that is kept up to date, so this one is a starting point rather than
// the whole of it -- the hint under the field on either page points at
// the page itself. Refreshed by hand against that page, and one that
// stops being listed there is dropped here.
//
// A library rather than a component: nothing here is translated, so it
// needs no component's context, and a list shared by reference costs one
// copy rather than one per page.
.pragma library

var list = [
    { domain: "nine.testrun.org", location: "Default for many chatmail apps" },
    { domain: "mehl.cloud", location: "German speakers" },
    { domain: "mailchat.pl", location: "Polish speakers" },
    { domain: "chatmail.woodpeckersnest.space", location: "Italian speakers" },
    { domain: "chatmail.culturanerd.it", location: "Italian speakers" },
    { domain: "chat.adminforge.de", location: "Falkenstein, Germany" },
    { domain: "chika.aangat.lahat.computer", location: "Santa Clara, USA" },
    { domain: "tarpit.fun", location: "Nuremberg, Germany" },
    { domain: "d.gaufr.es", location: "Roubaix, France" },
    { domain: "chtml.ca", location: "Quebec, Canada" },
    { domain: "e2ee.wang", location: "Johannesburg, South Africa" },
    { domain: "chat.privittytech.com", location: "Bangalore, India" },
    { domain: "e2ee.im", location: "Orastie, Romania" },
    { domain: "chatmail.email", location: "Warsaw, Poland" },
    { domain: "chat.in-the.eu", location: "Falkenstein, Germany" },
    { domain: "chat.nuvon.app", location: "Prague, Czechia" },
    { domain: "nibblehole.com", location: "Zug, Switzerland" },
    { domain: "chat.zashm.org", location: "Lviv, Ukraine" },
    { domain: "chat.sus.fr", location: "Iceland/Japan/Kenya/South Africa" },
    { domain: "delta.thelab.uno", location: "Gravelines, France" },
    { domain: "chat.vim.wtf", location: "Frankfurt, Germany" },
    { domain: "uninterest.ing", location: "Elk Grove Village, USA" },
    { domain: "sweetfern.net", location: "Ashburn, USA" },
    { domain: "delta.disobey.net", location: "Roon, Netherlands" }
]

/// What a row of the list says: the relay, and where or for whom it is.
function label(relay) {
    return relay.domain + " (" + relay.location + ")"
}
