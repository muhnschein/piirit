//! Mailbox quota and relay status, from the core's report.
//!
//! `get_connectivity_html` wants a web view (none allowed on Silica): parsed as parla does
//! (github.com/trufae/parla, `storage_quota.vala`), core's own percent and sentence. Own relay:
//! `quota_from_report`; all relays (profile rows): `transport_reports`.

// `get_connectivity` bands (at or above): 1000 not connected, 2000 connecting, 3000 working,
// 4000 idle.

/// Quota as reported: percent, core's words, used/limit bytes. Bytes 0 if not core's shape.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Quota {
    pub percent: u32,
    pub text: String,
    pub used_bytes: u64,
    pub limit_bytes: u64,
}

/// Quota bar for `address`'s relay; None if none reported (or before first connection).
///
/// One `<li class="transport">` per transport in add order, so matched by address. One block:
/// that relay, any heading. No blocks (older core): whole report.
pub(crate) fn quota_from_report(html: &str, address: &str) -> Option<Quota> {
    let blocks = transport_blocks(html);
    match blocks.as_slice() {
        [] => quota_in(html),
        [only] => quota_in(only),
        // None with profile's address: no quota, not another's.
        _ => quota_in(block_for(&blocks, address)?),
    }
}

/// One relay in report: domain, dot colour, words, quota bar.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TransportReport {
    /// Domain block is headed with.
    pub domain: String,
    /// Dot colour before domain: `green`, `yellow`, `red`, `grey`; empty if none.
    pub dot: String,
    /// Core's words after domain, in its language.
    pub status: String,
    /// Mailbox, once relay reports fullness.
    pub quota: Option<Quota>,
}

/// Every relay in report, add order; domainless blocks skipped; none (older core, unconnected):
/// empty. One parse, not a call per row.
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

/// Colour of first `dot` class in block (`<span class="green dot">`); empty if none.
fn dot_in(block: &str) -> &str {
    block
        .split("class=\"")
        .skip(1)
        .map(|after| after.split('"').next().unwrap_or_default())
        .find(|classes| classes.split_whitespace().any(|class| class == "dot"))
        .and_then(|classes| classes.split_whitespace().find(|class| *class != "dot"))
        .unwrap_or_default()
}

/// Words after domain, up to core's line break.
fn status_in(block: &str) -> String {
    block
        .split_once(":</b>")
        .map(|(_, after)| after.split("<br").next().unwrap_or_default())
        .map(strip_tags)
        .unwrap_or_default()
}

/// Each transport block's span: to next block or end of report.
fn transport_blocks(html: &str) -> Vec<&str> {
    const MARK: &str = "<li class=\"transport\">";
    let starts: Vec<usize> = html.match_indices(MARK).map(|(at, _)| at).collect();
    let ends = starts.iter().skip(1).copied().chain([html.len()]);
    starts
        .iter()
        .zip(ends)
        .filter_map(|(&start, end)| html.get(start..end))
        .collect()
}

/// Block for relay `address`: core heads each with `<b>domain:</b>`.
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

/// Domain block is headed with: first `<b>...:</b>`.
fn block_domain(block: &str) -> Option<&str> {
    block
        .split("<b>")
        .nth(1)
        .and_then(|after| after.split_once(":</b>"))
        .map(|(name, _)| name.trim())
        .filter(|name| !name.is_empty())
}

/// First quota bar in fragment, if any.
fn quota_in(html: &str) -> Option<Quota> {
    // `<div class="progress grey" style="width:12%">12%</div>`: number real, width capped.
    let bar = html.find("class=\"progress")?;
    let before_bar = html.get(..bar)?;
    let (_, after_open) = html.get(bar..)?.split_once('>')?;
    let (inner, _) = after_open.split_once('<')?;
    let percent: u32 = inner.trim().trim_end_matches('%').parse().ok()?;
    // Sentence: bar back to its `<li>`, untagged. `<div` before `<li>`: no words, percent read.
    let item_start = before_bar.rfind("<li>")?.saturating_add("<li>".len());
    let bar_div = before_bar.rfind("<div")?;
    let text = strip_tags(before_bar.get(item_start..bar_div).unwrap_or_default());
    let (used_bytes, limit_bytes) = amounts(&text).unwrap_or((0, 0));
    Some(Quota {
        percent,
        text,
        used_bytes,
        limit_bytes,
    })
}

