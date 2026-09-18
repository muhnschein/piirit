//! Tracking parameters taken out of links.
//!
//! A link pasted from a browser or another app carries whatever the site
//! appended to it: click ids, campaign tags, the sharer's account. None of
//! it is the page, and all of it tells the site who sent the link to whom.
//! The rules are parla's (github.com/trufae/parla, `link_cleaner.vala`):
//! parameters that carry content -- a video id, a timestamp, a search --
//! are kept, and everything not on a list is left byte for byte as it was.
//!
//! Nothing here fetches anything. The text is rewritten on the way out and
//! the reader sees the result in their own bubble.

/// Ad-click and campaign ids that platforms append regardless of the
/// destination site, so they are safe to drop anywhere.
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

/// Prefixes of the same kind: Google Analytics, Matomo, `HubSpot` ads.
const GLOBAL_PREFIXES: &[&str] = &["utm_", "pk_", "mtm_", "piwik_", "hsa_"];

/// What one site appends to its own links.
struct SiteRule {
    /// A host matches itself and its subdomains; `brand.*` matches the
    /// brand under any country domain (`amazon.co.uk`, `smile.amazon.de`).
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
    // YouTube's thumbnail CDN: rendering hints and signatures the plain
    // picture does not need.
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

/// The text with every `http(s)` link in it cleaned. Anything that is not
/// a link passes through unchanged.
pub(crate) fn clean_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (start, end) in url_spans(text) {
        out.push_str(&text[at..start]);
        out.push_str(&clean_url(&text[start..end]));
        at = end;
    }
    out.push_str(&text[at..]);
    out
}

/// The byte ranges of every `http://` or `https://` link in the text, in
/// order.
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

/// Where the next link starts at or after `from`, if there is one.
fn url_start(text: &str, from: usize) -> Option<usize> {
    let mut at = from;
    while let Some(offset) = text.get(at..)?.find("http") {
        let index = at + offset;
        let rest = &text[index..];
        if rest.starts_with("http://") || rest.starts_with("https://") {
            return Some(index);
        }
        at = index + 4;
    }
    None
}

/// Where a link starting at `start` ends: at whitespace or a quote or an
/// angle bracket, less any trailing punctuation that belongs to the
/// sentence rather than the link. A `)` or `]` counts as the sentence's
/// only when the link itself has no opening one for it.
fn url_end(text: &str, start: usize) -> usize {
    let bytes = text.as_bytes();
    let mut end = start;
    while end < bytes.len() {
        if matches!(
            bytes[end],
            b' ' | b'\t' | b'\n' | b'\r' | b'<' | b'>' | b'"' | b'\''
        ) {
            break;
        }
        end += 1;
    }
    while end > start {
        let last = bytes[end - 1];
        let trailing = match last {
            b'.' | b',' | b';' | b':' | b'!' | b'?' => true,
            b')' => !balanced(&bytes[start..end], b'(', b')'),
            b']' => !balanced(&bytes[start..end], b'[', b']'),
            _ => false,
        };
        if !trailing {
            break;
        }
        end -= 1;
    }
    end
}

/// Whether every `close` in the span has an `open` before it.
fn balanced(span: &[u8], open: u8, close: u8) -> bool {
    let mut depth: i32 = 0;
    for byte in span {
        if *byte == open {
            depth += 1;
        } else if *byte == close {
            depth -= 1;
        }
    }
    depth >= 0
}

/// One link with its tracking parameters taken out. The fragment is kept,
/// and so is every parameter not on a list.
pub(crate) fn clean_url(url: &str) -> String {
    let host = host_of(url);
    let fragment_at = url.find('#');
    // `?` after `#` is part of the fragment. `map_or` rather than
    // `is_none_or`, which arrived in Rust 1.82, past the 1.75 floor.
    let query_at = url
        .find('?')
        .filter(|q| fragment_at.map_or(true, |f| *q < f));

    let base_end = query_at.or(fragment_at).unwrap_or(url.len());
    let mut base = &url[..base_end];
    let fragment = fragment_at.map_or("", |at| &url[at..]);

    // Amazon appends the click source as a path segment rather than a
    // parameter: `/dp/B0XXXXXXXX/ref=sr_1_3`.
    if host_matches(&host, "amazon.*") {
        if let Some(at) = base.find("/ref=") {
            base = &base[..at];
        }
    }

    let Some(query_at) = query_at else {
        return format!("{base}{fragment}");
    };
    let query = &url[query_at + 1..fragment_at.unwrap_or(url.len())];
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

/// The host, lowercased, without user info, a port, or a leading `www.`
/// or `m.`.
fn host_of(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return String::new();
    };
    let rest = &url[scheme_end + 3..];
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let mut host = rest[..end].to_ascii_lowercase();
    if let Some(at) = host.rfind('@') {
        host = host[at + 1..].to_string();
    }
    if let Some(colon) = host.find(':') {
        host.truncate(colon);
    }
    for prefix in ["www.", "m."] {
        if let Some(stripped) = host.strip_prefix(prefix) {
            return stripped.to_string();
        }
    }
    host
}

/// `example.com` matches the domain and its subdomains; `brand.*` matches
/// the brand under any country domain.
fn host_matches(host: &str, pattern: &str) -> bool {
    if let Some(brand) = pattern.strip_suffix('*') {
        // `brand` still ends in the dot: `amazon.` matches `amazon.co.uk`
        // at the start, and `smile.amazon.de` after a dot.
        return host
            .find(brand)
            .is_some_and(|at| at == 0 || host.as_bytes()[at - 1] == b'.');
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
        // Nothing to remove: byte for byte as it came.
        assert_eq!(
            clean_url("https://example.org/a?b=c#frag"),
            "https://example.org/a?b=c#frag"
        );
        // Every parameter gone leaves no lone `?`, and the fragment stays.
        assert_eq!(
            clean_url("https://youtu.be/abc?si=xyz#t=1"),
            "https://youtu.be/abc#t=1"
        );
    }

    #[test]
    fn site_rules_apply_to_their_hosts_only() {
        // `ref` is Facebook's and Amazon's, not everyone's.
        assert_eq!(
            clean_url("https://example.org/?ref=home"),
            "https://example.org/?ref=home"
        );
        assert_eq!(
            clean_url("https://m.facebook.com/story?ref=home&story=1&__tn__=x"),
            "https://m.facebook.com/story?story=1"
        );
        // A brand under any country domain, and its path-style click id.
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
        // A bracket the link itself opened is part of it.
        assert_eq!(
            clean_text("https://en.wikipedia.org/wiki/Foo_(bar)?utm_x=1"),
            "https://en.wikipedia.org/wiki/Foo_(bar)"
        );
        // Not a link: no scheme separator.
        assert_eq!(clean_text("httpx and http:/x"), "httpx and http:/x");
        assert_eq!(clean_text(""), "");
    }
}
