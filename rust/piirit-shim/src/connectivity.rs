//! What the core says about a profile's connection and its storage.
//!
//! `get_connectivity` is a number, and `get_connectivity_html` is a page
//! written for a web view: headings, a list per relay, a progress bar for
//! the mailbox quota. Silica has no web view this app may use, so the one
//! fact worth showing -- how full the mailbox is -- is read off that page
//! the way parla reads it (github.com/trufae/parla, `storage_quota.vala`):
//! the percentage the core wrote on its own bar, and the sentence it wrote
//! beside it, in whatever language the core is in. Nothing here computes
//! a quota; the core did, and this finds where it put the answer. Once
//! for the relay the profile sends from (`quota_from_report`), and once
//! for every relay it has (`transport_reports`) -- the dot the core
//! drew for it, its own words about the connection, and the bar --
//! which is what the profile page's relay rows are drawn from.

// The core's `get_connectivity` bands, which the profile page puts words
// to: 1000 not connected, 2000 connecting, 3000 connected and working,
// 4000 connected and idle. Anything at or above a band is in it; the
// values in between are the core's own finer steps.

/// The mailbox quota as the report states it: the percentage used, the
/// core's own words for the amounts, and those amounts in bytes -- read
/// out of the words, so the page can say how much is left the way parla
/// does. Both are 0 when the sentence is not in the shape the core writes.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Quota {
    pub percent: u32,
    pub text: String,
    pub used_bytes: u64,
    pub limit_bytes: u64,
}

/// The quota bar for `address`'s relay, if there is one. A relay that
/// reports no quota, or a report from before the first connection, has
/// none.
///
/// A profile can have several transports, and the core writes one
/// `<li class="transport">` per transport, in the order they were added,
/// each with its own quota. Reading the first bar in the report would
/// give whichever relay was set up first rather than the one the profile
/// sends from, so the block is picked by the address before the bar is
/// read. A report with one transport block has only one relay to report
/// on, whatever the block is headed with, and a report with none of them
/// at all -- an older core -- is read as it always was.
pub(crate) fn quota_from_report(html: &str, address: &str) -> Option<Quota> {
    let blocks = transport_blocks(html);
    match blocks.len() {
        0 => quota_in(html),
        1 => quota_in(blocks[0]),
        // Several relays and none of them this profile's primary: no
        // quota rather than another relay's.
        _ => quota_in(block_for(&blocks, address)?),
    }
}

/// What the report says about one relay: the block the core headed with
/// its domain, read for the dot it drew, the words beside it and the
/// quota bar under it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TransportReport {
    /// The domain the block is headed with.
    pub domain: String,
    /// The colour of the dot the core drew before the domain -- `green`,
    /// `yellow`, `red` or `grey`, as its own stylesheet names them --
    /// or empty when it drew none.
    pub dot: String,
    /// The core's own words after the domain: "Connected", "Connecting…",
    /// "Not connected: ...". In whatever language the core is in.
    pub status: String,
    /// The mailbox, when the relay has said how full it is.
    pub quota: Option<Quota>,
}

/// Every relay the report covers, in the order the core lists them,
/// which is the order they were added. A block the core never headed
/// with a domain is left out, and a report with no transport blocks at
/// all -- an older core, or a profile not connected yet -- has nothing
/// to list. What the profile page's relay rows are drawn from: one parse
/// of the report, rather than a call to the core per row.
pub(crate) fn transport_reports(html: &str) -> Vec<TransportReport> {
    transport_blocks(html)
        .into_iter()
        .filter_map(|block| {
            let domain = block_domain(block)?.to_string();
            Some(TransportReport {
                domain,
                dot: dot_in(block).to_string(),
                status: status_in(block),
                quota: quota_in(block),
            })
        })
        .collect()
}

