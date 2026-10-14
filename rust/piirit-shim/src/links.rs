//! Tracking params stripped from outgoing links. Rules: parla's (github.com/trufae/parla,
//! `link_cleaner.vala`).
//!
//! Content params (video id, timestamp, search) stay; anything not listed left byte for byte.
//! Nothing fetched: text rewritten on way out.

/// Ad-click and campaign ids: safe on any site.
const GLOBAL_PARAMS: &[&str] = &[
    "fbclid",
    "gclid",
    "gclsrc",
    "dclid",
    "wbraid",
    "gbraid",
    "msclkid",
    "yclid",
    "twclid",
    "ttclid",
    "li_fat_id",
    "mc_cid",
    "mc_eid",
    "igshid",
    "srsltid",
    "s_cid",
    "_hsenc",
    "_hsmi",
    "_openstat",
    "vero_conv",
    "vero_id",
    "oly_anon_id",
    "oly_enc_id",
];

/// Analytics prefixes: Google, Matomo, `HubSpot`.
const GLOBAL_PREFIXES: &[&str] = &["utm_", "pk_", "mtm_", "piwik_", "hsa_"];

/// One site's tracking params.
struct SiteRule {
    /// Host and subdomains. `brand.*`: any country domain.
    hosts: &'static [&'static str],
    params: &'static [&'static str],
    prefixes: &'static [&'static str],
}

const SITE_RULES: &[SiteRule] = &[
    SiteRule {
        hosts: &["youtube.com", "youtu.be"],
        params: &[
            "si",
            "feature",
            "pp",
            "ab_channel",
            "embeds_referring_euri",
            "source_ve_path",
        ],
        prefixes: &[],
    },
    SiteRule {
        hosts: &["x.com", "twitter.com"],
        params: &["s", "t", "ref_src", "ref_url"],
        prefixes: &[],
    },
    SiteRule {
        hosts: &["instagram.com", "threads.net", "threads.com"],
        params: &["igsh", "ig_rid", "ig_mid", "xmt"],
        prefixes: &[],
    },
    SiteRule {
        hosts: &[
            "facebook.com",
            "fb.com",
            "fb.watch",
            "fb.me",
            "messenger.com",
        ],
        params: &[
            "mibextid",
            "rdid",
            "share_url",
            "refid",
            "ref",
            "fref",
            "hc_ref",
            "sfnsn",
            "wtsid",
            "paipv",
            "eav",
            "comment_tracking",
            "notif_id",
            "notif_t",
        ],
        // __tn__, __cft__[0], __xts__[0] ...
        prefixes: &["__"],
    },
    SiteRule {
        hosts: &["linkedin.com", "lnkd.in"],
        params: &[
            "trk",
            "trackingid",
            "lipi",
            "midtoken",
            "midsig",
            "trkemail",
            "ebp",
            "refid",
            "otptoken",
            "original_referer",
            "rcm",
        ],
        prefixes: &[],
    },
    SiteRule {
        hosts: &["tiktok.com"],
        params: &[
            "_t",
            "_r",
            "is_from_webapp",
            "sender_device",
            "sender_web_id",
            "web_id",
            "u_code",
            "tt_from",
            "is_copy_url",
            "share_app_id",
            "share_link_id",
            "share_iid",
            "ug_btm",
            "checksum",
        ],
        prefixes: &[],
    },
    SiteRule {
        hosts: &["reddit.com", "redd.it"],
        params: &["share_id", "ref", "ref_source", "rdt", "correlation_id"],
        prefixes: &[],
    },
    SiteRule {
        hosts: &["spotify.com"],
        params: &["si", "nd", "_branch_match_id", "_branch_referrer"],
        prefixes: &[],
    },
    SiteRule {
        hosts: &["twitch.tv"],
        params: &["tt_content", "tt_medium"],
        prefixes: &[],
    },
    // YouTube thumbnail CDN: rendering hints.
    SiteRule {
        hosts: &["ytimg.com"],
        params: &["sqp", "rs", "usqp"],
        prefixes: &[],
    },
    SiteRule {
        hosts: &["amazon.*"],
        params: &[
            "tag",
            "ref",
            "ref_",
            "ascsubtag",
            "linkcode",
            "linkid",
            "camp",
            "creative",
            "creativeasin",
            "qid",
            "sr",
            "sprefix",
            "crid",
            "dib",
            "dib_tag",
            "content-id",
            "social_share",
        ],
        prefixes: &["pf_rd_", "pd_rd_"],
    },
    SiteRule {
        hosts: &["ebay.*"],
        params: &[
            "mkcid", "mkevt", "mkrid", "ssspo", "sssrc", "ssuid", "campid", "toolid", "customid",
            "mkpid", "ul_noapp", "amdata",
        ],
        prefixes: &[],
    },
    SiteRule {
        hosts: &["aliexpress.*"],
        params: &[
            "spm",
            "scm",
            "aff_platform",
            "aff_trace_key",
            "aff_fcid",
            "aff_fsk",
            "terminal_id",
            "pdp_npi",
            "gatewayadapt",
            "algo_pvid",
            "algo_exp_id",
            "utparam-url",
        ],
        prefixes: &[],
    },
];

