//! Whether a message about to be sent will arrive whole.
//!
//! The core cuts a long body before it sends it: past a point, what goes
//! out is a shortened version plus an HTML part holding the rest, and the
//! reader at the other end sees a preview with something to tap. That is
//! a fine thing to happen and a surprising thing to discover, so the
//! conversation says it before the send rather than after.
//!
//! The rule is the core's `truncate_by_lines` (`tools.rs`): a body is cut
//! once it reaches 38 lines, where a line is a newline *or* 100
//! characters of one. The numbers are compiled into the pinned
//! `deltachat-rpc-server` and cannot be asked for at runtime, so they are
//! written here -- the same reading, and the same two constants, parla
//! keeps for the same warning.
//!
//! Being wrong here costs a notice that should not be there, or one that
//! should have been: nothing is sent differently on account of it.

/// How many lines the core lets a message have before it cuts it.
const MAX_LINES: usize = 38;
/// How many characters of one line count as a line.
const MAX_LINE_LEN: usize = 100;

/// Whether the core would cut this text and send the rest as an HTML
/// part.
pub(crate) fn would_be_cut(text: &str) -> bool {
    let mut lines = 0;
    let mut in_line = 0;
    for character in text.chars() {
        if character == '\n' {
            in_line = 0;
            lines += 1;
        } else {
            in_line += 1;
            if in_line > MAX_LINE_LEN {
                in_line = 1;
                lines += 1;
            }
        }
        if lines == MAX_LINES {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{would_be_cut, MAX_LINES, MAX_LINE_LEN};

    #[test]
    fn a_message_anyone_would_call_short_goes_whole() {
        assert!(!would_be_cut(""));
        assert!(!would_be_cut("Buy milk"));
        assert!(!would_be_cut(&"a line\n".repeat(10)));
    }

    #[test]
    fn the_cut_is_at_the_line_the_core_puts_it_at() {
        // 37 newlines is 37 lines and stays whole; 38 does not.
        assert!(!would_be_cut(&"x\n".repeat(MAX_LINES - 1)));
        assert!(would_be_cut(&"x\n".repeat(MAX_LINES)));
    }

    #[test]
    fn a_wrapped_line_counts_as_the_lines_it_wraps_into() {
        // One unbroken run of characters, wrapping every 100: enough of
        // them is a long message even though it has no newline in it.
        let short = "x".repeat(MAX_LINE_LEN * (MAX_LINES - 2));
        let long = "x".repeat(MAX_LINE_LEN * (MAX_LINES + 1));
        assert!(!would_be_cut(&short));
        assert!(would_be_cut(&long));
    }

    #[test]
    fn a_line_of_exactly_the_limit_has_not_wrapped_yet() {
        // 100 characters is one line; the 101st starts the second.
        assert!(!would_be_cut(&"x".repeat(MAX_LINE_LEN)));
    }

    #[test]
    fn counting_is_by_character_and_not_by_byte() {
        // 100 emoji are 100 characters and 400 bytes. Counting bytes
        // would call a short message long.
        assert!(!would_be_cut(&"🙂".repeat(MAX_LINE_LEN)));
    }

    #[test]
    fn a_to_do_list_of_forty_items_is_a_long_message() {
        let mut list = String::new();
        for number in 0..40 {
            use std::fmt::Write as _;
            let _ = writeln!(list, "- [ ] item {number}");
        }
        assert!(would_be_cut(&list));
    }
}
