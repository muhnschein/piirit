//! Markdown in messages, rendered to `Text.StyledText`.
//!
//! Sailfish Qt 5.6 has no Markdown (`Text` got it in 5.14); other clients send it. Typed subset:
//! emphasis, strikethrough, code, headings, links; no block model.
//!
//! Safe: every message char escaped; only tags are ours. `<img src="https://tracker/p.gif">`
//! stays text (hence plain labels, `tests/qml_syntax.rs`). Links reach network only on tap.
//! Rendered or off; no marker-stripping mode.

use crate::links::url_spans;

/// Message as `Text.StyledText`: escaped, Markdown to Qt tags. Line by line (fences, headings
/// are lines).
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

/// Fence marker: three backticks or tildes, optional language.
fn is_fence(trimmed: &str) -> bool {
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

/// Code-block state between lines.
#[derive(Default)]
struct Fence {
    /// Inside block: escaped, never styled.
    inside: bool,
    /// Next drawn line opens a block.
    opening: bool,
}

impl Fence {
    /// Fence line (language word too): never shown, leaves no line; block opens or closes.
    /// Open tag goes on next line, close on line before. Nothing drawn since open: no block.
    fn cross(&mut self, lines: &mut [String]) {
        if !self.inside {
            self.opening = true;
        } else if self.opening {
            self.opening = false;
        } else {
            close_block(lines);
        }
        self.inside = !self.inside;
    }

    /// Text ended: unclosed block still closes.
    fn finish(&self, lines: &mut [String]) {
        if self.inside && !self.opening {
            close_block(lines);
        }
    }
}

/// End block on last line drawn.
fn close_block(lines: &mut [String]) {
    if let Some(last) = lines.last_mut() {
        last.push_str("</pre>");
    }
}

/// Lines joined with `<br>` (newline is whitespace in `Text.StyledText`), none at `<pre>`: Qt
/// breaks there, `<br>` adds blank line. Checked by `qml_message_lines.rs`.
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
            // Tag is the break.
        } else if in_pre {
            // In block: newline kept.
            out.push('\n');
        } else {
            out.push_str("<br>");
        }
    }
    out
}