/// The colour of the first dot in a block: the core writes
/// `<span class="green dot"></span>` before the domain, the colour first.
/// Empty when there is no dot. The block itself opens with a class of
/// its own (`transport`), and the bar under it with more, so it is the
/// first class list naming `dot` that counts.
fn dot_in(block: &str) -> &str {
    block
        .split("class=\"")
        .skip(1)
        .map(|after| after.split('"').next().unwrap_or_default())
        .find(|classes| classes.split_whitespace().any(|class| class == "dot"))
        .and_then(|classes| classes.split_whitespace().find(|class| *class != "dot"))
        .unwrap_or_default()
}

/// The words after the domain, up to the line break the core ends them
/// with: its own account of that relay's connection.
fn status_in(block: &str) -> String {
    block
        .split_once(":</b>")
        .map(|(_, after)| after.split("<br").next().unwrap_or_default())
        .map(strip_tags)
        .unwrap_or_default()
}

/// Where each `<li class="transport">` block starts and ends. The core
/// writes them one after another inside the incoming-messages list, so a
/// block runs to the next one, or to the end of the report.
fn transport_blocks(html: &str) -> Vec<&str> {
    const MARK: &str = "<li class=\"transport\">";
    let starts: Vec<usize> = html.match_indices(MARK).map(|(at, _)| at).collect();
    starts
        .iter()
        .enumerate()
        .map(|(nth, &start)| {
            let end = starts.get(nth + 1).copied().unwrap_or(html.len());
            &html[start..end]
        })
        .collect()
}

/// The block for the relay `address` sends from: the core heads each one
/// with `<b>domain:</b>`, the domain of that transport's address.
fn block_for<'html>(blocks: &[&'html str], address: &str) -> Option<&'html str> {
    let domain = address.rsplit('@').next()?.trim();
    if domain.is_empty() {
        return None;
    }
    blocks
        .iter()
        .copied()
        .find(|block| block_domain(block).is_some_and(|name| name.eq_ignore_ascii_case(domain)))
}

/// The domain a transport block is headed with: the text of the first
/// `<b>...:</b>` in it, which is how the core names the relay.
fn block_domain(block: &str) -> Option<&str> {
    block
        .split("<b>")
        .nth(1)
        .and_then(|after| after.split_once(":</b>"))
        .map(|(name, _)| name.trim())
        .filter(|name| !name.is_empty())
}

/// The first quota bar in a fragment of the report, if there is one.
fn quota_in(html: &str) -> Option<Quota> {
    // The bar the core draws: `<div class="progress grey" style="width:
    // 12%">12%</div>`. The number inside the div is the real percentage;
    // the width is capped at 100.
    let bar = html.find("class=\"progress")?;
    let after_bar = &html[bar..];
    let open_end = after_bar.find('>')? + 1;
    let inner_end = after_bar[open_end..].find('<')? + open_end;
    let percent: u32 = after_bar[open_end..inner_end]
        .trim()
        .trim_end_matches('%')
        .parse()
        .ok()?;
    // The sentence before the bar, inside the same list item: back from
    // the bar to the `<li>` that holds it, then its tags dropped.
    let item_start = html[..bar].rfind("<li>")? + "<li>".len();
    let bar_div = html[..bar].rfind("<div")?;
    let text = strip_tags(&html[item_start..bar_div]);
    let (used_bytes, limit_bytes) = amounts(&text).unwrap_or((0, 0));
    Some(Quota {
        percent,
        text,
        used_bytes,
        limit_bytes,
    })
}

/// The two amounts in "1.34 GiB of 2 GiB used", in bytes. parla reads
/// the same sentence with the same pattern (`storage_quota.vala`): a
/// number, a unit, "of", a number, a unit, "used".
fn amounts(text: &str) -> Option<(u64, u64)> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let of = words
        .iter()
        .position(|word| word.eq_ignore_ascii_case("of"))?;
    if of < 2 || of + 3 > words.len() {
        return None;
    }
    let used = bytes(words[of - 2], words[of - 1])?;
    let limit = bytes(words[of + 1], words[of + 2])?;
    Some((used, limit))
}

