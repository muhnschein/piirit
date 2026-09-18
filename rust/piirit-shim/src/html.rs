//! The core's message HTML, as text.
//!
//! A long message is not sent whole. The sending core cuts the body at
//! `DC_DESIRED_TEXT_LEN` and puts the rest in an HTML part, so what
//! arrives in `text` ends in `[...]` and the whole of it is only behind
//! `get_message_html`. Every client that offers "show full message" goes
//! that way; there is no other way to the rest of the words.
//!
//! What comes back is a mail's HTML, and this app has no business
//! rendering one: a `WebView` for a message body is the tracking pixel
//! the whole of `markdown.rs` exists to keep out. So the markup is taken
//! off and the words are kept, which is what the reader wanted -- and
//! what parla does with the same answer, for the same reason.
//!
//! This is not an HTML parser and cannot become one. It is four
//! substitutions over the shape mail bodies actually have, and its output
//! is plain text: whatever it fails to strip is shown as the angle
//! brackets it is, never as markup.

/// The words in a message's HTML part, with the tags taken out.
///
/// Block ends become line breaks, so paragraphs and list items stay
/// separate lines rather than running together; `head`, `script` and
/// `style` go out whole, contents and all, because none of what is in
/// them is anything the reader wrote.
///
/// Markup with no tag in it at all is not markup: its newlines are the
/// reader's own and are left where they are. Everything else goes
/// through the rule below, where a newline in the source is only
/// whitespace.
pub(crate) fn to_text(html: &str) -> String {
    if !html.contains('<') {
        return collapse_blank_lines(decode_entities(html).trim());
    }
    let text = drop_hidden(html);
    let text = replace_tags(&text);
    let text = decode_entities(&text);
    collapse_blank_lines(text.trim())
}

/// `<head>`, `<script>` and `<style>` with everything inside them.
///
/// Written as a scan rather than a regex: the crate has no regex engine
/// and is not gaining one for four keywords.
fn drop_hidden(html: &str) -> String {
    const HIDDEN: [&str; 3] = ["head", "script", "style"];
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    'outer: while !rest.is_empty() {
        let Some(open) = rest.find('<') else {
            out.push_str(rest);
            break;
        };
        let (before, from_tag) = rest.split_at(open);
        out.push_str(before);
        for name in HIDDEN {
            if opens(from_tag, name) {
                // Everything up to the matching close, or -- for markup
                // that never closes it -- the rest of the document,
                // which is the safer of the two readings.
                let closing = format!("</{name}");
                rest = match find_ignoring_case(from_tag, &closing) {
                    Some(at) => &from_tag[at..],
                    None => "",
                };
                // Past the close tag itself.
                rest = rest.find('>').map_or("", |at| &rest[at + 1..]);
                continue 'outer;
            }
        }
        out.push('<');
        rest = &from_tag[1..];
    }
    out
}

/// Whether `text` starts with an opening tag named `name`.
fn opens(text: &str, name: &str) -> bool {
    let Some(after) = text.strip_prefix('<') else {
        return false;
    };
    let Some(after) = strip_prefix_ignoring_case(after, name) else {
        return false;
    };
    // `<head>` and `<style type=...>` open one; `<header>` does not.
    after.starts_with('>') || after.starts_with(|c: char| c.is_ascii_whitespace())
}