/// Text with every `http(s)` link cleaned.
pub(crate) fn clean_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (start, end) in url_spans(text) {
        // Bad span: link goes out uncleaned; no words lost.
        if let (Some(before), Some(url)) = (text.get(at..start), text.get(start..end)) {
            out.push_str(before);
            out.push_str(&clean_url(url));
            at = end;
        }
    }
    out.push_str(text.get(at..).unwrap_or_default());
    out
}

/// Byte ranges of each `http://` or `https://` link.
pub(crate) fn url_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut from = 0;
    while let Some(start) = url_start(text, from) {
        let end = url_end(text, start);
        spans.push((start, end));
        from = end;
    }
    spans
}

/// Start of next link at or after `from`.
fn url_start(text: &str, from: usize) -> Option<usize> {
    let mut at = from;
    while let Some(offset) = text.get(at..)?.find("http") {
        let index = at.saturating_add(offset);
        let rest = text.get(index..)?;
        if rest.starts_with("http://") || rest.starts_with("https://") {
            return Some(index);
        }
        at = index.saturating_add("http".len());
    }
    None
}

/// End of link at `start`: whitespace, quote or angle bracket.
///
/// Trailing punctuation is sentence's; `)`/`]` too unless opened.
fn url_end(text: &str, start: usize) -> usize {
    let tail = text.as_bytes().get(start..).unwrap_or_default();
    let mut link = tail
        .split(|byte| {
            matches!(
                byte,
                b' ' | b'\t' | b'\n' | b'\r' | b'<' | b'>' | b'"' | b'\''
            )
        })
        .next()
        .unwrap_or_default();
    // Closers counted once. Per-char recount was quadratic.
    let mut parens = unopened(link, b'(', b')');
    let mut brackets = unopened(link, b'[', b']');
    while let Some((&last, shorter)) = link.split_last() {
        let unopened = match last {
            b'.' | b',' | b';' | b':' | b'!' | b'?' => None,
            b')' => Some(&mut parens),
            b']' => Some(&mut brackets),
            _ => break,
        };
        if let Some(count) = unopened {
            if *count == 0 {
                break;
            }
            *count = count.saturating_sub(1);
        }
        link = shorter;
    }
    start.saturating_add(link.len())
}

/// Closers beyond openers in `span`.
fn unopened(span: &[u8], open: u8, close: u8) -> usize {
    let (mut opens, mut closes) = (0_usize, 0_usize);
    for byte in span {
        if *byte == open {
            opens = opens.saturating_add(1);
        } else if *byte == close {
            closes = closes.saturating_add(1);
        }
    }
    closes.saturating_sub(opens)
}

/// Link with tracking params removed; fragment kept.
pub(crate) fn clean_url(url: &str) -> String {
    let host = host_of(url);
    // `#` kept; a `?` after it belongs to fragment.
    let (before_fragment, fragment) = url
        .find('#')
        .and_then(|at| Some((url.get(..at)?, url.get(at..)?)))
        .unwrap_or((url, ""));
    let (mut base, query) = match before_fragment.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (before_fragment, None),
    };

    // Amazon click source is a path segment: `/dp/.../ref=sr_1_3`.
    if host_matches(&host, "amazon.*") {
        if let Some((path, _)) = base.split_once("/ref=") {
            base = path;
        }
    }

    let Some(query) = query else {
        return format!("{base}{fragment}");
    };
    let site = SITE_RULES.iter().find(|rule| {
        rule.hosts
            .iter()
            .any(|host_rule| host_matches(&host, host_rule))
    });

    let kept: Vec<&str> = query
        .split('&')
        .filter(|param| !param.is_empty())
        .filter(|param| {
            let name = param
                .split('=')
                .next()
                .unwrap_or(param)
                .to_ascii_lowercase();
            !is_tracking(&name, site)
        })
        .collect();

    if kept.is_empty() {
        format!("{base}{fragment}")
    } else {
        format!("{base}?{}{fragment}", kept.join("&"))
    }
}

