// Public chatmail relays for pick lists (profile dialog, add-relay page).
// One list so they can't drift.
//
// chatmail.at/relays snapshot, 2026-10-07. Not whole list; anyone may run one.
// Refresh by hand; drop relays unlisted there.
//
// Library: nothing translated, shared by reference, one copy.
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
    { domain: "chatmail.au", location: "Melbourne, Australia" },
    { domain: "e2ee.wang", location: "Johannesburg, South Africa" },
    { domain: "chat.privittytech.com", location: "Bangalore, India" },
    { domain: "e2ee.im", location: "Orastie, Romania" },
    { domain: "chatmail.email", location: "Warsaw, Poland" },
    { domain: "chat.in-the.eu", location: "Falkenstein, Germany" },
    { domain: "chat.nuvon.app", location: "Prague, Czechia" },
    { domain: "nibblehole.com", location: "Zug, Switzerland" },
    { domain: "chat.sus.fr", location: "Iceland/Japan/Kenya/South Africa" },
    { domain: "delta.thelab.uno", location: "Gravelines, France" },
    { domain: "chat.vim.wtf", location: "Frankfurt, Germany" },
    { domain: "uninterest.ing", location: "Elk Grove Village, USA" },
    { domain: "sweetfern.net", location: "Ashburn, USA" },
    { domain: "delta.disobey.net", location: "Roon, Netherlands" },
    { domain: "toque.im", location: "Quebec, Canada" }
]

/// Row label: relay, then where or for whom.
function label(relay) {
    return relay.domain + " (" + relay.location + ")"
}