/// Every tag out, with the ones that end a line leaving one behind.
///
/// A newline in the markup is *not* a line break: in HTML it is
/// whitespace like a space, and the break is the tag. The core's own
/// long-message part is written `line<br/>` with a newline after the tag,
/// so counting both would put a blank line between every line of a message
/// the core had cut. Whitespace between the markup is collapsed the way a
/// browser collapses it: to one space in the middle of a line, and to
/// nothing at either end of one.
///
/// Every break is worth one line break and no more, `</p>` included. Two
/// in a row is still two, so a blank line the reader typed -- which the
/// core writes as two `<br/>` -- arrives as the blank line it was.
fn replace_tags(html: &str) -> String {
    // The tags a reader would see as a line ending. `br` is the break
    // itself; the rest are blocks, and their *closing* tag is where the
    // line ends.
    const BREAKS: [&str; 2] = ["br", "hr"];
    const BLOCKS: [&str; 12] = [
        "p", "div", "section", "article", "li", "tr", "h1", "h2", "h3", "h4", "h5", "h6",
    ];
    let mut out = String::with_capacity(html.len());
    // A run of whitespace waiting to be written as the single space it
    // stands for. It is only written once something follows it on the
    // same line, so whitespace before a break is dropped rather than
    // left hanging off the end.
    let mut space = false;
    // Whether the line being written is still empty, so leading
    // whitespace has nothing to be a space between.
    let mut fresh = true;
    let mut rest = html;
    while !rest.is_empty() {
        let Some(open) = rest.find('<') else {
            push_words(rest, &mut out, &mut space, &mut fresh);
            break;
        };
        push_words(&rest[..open], &mut out, &mut space, &mut fresh);
        let from_tag = &rest[open..];
        // A `<` that does not begin a tag is a stray angle bracket in
        // the text -- "3 < 4" -- and stays one. A tag's name starts
        // with a letter, a `/`, or the `!` of a comment or a doctype;
        // anything else after the bracket is arithmetic.
        if !from_tag[1..].starts_with(|c: char| c.is_ascii_alphabetic() || c == '/' || c == '!') {
            push_words("<", &mut out, &mut space, &mut fresh);
            rest = &from_tag[1..];
            continue;
        }
        // A `<` with no `>` after it is the same thing at the end of the
        // text: keep it and stop.
        let Some(close) = from_tag.find('>') else {
            push_words(from_tag, &mut out, &mut space, &mut fresh);
            break;
        };
        let tag = &from_tag[1..close];
        let name = tag
            .trim_start_matches('/')
            .split(|c: char| c.is_ascii_whitespace() || c == '/')
            .next()
            .unwrap_or("");
        let ends_line = BREAKS.iter().any(|kind| name.eq_ignore_ascii_case(kind))
            || (tag.starts_with('/') && BLOCKS.iter().any(|kind| name.eq_ignore_ascii_case(kind)));
        if ends_line {
            // Whatever whitespace led up to the break was the markup's
            // own layout, and the break is where the line ends.
            space = false;
            fresh = true;
            out.push('\n');
        }
        rest = &from_tag[close + 1..];
    }
    out
}

/// Write the words of one run of text, with its whitespace collapsed.
///
/// `space` carries a run of whitespace that has not been written yet, and
/// `fresh` says the line is still empty -- so a run at the start of a
/// line, or one that a break comes along and ends, leaves nothing behind.
fn push_words(text: &str, out: &mut String, space: &mut bool, fresh: &mut bool) {
    for character in text.chars() {
        if character.is_whitespace() {
            *space = true;
            continue;
        }
        if *space {
            if !*fresh {
                out.push(' ');
            }
            *space = false;
        }
        out.push(character);
        *fresh = false;
    }
}

/// The handful of entities a mail body actually carries. `&amp;` last, so
/// an `&amp;lt;` in the source does not turn into a `<`.
fn decode_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Three line breaks in a row are the markup showing through: two is a
/// blank line, which the reader may well have typed, and more than that
/// is nothing they can have meant.
fn collapse_blank_lines(text: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut blanks = 0;
    for line in text.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            blanks += 1;
            if blanks > 1 {
                continue;
            }
        } else {
            blanks = 0;
        }
        out.push(line);
    }
    out.join("\n")
}

/// `haystack.find(needle)` without regard to case, for ASCII needles.
///
/// Compared a window at a time rather than by lowercasing the rest of the
/// document at every position, which allocated a copy of the whole
/// remaining message per character looked at: a long message's HTML part
/// is megabytes, and the search for `</head` starts at the front of it.
///
/// The index is a character boundary because a match is ASCII, and no
/// byte of a multi-byte character is.
fn find_ignoring_case(haystack: &str, needle: &str) -> Option<usize> {
    let needle = needle.as_bytes();
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle))
}

