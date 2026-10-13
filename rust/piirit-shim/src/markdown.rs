//! Markdown in a message, rendered.
//!
//! Delta Chat messages are plain text by convention and Markdown by
//! habit: the other clients render `**bold**` and `` `code` `` and turn
//! links blue, so a message written on one of them arrives here full of
//! asterisks. Sailfish's Qt 5.6 has no Markdown of its own (`Text` learnt
//! it in 5.14), so this is a small renderer for the subset people
//! actually type -- emphasis, strikethrough, code, headings, links -- and
//! nothing that needs a block model.
//!
//! What it emits is `Text.StyledText`, and what makes that safe is that
//! it emits *only* what it chose to: every character of the message goes
//! out escaped, and the handful of tags are the ones this file writes. A
//! message body of `<img src="https://tracker/p.gif">` is why every label
//! in the app is pinned to plain text (`tests/qml_syntax.rs`); here it
//! stays a string of angle brackets. Links are the one thing
//! that can reach the network, and only on a tap.
//!
//! Rendered or off, and nothing between: a third mode that took the
//! markers out and kept the words would be a second way to not see the
//! formatting, so the setting offers the two a reader can tell apart.

use crate::links::url_spans;

/// The message rendered as `Text.StyledText`: every character escaped,
/// and the Markdown the reader wrote turned into the few tags Qt draws.
///
/// Line by line, because fences and headings are lines. A fence line is
/// a marker and nothing else -- it opens or closes a block and is never
/// shown, its language word included -- so it leaves no line behind: the
/// block's tag goes on the line after it, or the line before.
pub(crate) fn render(text: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut fence = Fence::default();
    for line in text.split('\n') {
        let trimmed = line.trim_start();
        if is_fence(trimmed) {
            fence.cross(&mut lines);
            continue;
        }
        let mut out = String::with_capacity(line.len().saturating_add(8));
        if fence.opening {
            out.push_str("<pre>");
            fence.opening = false;
        }
        if fence.inside {
            push_escaped(&mut out, line);
        } else {
            block(line, trimmed, &mut out);
        }
        lines.push(out);
    }
    fence.finish(&mut lines);
    join_styled(&lines)
}