fn is_tracking(name: &str, site: Option<&SiteRule>) -> bool {
    if GLOBAL_PARAMS.contains(&name)
        || GLOBAL_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
    {
        return true;
    }
    site.is_some_and(|rule| {
        rule.params.contains(&name) || rule.prefixes.iter().any(|prefix| name.starts_with(prefix))
    })
}

/// Lowercased host without user info, port, `www.`, `m.`.
fn host_of(url: &str) -> String {
    let Some((_, rest)) = url.split_once("://") else {
        return String::new();
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
    for prefix in ["www.", "m."] {
        if let Some(stripped) = host.strip_prefix(prefix) {
            return stripped.to_string();
        }
    }
    host
}

/// `example.com`: domain and subdomains. `brand.*`: any country.
fn host_matches(host: &str, pattern: &str) -> bool {
    if let Some(brand) = pattern.strip_suffix('*') {
        // `amazon.co.uk` at start, `smile.amazon.de` after a dot.
        return host
            .find(brand)
            .and_then(|at| host.get(..at))
            .is_some_and(|before| before.is_empty() || before.ends_with('.'));
    }
    host == pattern || host.ends_with(&format!(".{pattern}"))
}

#[cfg(test)]
mod tests {
    use super::{clean_text, clean_url, url_spans};

    #[test]
    fn campaign_and_click_ids_go_and_content_stays() {
        assert_eq!(
            clean_url("https://example.org/page?utm_source=x&id=7&fbclid=abc"),
            "https://example.org/page?id=7"
        );
        assert_eq!(
            clean_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ&si=tracker&t=42"),
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=42"
        );
        // Nothing to remove: unchanged.
        assert_eq!(
            clean_url("https://example.org/a?b=c#frag"),
            "https://example.org/a?b=c#frag"
        );
        // No lone `?`; fragment stays.
        assert_eq!(
            clean_url("https://youtu.be/abc?si=xyz#t=1"),
            "https://youtu.be/abc#t=1"
        );
    }

    #[test]
    fn site_rules_apply_to_their_hosts_only() {
        // `ref` only for Facebook, Amazon.
        assert_eq!(
            clean_url("https://example.org/?ref=home"),
            "https://example.org/?ref=home"
        );
        assert_eq!(
            clean_url("https://m.facebook.com/story?ref=home&story=1&__tn__=x"),
            "https://m.facebook.com/story?story=1"
        );
        // Any country domain; path-style click id.
        assert_eq!(
            clean_url("https://www.amazon.co.uk/dp/B0ABCDEF/ref=sr_1_3?tag=aff-21&th=1"),
            "https://www.amazon.co.uk/dp/B0ABCDEF?th=1"
        );
        assert_eq!(
            clean_url("https://smile.amazon.de/dp/B0ABCDEF?pf_rd_r=xyz"),
            "https://smile.amazon.de/dp/B0ABCDEF"
        );
        assert_eq!(
            clean_url("https://notamazon.example/?tag=keep"),
            "https://notamazon.example/?tag=keep"
        );
    }

    #[test]
    fn links_are_found_in_prose_and_punctuation_is_left_to_the_sentence() {
        let text =
            "see https://example.org/a?utm_medium=mail, and (https://example.org/b?gclid=1).";
        assert_eq!(url_spans(text).len(), 2);
        assert_eq!(
            clean_text(text),
            "see https://example.org/a, and (https://example.org/b)."
        );
        // Bracket link opened is part of it.
        assert_eq!(
            clean_text("https://en.wikipedia.org/wiki/Foo_(bar)?utm_x=1"),
            "https://en.wikipedia.org/wiki/Foo_(bar)"
        );
        // Not a link: no `://`.
        assert_eq!(clean_text("httpx and http:/x"), "httpx and http:/x");
        assert_eq!(clean_text(""), "");
    }

    #[test]
    fn closers_after_a_link_are_counted_once() {
        // `)` recount made this half a minute.
        let text = format!("https://example.org/a{}", ")".repeat(100_000));
        assert_eq!(url_spans(&text), [(0, 21)]);
        // Link's own pairs stay; rest is sentence.
        let text = "(see https://example.org/(a)[b]).]";
        assert_eq!(url_spans(text), [(5, 31)]);
    }
}