/// `text.strip_prefix(prefix)` without regard to case, for ASCII
/// prefixes.
fn strip_prefix_ignoring_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    if text.len() >= prefix.len() && text[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&text[prefix.len()..])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::to_text;

    #[test]
    fn the_words_survive_and_the_tags_do_not() {
        assert_eq!(
            to_text("<html><body><p>Buy milk</p><p>Call Ada</p></body></html>"),
            "Buy milk\nCall Ada"
        );
    }

    #[test]
    fn a_list_stays_one_item_to_a_line() {
        assert_eq!(
            to_text("<ul><li>one</li><li>two</li><li>three</li></ul>"),
            "one\ntwo\nthree"
        );
    }

    #[test]
    fn a_break_breaks_the_line() {
        assert_eq!(to_text("first<br>second<br/>third"), "first\nsecond\nthird");
    }

    #[test]
    fn what_the_reader_never_wrote_goes_out_whole() {
        assert_eq!(
            to_text(
                "<head><title>ignored</title></head><body>\
                 <style>p { color: red }</style>kept\
                 <script>alert('no')</script></body>"
            ),
            "kept"
        );
    }

    #[test]
    fn a_tag_whose_name_merely_starts_the_same_is_not_one_of_them() {
        // `<header>` is not `<head>`, and its words are the reader's.
        assert_eq!(to_text("<header>Shopping</header>list"), "Shoppinglist");
    }

    #[test]
    fn entities_come_back_as_the_characters_they_stand_for() {
        assert_eq!(
            to_text("<p>a &lt;b&gt; &amp; &quot;c&quot;&nbsp;d</p>"),
            "a <b> & \"c\" d"
        );
    }

    #[test]
    fn an_escaped_entity_is_not_decoded_twice() {
        // `&amp;lt;` is the reader writing `&lt;`, not writing `<`.
        assert_eq!(to_text("<p>&amp;lt;</p>"), "&lt;");
    }

    #[test]
    fn a_stray_angle_bracket_is_text_like_any_other() {
        // Nothing here begins a tag: a name cannot start with a space
        // or a digit.
        assert_eq!(to_text("<p>3 < 4 and 5 > 2</p>"), "3 < 4 and 5 > 2");
        assert_eq!(to_text("a < b"), "a < b");
    }

    #[test]
    fn a_comment_is_not_the_readers_words() {
        assert_eq!(to_text("<p>kept<!-- hidden --></p>"), "kept");
    }

    #[test]
    fn the_markups_own_newlines_are_not_the_readers() {
        // Pretty-printed markup: every one of those newlines is the mail
        // laying itself out, and none of them is a line the reader
        // typed. The break is the tag.
        assert_eq!(
            to_text("<div>\n<p>one</p>\n\n<p>two</p>\n</div>"),
            "one\ntwo"
        );
    }

    #[test]
    fn the_shape_the_core_writes_a_cut_message_in_comes_back_as_typed() {
        // What `get_message_html` answers with, as the pinned
        // deltachat-rpc-server writes it: its own head, its own body
        // tag, and the message with every newline turned into `<br/>`
        // *followed by a newline*. Counting both is what double-spaced
        // every long message on the phone.
        let html = "<!DOCTYPE html>\n<html><head>\n\
                    <meta http-equiv=\"Content-Type\" \
                    content=\"text/html; charset=utf-8\" />\n\
                    <meta name=\"color-scheme\" content=\"light dark\" />\n\
                    </head><body dir=\"auto\" style=\"unicode-bidi: plaintext\">\n\
                    Shshs<br/>\nHzhshs<br/>\nHsbsbs<br/>\n</body></html>\n";
        assert_eq!(to_text(html), "Shshs\nHzhshs\nHsbsbs");
    }

    #[test]
    fn a_long_message_comes_back_with_every_line_it_went_out_with() {
        // The whole point of the HTML part is the words that did not fit
        // in `text`, so a reader that drops any of them has kept the bug
        // it was written to fix. Forty lines, which is past the length
        // the core cuts at.
        let mut html = String::from(
            "<!DOCTYPE html>\n<html><head>\n</head>\
             <body dir=\"auto\" style=\"unicode-bidi: plaintext\">\n",
        );
        for number in 1..=40 {
            use std::fmt::Write as _;
            let _ = writeln!(html, "- [ ] item {number}<br/>");
        }
        html.push_str("</body></html>\n");

        let words = to_text(&html);
        let lines: Vec<&str> = words.lines().collect();
        assert_eq!(
            lines.len(),
            40,
            "forty lines came back as {}: {words:?}",
            lines.len()
        );
        assert_eq!(lines[0], "- [ ] item 1");
        assert_eq!(lines[39], "- [ ] item 40");
    }

    #[test]
    fn a_blank_line_the_reader_typed_is_still_a_blank_line() {
        // Two newlines in the message are two `<br/>` in the part, and
        // the reader meant the gap between the paragraphs.
        assert_eq!(
            to_text("<body>\none<br/>\n<br/>\ntwo<br/>\n</body>"),
            "one\n\ntwo"
        );
    }

    #[test]
    fn a_line_broken_across_the_markup_is_one_line() {
        // A newline inside a run of words is whitespace, as it is in
        // every browser: it joins rather than breaks.
        assert_eq!(
            to_text("<p>Buy milk\n   and bread</p>"),
            "Buy milk and bread"
        );
    }

    #[test]
    fn nothing_in_is_nothing_out() {
        assert_eq!(to_text(""), "");
        assert_eq!(to_text("   \n  "), "");
    }

    #[test]
    fn text_with_no_markup_at_all_is_left_as_it_is() {
        assert_eq!(to_text("just words\nand a line"), "just words\nand a line");
    }

    #[test]
    fn a_tag_that_never_closes_takes_the_rest_with_it() {
        // Whichever way this is read, none of it is the reader's words.
        assert_eq!(to_text("kept<script>alert('no')"), "kept");
    }
}