/// One line outside code: heading or text.
fn block(line: &str, trimmed: &str, out: &mut String) {
    // Heading: 1-6 hashes, space, text.
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

/// Chars `StyledText` reads as markup, escaped.
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

/// One line of text: emphasis, code, links, rest escaped.
fn inline(line: &str, out: &mut String) {
    let mut links = url_spans(line).into_iter().peekable();
    let mut at = 0;
    let mut searches = Searches::default();
    while let Some(rest) = line.get(at..).filter(|rest| !rest.is_empty()) {
        // Links in order; `at` only grows.
        while links.next_if(|(start, _)| *start < at).is_some() {}
        // Link chars never markup: `_` in path stays.
        if let Some((_, end)) = links.next_if(|(start, _)| *start == at) {
            if let Some(url) = line.get(at..end) {
                push_link(out, url, url);
                at = end;
                continue;
            }
        }
        // Backslash escapes next marker.
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
            .or_else(|| link_span(line, at, &mut searches, out))
            .or_else(|| emphasis_span(line, at, &mut searches, out))
        {
            at = at.saturating_add(consumed);
            continue;
        }
        let Some(c) = rest.chars().next() else {
            break;
        };
        push_escaped(out, c.encode_utf8(&mut [0; 4]));
        at = at.saturating_add(c.len_utf8());
    }
}

/// `` `code` ``: shown as typed, markers and all.
fn code_span(rest: &str, out: &mut String) -> Option<usize> {
    let inner = rest.strip_prefix('`')?;
    let (code, _) = inner.split_once('`').filter(|(code, _)| !code.is_empty())?;
    out.push_str("<tt>");
    push_escaped(out, code);
    out.push_str("</tt>");
    Some(code.len().saturating_add(2))
}

/// `[text](https://...)`: text shown, link on tap. Web and mail only; no `file:`, `javascript:`.
fn link_span(line: &str, at: usize, searches: &mut Searches, out: &mut String) -> Option<usize> {
    if !line.get(at..)?.starts_with('[') {
        return None;
    }
    let bytes = line.as_bytes();
    let text_start = at.saturating_add(1);
    let close = searches
        .bracket
        .find(text_start, |from| next_byte(bytes, from, b']'))?;
    if bytes.get(close.saturating_add(1)) != Some(&b'(') {
        return None;
    }
    let url_start = close.saturating_add(2);
    let url_end = searches
        .paren
        .find(url_start, |from| next_byte(bytes, from, b')'))?;
    let text = line.get(text_start..close)?;
    let url_typed = line.get(url_start..url_end)?;
    if text.is_empty() {
        return None;
    }
    // Skip leading spaces; none left: no URL.
    let url_from = searches.url.find(url_start, |from| {
        next_char(line, from, |c| !c.is_whitespace())
    })?;
    let url = line.get(url_from..url_end)?;
    if !(url.starts_with("https://") || url.starts_with("http://") || url.starts_with("mailto:")) {
        return None;
    }
    // Word after space inside URL: no link. Remembered: rereading per opener is quadratic on
    // repeated `[x](https://`.
    let space = searches
        .space
        .find(url_from, |from| next_char(line, from, char::is_whitespace))
        .filter(|space| *space < url_end);
    if let Some(space) = space {
        let word = searches
            .after_space
            .find(space, |from| next_char(line, from, |c| !c.is_whitespace()));
        if word.is_some_and(|word| word < url_end) {
            return None;
        }
    }
    push_link(out, url.trim_end(), text);
    Some(text.len().saturating_add(url_typed.len()).saturating_add(4))
}

fn push_link(out: &mut String, url: &str, text: &str) {
    out.push_str("<a href=\"");
    push_escaped(out, url);
    out.push_str("\">");
    push_escaped(out, text);
    out.push_str("</a>");
}

/// Emphasis markers and tags; doubled first, so `**` beats `*`.
const MARKERS: [(&str, &str); 5] = [
    ("**", "b"),
    ("__", "b"),
    ("~~", "s"),
    ("*", "i"),
    ("_", "i"),
];

/// `**bold**`, `__bold__`, `*italic*`, `_italic_`, `~~struck~~`. Underscores only at word edges:
/// `snake_case_names` stays.
fn emphasis_span(
    line: &str,
    at: usize,
    searches: &mut Searches,
    out: &mut String,
) -> Option<usize> {
    let rest = line.get(at..)?;
    let (kind, &(marker, tag)) = MARKERS
        .iter()
        .enumerate()
        .find(|(_, (marker, _))| rest.starts_with(marker))?;
    let underscore = marker.starts_with('_');
    if underscore && is_word(line.get(..at)?.chars().next_back()) {
        return None;
    }
    let inner = rest.strip_prefix(marker)?;
    // Text hugs markers: `a * b * c` is arithmetic.
    if inner.starts_with(char::is_whitespace) || inner.is_empty() {
        return None;
    }
    let start = at.saturating_add(marker.len());
    let close =
        searches
            .closers
            .get_mut(kind)?
            .closer(line.as_bytes(), start, marker.as_bytes())?;
    let text = line.get(start..close)?;
    if text.ends_with(char::is_whitespace) {
        return None;
    }
    let end = close.saturating_add(marker.len());
    if underscore && is_word(line.get(end..)?.chars().next()) {
        return None;
    }
    out.push('<');
    out.push_str(tag);
    out.push('>');
    inline(text, out);
    out.push_str("</");
    out.push_str(tag);
    out.push('>');
    Some(end.saturating_sub(at))
}

/// Where `marker` closes text at `start`, skipping one leaving text empty. Single never matches
/// half a doubled one: `*a **b** c*` closes at lone star.
fn find_closer(line: &[u8], start: usize, marker: &[u8]) -> Option<usize> {
    let single = marker.len() == 1;
    let mut from = start;
    loop {
        let offset = line
            .get(from..)?
            .windows(marker.len())
            .position(|window| window == marker)?;
        let index = from.saturating_add(offset);
        if index == start {
            from = start.saturating_add(marker.len());
            continue;
        }
        if single && line.get(index.saturating_add(1)) == marker.first() {
            from = index.saturating_add(2);
            continue;
        }
        return Some(index);
    }
}

/// Word char: underscore beside it joins the word.
fn is_word(c: Option<char>) -> bool {
    c.is_some_and(char::is_alphanumeric)
}

/// Next `byte` in `line` from `from`.
fn next_byte(line: &[u8], from: usize, byte: u8) -> Option<usize> {
    let offset = line.get(from..)?.iter().position(|b| *b == byte)?;
    Some(from.saturating_add(offset))
}

/// Next char matching `wanted` from `from`.
fn next_char(line: &str, from: usize, wanted: impl Fn(char) -> bool) -> Option<usize> {
    let (offset, _) = line.get(from..)?.char_indices().find(|(_, c)| wanted(*c))?;
    Some(from.saturating_add(offset))
}

/// Searches along one line, remembered: repeat openers read once.
#[derive(Default)]
struct Searches {
    /// Closer for each emphasis kind, in `MARKERS` order.
    closers: [Seek; 5],
    /// Next `]`: end of link text.
    bracket: Seek,
    /// Next `)`: end of URL.
    paren: Seek,
    /// Next non-space: URL start.
    url: Seek,
    /// Next space from there.
    space: Seek,
    /// Next non-space after that space.
    after_space: Seek,
}

/// One search with last answer: also next match from anywhere up to it (past here, if none).
/// Walk only forward: re-asking reads only past last answer.
#[derive(Default)]
struct Seek {
    /// Where last search started, and its result.
    last: Option<(usize, Option<usize>)>,
}

impl Seek {
    /// First place from `from` that `search` finds; reuses a held answer.
    fn find(&mut self, from: usize, search: impl FnOnce(usize) -> Option<usize>) -> Option<usize> {
        if let Some((start, found)) = self.last {
            if start <= from && !found.is_some_and(|found| found < from) {
                return found;
            }
        }
        self.search(from, search)
    }

    /// First place from `from` that `search` finds.
    fn search(
        &mut self,
        from: usize,
        search: impl FnOnce(usize) -> Option<usize>,
    ) -> Option<usize> {
        let found = search(from);
        self.last = Some((from, found));
        found
    }

    /// `find_closer`, remembered. Not plain next-of (skips marker at `start`, steps doubled in
    /// pairs), but from non-marker char matches earlier search; from marker: read afresh.
    fn closer(&mut self, line: &[u8], start: usize, marker: &[u8]) -> Option<usize> {
        let search = |from| find_closer(line, from, marker);
        if line.get(start) == marker.first() {
            self.search(start, search)
        } else {
            self.find(start, search)
        }
    }
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
        // `<br>`, not newline: see `join_styled`.
        assert_eq!(render("# Title\nbody"), "<b>Title</b><br>body");
        // Not at code block edges: its tags break the line.
        assert_eq!(render("```\nx = 1\n```\nafter"), "<pre>x = 1</pre>after");
        assert_eq!(
            render("before\n```rust\nlet x;\n```"),
            "before<pre>let x;</pre>"
        );
        assert_eq!(render("```\n```\nx"), "x");
        assert_eq!(render("```\nopen"), "<pre>open</pre>");
        assert_eq!(render("**bold *and* more**"), "<b>bold <i>and</i> more</b>");
    }

    #[test]
    fn every_line_of_a_message_is_drawn_on_a_line_of_its_own() {
        // Bug: to-do lines ran together.
        assert_eq!(render("one\ntwo\nthree"), "one<br>two<br>three");
        assert_eq!(render("one\n\nthree"), "one<br><br>three");
        // Inside code, newlines stay; `<pre>` tags break outside.
        assert_eq!(render("a\n```\nx\ny\n```\nb"), "a<pre>x\ny</pre>b");
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
    fn openers_that_never_close_cost_one_read_of_the_line() {
        // Unclosed openers each read to line end: slow.
        let underscores = "a__".repeat(7000);
        assert_eq!(render(&underscores), underscores);
        let brackets = "[".repeat(20_000);
        assert_eq!(render(&brackets), brackets);
        let spaced = format!("{}a]({})", "[".repeat(5000), " ".repeat(5000));
        assert_eq!(render(&spaced), spaced);
    }

    #[test]
    fn a_closer_found_for_one_opener_is_the_right_one_for_the_next() {
        // Remembered searches: where an old answer would go wrong.
        assert_eq!(render("a__b _c_ d_"), "a__b <i>c</i> d_");
        assert_eq!(render("_a_b _c_"), "_a_b <i>c</i>");
        assert_eq!(render("a __b__ __c d__"), "a <b>b</b> <b>c d</b>");
        assert_eq!(render("*a **b* c*"), "<i>a **b</i> c*");
        assert_eq!(
            render("[a [b](https://x.org/) c]"),
            "<a href=\"https://x.org/\">a [b</a> c]"
        );
        assert_eq!(
            render("[a](mailto:x y) [b]( mailto:z ) [c](http://q)"),
            "[a](mailto:x y) <a href=\"mailto:z\">b</a> <a href=\"http://q\">c</a>"
        );
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
        assert_eq!(
            render("[x](javascript:alert(1))"),
            "[x](javascript:alert(1))"
        );
        assert_eq!(render("[x](file:///etc/passwd)"), "[x](file:///etc/passwd)");
    }

    #[test]
    fn markup_in_a_message_stays_text() {
        // Labels plain: tracking pixels must not fetch on draw.
        let rendered = render("<img src=\"https://tracker/p.gif\"> & <a href=\"x\">y</a>");
        assert!(!rendered.contains("<img"), "{rendered}");
        assert!(!rendered.contains("<a href=\"x\""), "{rendered}");
        // Address still a link: followed on tap only.
        assert_eq!(
            rendered,
            "&lt;img src=&quot;<a href=\"https://tracker/p.gif\">https://tracker/p.gif</a>&quot;&gt; \
             &amp; &lt;a href=&quot;x&quot;&gt;y&lt;/a&gt;"
        );
        assert_eq!(render("<b>plain</b>"), "&lt;b&gt;plain&lt;/b&gt;");
        assert_eq!(render("`<b>`"), "<tt>&lt;b&gt;</tt>");
        assert_eq!(
            render("[<i>](https://example.org/?a=1&b=2)"),
            "<a href=\"https://example.org/?a=1&amp;b=2\">&lt;i&gt;</a>"
        );
    }

    #[test]
    fn many_links_on_one_line_are_each_found_once() {
        // Used to search every link per char: seconds.
        let line = "https://a.example/ ".repeat(20_000);
        let rendered = render(&line);
        assert_eq!(rendered.matches("<a href=").count(), 20_000);
        // Link in code span stays code; next still a link.
        assert_eq!(
            render("`https://a.example/` https://b.example/"),
            "<tt>https://a.example/</tt> \
             <a href=\"https://b.example/\">https://b.example/</a>"
        );
    }
}