/// Amounts in `"1.34 GiB of 2 GiB used"`, in bytes.
fn amounts(text: &str) -> Option<(u64, u64)> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let of = words
        .iter()
        .position(|word| word.eq_ignore_ascii_case("of"))?;
    let from = of.checked_sub(2)?;
    let [used_number, used_unit, _of, limit_number, limit_unit, ..] = words.get(from..)? else {
        return None;
    };
    let used = bytes(used_number, used_unit)?;
    let limit = bytes(limit_number, limit_unit)?;
    Some((used, limit))
}

/// Number and unit as bytes: `KiB` 1024s, `kB`/`MB` 1000s, `B` bytes; decimal comma ok.
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
    // Max 2 decimals: truncation loses nothing.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some((number * base.powi(power)).round() as u64)
}

/// HTML fragment as text: tags dropped, entities decoded, whitespace collapsed.
fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut chars = html.chars();
    while let Some(c) = chars.next() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if in_tag => {}
            '&' => {
                let (decoded, after) = decode_entity(chars.as_str());
                out.push_str(decoded);
                chars = after.chars();
            }
            _ => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Character an entity stands for, and text after it. Unknown entity: left as `&`.
fn decode_entity(rest: &str) -> (&'static str, &str) {
    for (entity, decoded) in [
        ("amp;", "&"),
        ("lt;", "<"),
        ("gt;", ">"),
        ("quot;", "\""),
        ("apos;", "'"),
        ("nbsp;", " "),
        ("#39;", "'"),
    ] {
        if let Some(after) = rest.strip_prefix(entity) {
            return (decoded, after);
        }
    }
    ("&", rest)
}

#[cfg(test)]
mod tests {
    use super::{amounts, quota_from_report, transport_reports, Quota, TransportReport};

    /// Core's shape, minus style block.
    const REPORT: &str = "<html><body><h3>Incoming messages</h3><ul>\
        <li class=\"transport\"><span class=\"dot green\"></span> <b>nine.testrun.org:</b> Connected<br />\
        <ul class=\"quota-list\"><li>1.34 GiB of 2 GiB used\
        <div class=\"bar\"><div class=\"progress grey\" style=\"width: 67%\">67%</div></div>\
        </li></ul></li></ul><h3>Outgoing messages</h3><ul><li>Connected</li></ul></body></html>";

    /// Two transports: older first, then own.
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

    /// Sole relay is the profile's, whatever its heading.
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

    /// Two transports: own bar, not first relay's.
    #[test]
    fn the_quota_is_the_one_on_the_profiles_own_relay() {
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
        // Core writes domain as stored; configured address may differ in case.
        assert_eq!(
            quota_from_report(TWO, "Ada@Chat.Example.ORG").map(|quota| quota.percent),
            Some(2)
        );
    }

    /// No quota of its own: bar beside another relay's name is not this mailbox.
    #[test]
    fn another_relays_bar_is_not_borrowed() {
        let silent = TWO.replace(
            "<ul class=\"quota-list\"><li>12 MiB of 1 GiB used\
             <div class=\"bar\"><div class=\"progress grey\" style=\"width: 2%\">2%</div></div>\
             </li></ul>",
            "",
        );
        assert_eq!(quota_from_report(&silent, "ada@chat.example.org"), None);
        // Nor borrowed for an address no block is headed with.
        assert_eq!(quota_from_report(TWO, "ada@chat.elsewhere.org"), None);
        assert_eq!(quota_from_report(TWO, ""), None);
    }

    /// Every relay's block in core order. Feeds profile relay rows.
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

    /// Core's shape for unreachable relay: dot colour, words, no bar.
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

    /// No mailbox info: listed, no bar. No transport blocks: none listed.
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
        assert_eq!(
            transport_reports("<html><body><h3>Not connected</h3></body></html>"),
            Vec::new()
        );
        assert_eq!(transport_reports(""), Vec::new());
        // Block with no domain has no relay to list under.
        let unheaded = REPORT.replace("<b>nine.testrun.org:</b>", "");
        assert_eq!(transport_reports(&unheaded), Vec::new());
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
        // Not core's shape: no amounts, not wrong ones.
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
        // Over-full: width capped, number not.
        let full = REPORT.replace("width: 67%\">67%", "width: 100%\">120%");
        assert_eq!(
            quota_from_report(&full, "ada@nine.testrun.org").map(|quota| quota.percent),
            Some(120)
        );
    }

    /// Bar before its item's `<li>`: slice would panic. Now: bar with no words.
    #[test]
    fn a_bar_outside_the_shape_the_core_writes_does_not_panic() {
        assert_eq!(
            quota_from_report(
                "<div><li>a<span class=\"progress\">5%</span>",
                "ada@nine.testrun.org"
            ),
            Some(Quota {
                percent: 5,
                text: String::new(),
                used_bytes: 0,
                limit_bytes: 0,
            })
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