/// A line that is a fence marker: three backticks or tildes, with or
/// without a language word after them.
fn is_fence(trimmed: &str) -> bool {
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

/// Where the walk stands with respect to code blocks, between lines.
#[derive(Default)]
struct Fence {
    /// Inside a block: lines are code, escaped and never styled.
    inside: bool,
    /// The next line drawn opens a block, and gets the tag in front.
    opening: bool,
}

impl Fence {
    /// A fence line was read: a block opens or closes here.
    ///
    /// The closing tag goes on the line before, since the marker leaves
    /// no line of its own -- unless nothing was drawn since the block
    /// opened, in which case there is no block.
    fn cross(&mut self, lines: &mut [String]) {
        if !self.inside {
            self.opening = true;
        } else if self.opening {
            // A block with nothing in it is no block at all.
            self.opening = false;
        } else {
            close_block(lines);
        }
        self.inside = !self.inside;
    }

    /// The text ended: a block that was never closed is still a block.
    fn finish(&self, lines: &mut [String]) {
        if self.inside && !self.opening {
            close_block(lines);
        }
    }
}

/// End the block on the last line drawn.
fn close_block(lines: &mut [String]) {
    if let Some(last) = lines.last_mut() {
        last.push_str("</pre>");
    }
}

/// The rendered lines, joined so that each one is drawn on a line.
///
/// A newline is not a line break in `Text.StyledText`: it is whitespace,
/// like a space, so a body joined with newlines is drawn as one running
/// paragraph however it was typed. Every line break in a message went
/// that way -- a to-do list arrived as a single sentence -- which is
/// what `<br>` is for.
///
/// Except where Qt breaks the line itself. `<pre>` keeps the newlines
/// inside it *and* starts a line of its own, and `</pre>` ends one, so a
/// `<br>` at either edge of a code block would leave a blank line
/// behind. Measured rather than assumed: `qml_message_lines.rs` puts
/// each of these shapes through a `Text` and counts the lines it comes
/// out as.
fn join_styled(lines: &[String]) -> String {
    let mut out = String::new();
    let mut in_pre = false;
    for (index, line) in lines.iter().enumerate() {
        if line.starts_with("<pre>") {
            in_pre = true;
        }
        out.push_str(line);
        let closes = line.ends_with("</pre>");
        if closes {
            in_pre = false;
        }
        let Some(next) = lines.get(index.saturating_add(1)) else {
            continue;
        };
        if closes || next.starts_with("<pre>") {
            // The tag is the break.
        } else if in_pre {
            // Inside a block, where a newline is a newline.
            out.push('\n');
        } else {
            out.push_str("<br>");
        }
    }
    out
}

/// One line outside a code block: a heading, or ordinary text.
fn block(line: &str, trimmed: &str, out: &mut String) {
    // A heading: one to six hashes, a space, the text.
    let hashes = trimmed.bytes().take_while(|byte| *byte == b'#').count();
    let heading = trimmed
        .get(hashes..)
        .and_then(|after| after.strip_prefix(' '))
        .filter(|_| (1..=6).contains(&hashes));
    if let Some(heading) = heading {
        let heading = heading.trim();
        out.push_str("<b>");
        inline(heading, out);
        out.push_str("</b>");
        return;
    }
    inline(line, out);
}

/// The characters `StyledText` reads as markup, escaped; everything else as
/// it is.
fn push_escaped(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

/// One line of ordinary text: emphasis, code, links, and everything else
/// escaped.
fn inline(line: &str, out: &mut String) {
    let links = url_spans(line);
    let mut at = 0;
    while let Some(rest) = line.get(at..).filter(|rest| !rest.is_empty()) {
        // A bare link, as it stands. Its own characters are never markup,
        // so a `_` in a path is a `_` and not an italic.
        if let Some(&(_, end)) = links.iter().find(|(start, _)| *start == at) {
            if let Some(url) = line.get(at..end) {
                push_link(out, url, url);
                at = end;
                continue;
            }
        }
        // A backslash escapes the marker after it.
        if let Some(after) = rest.strip_prefix('\\') {
            if let Some(next) = after.chars().next() {
                if "\\`*_~[]()#".contains(next) {
                    push_escaped(out, next.encode_utf8(&mut [0; 4]));
                    at = at.saturating_add(1).saturating_add(next.len_utf8());
                    continue;
                }
            }
        }
        if let Some(consumed) = code_span(rest, out)
            .or_else(|| link_span(rest, out))
            .or_else(|| emphasis_span(line, at, out))
        {
            at = at.saturating_add(consumed);
            continue;
        }
        // Whatever it is, one character of it, escaped.
        let Some(c) = rest.chars().next() else {
            break;
        };
        push_escaped(out, c.encode_utf8(&mut [0; 4]));
        at = at.saturating_add(c.len_utf8());
    }
}

/// `` `code` ``: the contents are shown as typed, markers and all.
fn code_span(rest: &str, out: &mut String) -> Option<usize> {
    let inner = rest.strip_prefix('`')?;
    let (code, _) = inner.split_once('`').filter(|(code, _)| !code.is_empty())?;
    out.push_str("<tt>");
    push_escaped(out, code);
    out.push_str("</tt>");
    // The code and its two backticks.
    Some(code.len().saturating_add(2))
}

/// `[text](https://...)`: the text is shown, the link is followed on a
/// tap. Only web and mail links: a `file:` or `javascript:` one is not
/// something a message gets to open.
fn link_span(rest: &str, out: &mut String) -> Option<usize> {
    let inner = rest.strip_prefix('[')?;
    let (text, after) = inner.split_once(']')?;
    let url_part = after.strip_prefix('(')?;
    let (url_typed, _) = url_part.split_once(')')?;
    let url = url_typed.trim();
    if text.is_empty() || url.contains(char::is_whitespace) {
        return None;
    }
    if !(url.starts_with("https://") || url.starts_with("http://") || url.starts_with("mailto:")) {
        return None;
    }
    push_link(out, url, text);
    // The text and the URL as typed, and the `[`, `](` and `)` around them.
    Some(text.len().saturating_add(url_typed.len()).saturating_add(4))
}

fn push_link(out: &mut String, url: &str, text: &str) {
    out.push_str("<a href=\"");
    push_escaped(out, url);
    out.push_str("\">");
    push_escaped(out, text);
    out.push_str("</a>");
}

/// `**bold**`, `__bold__`, `*italic*`, `_italic_`, `~~struck~~`. The
/// underscore forms only at word boundaries, so `snake_case_names` stay
/// as written; the others whenever the marker hugs its text.
fn emphasis_span(line: &str, at: usize, out: &mut String) -> Option<usize> {
    let rest = line.get(at..)?;
    let (marker, tag): (&str, &str) = if rest.starts_with("**") {
        ("**", "b")
    } else if rest.starts_with("__") {
        ("__", "b")
    } else if rest.starts_with("~~") {
        ("~~", "s")
    } else if rest.starts_with('*') {
        ("*", "i")
    } else if rest.starts_with('_') {
        ("_", "i")
    } else {
        return None;
    };
    let underscore = marker.starts_with('_');
    if underscore && is_word(line.get(..at)?.chars().next_back()) {
        return None;
    }
    let inner = rest.strip_prefix(marker)?;
    // The text has to start right after the marker, and end right before
    // the closing one: `a * b * c` is arithmetic, not italics.
    if inner.starts_with(char::is_whitespace) || inner.is_empty() {
        return None;
    }
    let close = find_closer(inner, marker)?;
    let text = inner.get(..close)?;
    if text.ends_with(char::is_whitespace) {
        return None;
    }
    // Both markers and the text between them.
    let consumed = marker.len().saturating_mul(2).saturating_add(close);
    if underscore && is_word(rest.get(consumed..)?.chars().next()) {
        return None;
    }
    out.push('<');
    out.push_str(tag);
    out.push('>');
    inline(text, out);
    out.push_str("</");
    out.push_str(tag);
    out.push('>');
    Some(consumed)
}

/// Where `marker` closes `inner`, skipping a marker that would leave the
/// text empty. A one-character marker must not match the first half of
/// a doubled one, so `*a **b** c*` closes at the lone star.
fn find_closer(inner: &str, marker: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(offset) = inner.get(from..)?.find(marker) {
        let index = from.saturating_add(offset);
        if index == 0 {
            from = marker.len();
            continue;
        }
        let single = marker.len() == 1;
        let doubled = inner
            .get(index..)
            .is_some_and(|tail| tail.starts_with(&marker.repeat(2)));
        if single && doubled {
            from = index.saturating_add(2);
            continue;
        }
        return Some(index);
    }
    None
}

/// Whether a character is part of a word, so an underscore beside it is
/// part of the word too.
fn is_word(c: Option<char>) -> bool {
    c.is_some_and(char::is_alphanumeric)
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn emphasis_code_and_headings_become_tags() {
        assert_eq!(
            render("**bold** and *it* and ~~no~~"),
            "<b>bold</b> and <i>it</i> and <s>no</s>"
        );
        assert_eq!(render("__bold__ _it_"), "<b>bold</b> <i>it</i>");
        assert_eq!(render("say `a < b` here"), "say <tt>a &lt; b</tt> here");
        // A line break is a `<br>`, not a newline: see `join_styled`.
        assert_eq!(render("# Title\nbody"), "<b>Title</b><br>body");
        // And not at the edges of a code block, whose own tags break the
        // line.
        assert_eq!(render("```\nx = 1\n```\nafter"), "<pre>x = 1</pre>after");
        assert_eq!(
            render("before\n```rust\nlet x;\n```"),
            "before<pre>let x;</pre>"
        );
        assert_eq!(render("```\n```\nx"), "x");
        assert_eq!(render("```\nopen"), "<pre>open</pre>");
        // Nested: the inner markers still count.
        assert_eq!(render("**bold *and* more**"), "<b>bold <i>and</i> more</b>");
    }

    #[test]
    fn every_line_of_a_message_is_drawn_on_a_line_of_its_own() {
        // The bug this is for: a newline is whitespace in
        // `Text.StyledText`, so a body joined with newlines was drawn as
        // one running paragraph -- a to-do list arrived as a sentence.
        assert_eq!(render("one\ntwo\nthree"), "one<br>two<br>three");
        // A blank line is a blank line: two breaks, not a collapsed one.
        assert_eq!(render("one\n\nthree"), "one<br><br>three");
        // Inside a code block the newlines stay, because `<pre>` keeps
        // them; around it there is no `<br>`, because its tags break the
        // line themselves.
        assert_eq!(render("a\n```\nx\ny\n```\nb"), "a<pre>x\ny</pre>b");
        // Trailing and leading breaks are breaks too.
        assert_eq!(render("\nx\n"), "<br>x<br>");
    }

    #[test]
    fn what_is_not_markdown_is_left_alone() {
        assert_eq!(render("2 * 3 * 4"), "2 * 3 * 4");
        assert_eq!(render("snake_case_name"), "snake_case_name");
        assert_eq!(render("a ** b"), "a ** b");
        assert_eq!(render("*unclosed"), "*unclosed");
        assert_eq!(render("**"), "**");
        assert_eq!(render(r"\*not\*"), "*not*");
        assert_eq!(render(""), "");
    }

    #[test]
    fn links_are_anchors_and_only_web_and_mail_ones() {
        assert_eq!(
            render("see https://example.org/a_b?x=1, ok"),
            "see <a href=\"https://example.org/a_b?x=1\">https://example.org/a_b?x=1</a>, ok"
        );
        assert_eq!(
            render("[the page](https://example.org/)"),
            "<a href=\"https://example.org/\">the page</a>"
        );
        assert_eq!(
            render("[mail](mailto:a@example.org)"),
            "<a href=\"mailto:a@example.org\">mail</a>"
        );
        // Not followed: anything that is not a web or a mail link.
        assert_eq!(
            render("[x](javascript:alert(1))"),
            "[x](javascript:alert(1))"
        );
        assert_eq!(render("[x](file:///etc/passwd)"), "[x](file:///etc/passwd)");
    }

    #[test]
    fn markup_in_a_message_stays_text() {
        // The reason every label in the app is plain text: a tracking
        // pixel in a body must not fetch anything when its row is drawn.
        let rendered = render("<img src=\"https://tracker/p.gif\"> & <a href=\"x\">y</a>");
        assert!(!rendered.contains("<img"), "{rendered}");
        assert!(!rendered.contains("<a href=\"x\""), "{rendered}");
        // The address inside is still a link -- followed on a tap, and
        // fetched by nothing else.
        assert_eq!(
            rendered,
            "&lt;img src=&quot;<a href=\"https://tracker/p.gif\">https://tracker/p.gif</a>&quot;&gt; \
             &amp; &lt;a href=&quot;x&quot;&gt;y&lt;/a&gt;"
        );
        assert_eq!(render("<b>plain</b>"), "&lt;b&gt;plain&lt;/b&gt;");
        // The same inside code and inside a link's text.
        assert_eq!(render("`<b>`"), "<tt>&lt;b&gt;</tt>");
        assert_eq!(
            render("[<i>](https://example.org/?a=1&b=2)"),
            "<a href=\"https://example.org/?a=1&amp;b=2\">&lt;i&gt;</a>"
        );
    }
}
