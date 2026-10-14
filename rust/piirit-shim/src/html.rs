//! Core's message HTML as plain text.
//!
//! Long messages cut at `DC_DESIRED_TEXT_LEN`; rest in HTML part (`get_message_html`).
//! Not rendered: `WebView` for mail is the tracking pixel `markdown.rs` keeps out.
//! Words kept, markup dropped (like parla). Not a parser: four substitutions;
//! unstripped `<` shows as text.

/// Message HTML part, tags removed.
///
/// Block ends become line breaks. `head`, `script`, `style` go whole: nothing reader wrote.
///
/// No tags: newlines kept. Else markup newlines are whitespace (`replace_tags`).
pub(crate) fn to_text(html: &str) -> String {
    if !html.contains('<') {
        return collapse_blank_lines(decode_entities(html).trim());
    }
    let text = drop_hidden(html);
    let text = replace_tags(&text);
    let text = decode_entities(&text);
    collapse_blank_lines(text.trim())
}

/// `<head>`, `<script>`, `<style>` with contents.
///
/// Scan, not regex: crate has no regex engine.
fn drop_hidden(html: &str) -> String {
    const HIDDEN: [&str; 3] = ["head", "script", "style"];
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    'outer: while !rest.is_empty() {
        let Some((before, after_bracket)) = rest.split_once('<') else {
            out.push_str(rest);
            break;
        };
        out.push_str(before);
        for name in HIDDEN {
            if opens(after_bracket, name) {
                // To matching close; none: rest of doc dropped (safer reading).
                let closing = format!("</{name}");
                rest = find_ignoring_case(after_bracket, &closing)
                    .and_then(|at| after_bracket.get(at..))
                    .unwrap_or("");
                // Past close tag.
                rest = rest.split_once('>').map_or("", |(_, after)| after);
                continue 'outer;
            }
        }
        out.push('<');
        rest = after_bracket;
    }
    out
}

/// Whether text after `<` opens tag `name`.
fn opens(after_bracket: &str, name: &str) -> bool {
    let Some(after) = strip_prefix_ignoring_case(after_bracket, name) else {
        return false;
    };
    // `<header>` is not `<head>`.
    after.starts_with('>') || after.starts_with(|c: char| c.is_ascii_whitespace())
}

/// Tags out; line-ending tags leave one line break.
///
/// Markup newlines are whitespace: the tag is the break. Core writes `<br/>` then newline;
/// counting both double-spaces cut messages. Whitespace collapsed like browser: one space
/// mid-line, none at line ends.
///
/// Each break is one line break, `</p>` included. Two in a row stay two.
fn replace_tags(html: &str) -> String {
    // `br` breaks; blocks break at close tag.
    const BREAKS: [&str; 2] = ["br", "hr"];
    const BLOCKS: [&str; 12] = [
        "p", "div", "section", "article", "li", "tr", "h1", "h2", "h3", "h4", "h5", "h6",
    ];
    let mut out = String::with_capacity(html.len());
    // Pending whitespace: one space once text follows. Lost at break.
    let mut space = false;
    // Line empty: leading whitespace dropped.
    let mut fresh = true;
    let mut rest = html;
    while !rest.is_empty() {
        let Some((words, after_bracket)) = rest.split_once('<') else {
            push_words(rest, &mut out, &mut space, &mut fresh);
            break;
        };
        push_words(words, &mut out, &mut space, &mut fresh);
        // `<` not starting a tag stays text: "3 < 4".
        if !after_bracket.starts_with(|c: char| c.is_ascii_alphabetic() || c == '/' || c == '!') {
            push_words("<", &mut out, &mut space, &mut fresh);
            rest = after_bracket;
            continue;
        }
        // No `>`: text, stop.
        let Some((tag, after_tag)) = after_bracket.split_once('>') else {
            push_words("<", &mut out, &mut space, &mut fresh);
            push_words(after_bracket, &mut out, &mut space, &mut fresh);
            break;
        };
        let name = tag
            .trim_start_matches('/')
            .split(|c: char| c.is_ascii_whitespace() || c == '/')
            .next()
            .unwrap_or("");
        let ends_line = BREAKS.iter().any(|kind| name.eq_ignore_ascii_case(kind))
            || (tag.starts_with('/') && BLOCKS.iter().any(|kind| name.eq_ignore_ascii_case(kind)));
        if ends_line {
            // Whitespace before break is layout: drop.
            space = false;
            fresh = true;
            out.push('\n');
        }
        rest = after_tag;
    }
    out
}

/// Write words, whitespace collapsed.
///
/// `space`: pending. `fresh`: line empty, no leading space.
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

/// Mail entities. `&amp;` last: `&amp;lt;` stays `&lt;`.
fn decode_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Max one blank line: more is markup showing through.
fn collapse_blank_lines(text: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut blanks: u32 = 0;
    for line in text.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            blanks = blanks.saturating_add(1);
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

/// `haystack.find(needle)`, ASCII case ignored.
///
/// No lowercase copy per char: megabyte parts. Index on char boundary.
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

/// `strip_prefix`, ASCII case ignored.
///
/// `get`: slicing mid-char (`<ab😀`) panics; such text cannot match.
fn strip_prefix_ignoring_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    if head.eq_ignore_ascii_case(prefix) {
        text.get(prefix.len()..)
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
    fn a_tag_name_that_breaks_off_inside_a_character_is_only_text() {
        // `head` is 4 bytes: lands inside the emoji. Used to panic.
        assert_eq!(to_text("<ab😀 and more"), "<ab😀 and more");
    }

    #[test]
    fn a_tag_whose_name_merely_starts_the_same_is_not_one_of_them() {
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
        assert_eq!(to_text("<p>&amp;lt;</p>"), "&lt;");
    }

    #[test]
    fn a_stray_angle_bracket_is_text_like_any_other() {
        // Names cannot start with space or digit.
        assert_eq!(to_text("<p>3 < 4 and 5 > 2</p>"), "3 < 4 and 5 > 2");
        assert_eq!(to_text("a < b"), "a < b");
    }

    #[test]
    fn a_comment_is_not_the_readers_words() {
        assert_eq!(to_text("<p>kept<!-- hidden --></p>"), "kept");
    }

    #[test]
    fn the_markups_own_newlines_are_not_the_readers() {
        // Pretty-printed newlines: mail layout. Break is the tag.
        assert_eq!(
            to_text("<div>\n<p>one</p>\n\n<p>two</p>\n</div>"),
            "one\ntwo"
        );
    }

    #[test]
    fn the_shape_the_core_writes_a_cut_message_in_comes_back_as_typed() {
        // `get_message_html` (pinned deltachat-rpc-server): `<br/>` + newline per line.
        // Counting both double-spaced every long message.
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
        // HTML part holds what `text` cut: dropping a line reopens the bug.
        // Forty lines, past core's cut length.
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
        // Two newlines, two `<br/>`: a paragraph gap.
        assert_eq!(
            to_text("<body>\none<br/>\n<br/>\ntwo<br/>\n</body>"),
            "one\n\ntwo"
        );
    }

    #[test]
    fn a_line_broken_across_the_markup_is_one_line() {
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
        assert_eq!(to_text("kept<script>alert('no')"), "kept");
    }
}