/// A number and a unit -- "1.34" and "GiB" -- as bytes. Binary units
/// (`KiB`, `MiB`, ...) are powers of 1024, decimal ones (`kB`, `MB`, ...)
/// of 1000; a bare `B` is bytes. A decimal comma counts as a point.
fn bytes(number: &str, unit: &str) -> Option<u64> {
    let number: f64 = number.replace(',', ".").parse().ok()?;
    if !number.is_finite() || number < 0.0 {
        return None;
    }
    let unit = unit.trim_end_matches(['B', 'b']);
    let (prefix, binary) = match unit.strip_suffix('i') {
        Some(prefix) => (prefix, true),
        None => (unit, false),
    };
    let power = match prefix.to_ascii_lowercase().as_str() {
        "" => 0,
        "k" => 1,
        "m" => 2,
        "g" => 3,
        "t" => 4,
        _ => return None,
    };
    let base: f64 = if binary { 1024.0 } else { 1000.0 };
    // Rounded to the byte: the sentence carries two decimals at most, so
    // the truncation is below what it can say anyway.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some((number * base.powi(power)).round() as u64)
}

/// The text of a fragment of HTML: tags dropped, the few entities the
/// core writes put back, whitespace collapsed.
fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut rest = html;
    while let Some(c) = rest.chars().next() {
        rest = &rest[c.len_utf8()..];
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if in_tag => {}
            '&' => {
                let (decoded, skip) = decode_entity(rest);
                out.push_str(decoded);
                rest = &rest[skip..];
            }
            _ => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The character an entity stands for, and how much of the text it took.
/// An entity this does not know is left as its ampersand.
fn decode_entity(rest: &str) -> (&'static str, usize) {
    for (entity, decoded) in [
        ("amp;", "&"),
        ("lt;", "<"),
        ("gt;", ">"),
        ("quot;", "\""),
        ("apos;", "'"),
        ("nbsp;", " "),
        ("#39;", "'"),
    ] {
        if rest.starts_with(entity) {
            return (decoded, entity.len());
        }
    }
    ("&", 0)
}

#[cfg(test)]
mod tests {
    use super::{amounts, quota_from_report, transport_reports, Quota, TransportReport};

    /// The shape the core writes, less the style block.
    const REPORT: &str = "<html><body><h3>Incoming messages</h3><ul>\
        <li class=\"transport\"><span class=\"dot green\"></span> <b>nine.testrun.org:</b> Connected<br />\
        <ul class=\"quota-list\"><li>1.34 GiB of 2 GiB used\
        <div class=\"bar\"><div class=\"progress grey\" style=\"width: 67%\">67%</div></div>\
        </li></ul></li></ul><h3>Outgoing messages</h3><ul><li>Connected</li></ul></body></html>";

    /// Two transports, the way a profile set up twice has them: the older
    /// one first, as the core lists them, and the profile's own second.
    const TWO: &str = "<html><body><h3>Incoming messages</h3><ul>\
        <li class=\"transport\"><span class=\"dot green\"></span> <b>nine.testrun.org:</b> Connected<br />\
        <ul class=\"quota-list\"><li>1.34 GiB of 2 GiB used\
        <div class=\"bar\"><div class=\"progress grey\" style=\"width: 67%\">67%</div></div>\
        </li></ul></li>\
        <li class=\"transport\"><span class=\"dot green\"></span> <b>chat.example.org:</b> Connected<br />\
        <ul class=\"quota-list\"><li>12 MiB of 1 GiB used\
        <div class=\"bar\"><div class=\"progress grey\" style=\"width: 2%\">2%</div></div>\
        </li></ul></li></ul><h3>Outgoing messages</h3><ul><li>Connected</li></ul></body></html>";

    #[test]
    fn the_quota_is_read_off_the_bar_the_core_drew() {
        assert_eq!(
            quota_from_report(REPORT, "ada@nine.testrun.org"),
            Some(Quota {
                percent: 67,
                text: "1.34 GiB of 2 GiB used".to_string(),
                used_bytes: 1_438_814_044,
                limit_bytes: 2_147_483_648,
            })
        );
    }

    /// One relay to report on is that profile's, whatever it is headed
    /// with -- a report written before the core has named the transport
    /// still has a quota to show.
    #[test]
    fn a_single_transport_is_read_whatever_the_address() {
        assert_eq!(
            quota_from_report(REPORT, "ada@chat.example.org").map(|quota| quota.percent),
            Some(67)
        );
        assert_eq!(
            quota_from_report(REPORT, "").map(|quota| quota.percent),
            Some(67)
        );
    }

    /// The bug behind this: with a second transport, the first bar in the
    /// report is the first relay ever set up, not the one the profile
    /// sends from.
    #[test]
    fn the_quota_is_the_relay_the_profile_sends_from() {
        assert_eq!(
            quota_from_report(TWO, "ada@chat.example.org"),
            Some(Quota {
                percent: 2,
                text: "12 MiB of 1 GiB used".to_string(),
                used_bytes: 12_582_912,
                limit_bytes: 1_073_741_824,
            })
        );
        assert_eq!(
            quota_from_report(TWO, "ada@nine.testrun.org").map(|quota| quota.percent),
            Some(67)
        );
        // The core writes the domain as it stored it; the address the
        // profile was configured with need not match its case.
        assert_eq!(
            quota_from_report(TWO, "Ada@Chat.Example.ORG").map(|quota| quota.percent),
            Some(2)
        );
    }

    /// A relay that reports no quota of its own reports none: the bar
    /// beside another relay's name is not this profile's mailbox.
    #[test]
    fn another_relays_bar_is_not_borrowed() {
        let silent = TWO.replace(
            "<ul class=\"quota-list\"><li>12 MiB of 1 GiB used\
             <div class=\"bar\"><div class=\"progress grey\" style=\"width: 2%\">2%</div></div>\
             </li></ul>",
            "",
        );
        assert_eq!(quota_from_report(&silent, "ada@chat.example.org"), None);
        // Nor is it borrowed for an address no block is headed with.
        assert_eq!(quota_from_report(TWO, "ada@chat.elsewhere.org"), None);
        assert_eq!(quota_from_report(TWO, ""), None);
    }

    /// Every relay's block, headed with its domain, in the order the
    /// core listed them: the dot it drew, its words, its bar. What the
    /// relay rows on the profile page draw.
    #[test]
    fn every_relay_is_listed_under_its_domain_with_its_dot_words_and_bar() {
        let listed: Vec<(String, String, String, Option<u32>)> = transport_reports(TWO)
            .into_iter()
            .map(|report| {
                (
                    report.domain,
                    report.dot,
                    report.status,
                    report.quota.map(|quota| quota.percent),
                )
            })
            .collect();
        assert_eq!(
            listed,
            vec![
                (
                    "nine.testrun.org".to_string(),
                    "green".to_string(),
                    "Connected".to_string(),
                    Some(67)
                ),
                (
                    "chat.example.org".to_string(),
                    "green".to_string(),
                    "Connected".to_string(),
                    Some(2)
                ),
            ]
        );
        assert_eq!(
            transport_reports(REPORT)
                .into_iter()
                .filter_map(|report| report.quota)
                .map(|quota| quota.text)
                .collect::<Vec<_>>(),
            vec!["1.34 GiB of 2 GiB used".to_string()]
        );
    }

    /// The shape the pinned core writes for a relay it cannot reach: the
    /// colour before `dot`, the words after the domain ending at the
    /// line break, and no bar.
    #[test]
    fn a_relay_that_cannot_be_reached_has_a_red_dot_the_words_and_no_bar() {
        let report = "<html><body><h3>Incoming Messages</h3><ul>\
            <li class=\"transport\"><span class=\"red dot\"></span> <b>example.invalid:</b> \
            Error: No IMAP connection candidates provided<br /></li></ul>\
            <h3>Outgoing Messages</h3><ul><li><span class=\"green dot\"></span> \
            Your last message was sent successfully.</li></ul></body></html>";
        assert_eq!(
            transport_reports(report),
            vec![TransportReport {
                domain: "example.invalid".to_string(),
                dot: "red".to_string(),
                status: "Error: No IMAP connection candidates provided".to_string(),
                quota: None,
            }]
        );
    }

    /// A relay that has said nothing about its mailbox is listed with no
    /// bar, a block with no dot with no colour, and a report without
    /// transport blocks -- an older core, or a profile not connected yet
    /// -- has nothing to list.
    #[test]
    fn a_relay_without_a_bar_or_a_dot_is_still_listed() {
        let silent = TWO.replace(
            "<ul class=\"quota-list\"><li>12 MiB of 1 GiB used\
             <div class=\"bar\"><div class=\"progress grey\" style=\"width: 2%\">2%</div></div>\
             </li></ul>",
            "",
        );
        let listed: Vec<(String, bool)> = transport_reports(&silent)
            .into_iter()
            .map(|report| (report.domain, report.quota.is_some()))
            .collect();
        assert_eq!(
            listed,
            vec![
                ("nine.testrun.org".to_string(), true),
                ("chat.example.org".to_string(), false)
            ]
        );
        let undotted = REPORT.replace("<span class=\"dot green\"></span> ", "");
        assert_eq!(
            transport_reports(&undotted)
                .into_iter()
                .map(|report| (report.dot, report.status))
                .collect::<Vec<_>>(),
            vec![(String::new(), "Connected".to_string())]
        );
        assert!(transport_reports("<html><body><h3>Not connected</h3></body></html>").is_empty());
        assert!(transport_reports("").is_empty());
        // A block the core never headed is a block with no relay to
        // list under.
        let unheaded = REPORT.replace("<b>nine.testrun.org:</b>", "");
        assert!(transport_reports(&unheaded).is_empty());
        assert_eq!(
            quota_from_report(&unheaded, "ada@nine.testrun.org").map(|quota| quota.percent),
            Some(67),
            "one block is still that profile's, headed or not"
        );
    }

    #[test]
    fn the_amounts_are_read_in_either_kind_of_unit() {
        assert_eq!(
            amounts("1.34 GiB of 2 GiB used"),
            Some((1_438_814_044, 2_147_483_648))
        );
        assert_eq!(amounts("512 kB of 1 MB used"), Some((512_000, 1_000_000)));
        assert_eq!(
            amounts("0,5 MiB of 10 MiB used"),
            Some((524_288, 10_485_760))
        );
        assert_eq!(amounts("900 B of 1 KiB used"), Some((900, 1024)));
        // Not the shape the core writes: no amounts rather than wrong ones.
        assert_eq!(amounts("Quota unknown"), None);
        assert_eq!(amounts("1.34 parsecs of 2 GiB used"), None);
    }

    #[test]
    fn a_report_without_a_bar_has_no_quota() {
        assert_eq!(
            quota_from_report(
                "<html><body><h3>Not connected</h3></body></html>",
                "ada@nine.testrun.org"
            ),
            None
        );
        assert_eq!(quota_from_report("", "ada@nine.testrun.org"), None);
        // Over-full: the width is capped but the number is not.
        let full = REPORT.replace("width: 67%\">67%", "width: 100%\">120%");
        assert_eq!(
            quota_from_report(&full, "ada@nine.testrun.org").map(|quota| quota.percent),
            Some(120)
        );
    }

    #[test]
    fn entities_and_tags_come_out_of_the_words() {
        let report = REPORT.replace(
            "1.34 GiB of 2 GiB used",
            "<b>Storage:</b> 1.34&nbsp;GiB of 2 GiB used &amp; counting",
        );
        assert_eq!(
            quota_from_report(&report, "ada@nine.testrun.org").map(|quota| quota.text),
            Some("Storage: 1.34 GiB of 2 GiB used & counting".to_string())
        );
    }
}
