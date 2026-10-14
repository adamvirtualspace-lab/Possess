//! Converting between the editor's string offsets and Rust's.
//!
//! CodeMirror positions, and every JavaScript string index, count **UTF-16 code
//! units**. Rust strings are UTF-8 and index by bytes. The two agree for ASCII
//! and part company at the first emoji or CJK character — which is what makes
//! this worth isolating: code that conflates them passes every casual test and
//! then slices a note mid-character.
//!
//! A character outside the Basic Multilingual Plane is one `char` in Rust, up
//! to four bytes in UTF-8, and **two** code units in UTF-16. So neither
//! `.len()` nor `.chars().count()` is the number the editor means.

/// The text before a UTF-16 cursor position.
///
/// A cursor past the end of the line yields the whole line, which is what the
/// editor does. Landing inside a surrogate pair is not something CodeMirror
/// does, but the split is taken at the character boundary before it rather than
/// panicking.
pub fn truncate_at(line: &str, ch: usize) -> &str {
    if ch == 0 {
        return "";
    }

    let mut units = 0;
    for (byte, character) in line.char_indices() {
        if units >= ch {
            return &line[..byte];
        }
        units += character.len_utf16();
    }
    line
}

/// The UTF-16 offset of a byte offset in the same string.
pub fn offset_of(text: &str, byte: usize) -> usize {
    len(&text[..byte.min(text.len())])
}

/// How many UTF-16 code units this text occupies — what the editor would call
/// its length.
pub fn len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Two code units in UTF-16, four bytes in UTF-8, one char in Rust: the
    // case where all three measures disagree.
    const EMOJI: &str = "🙂";
    // Three bytes, one code unit, one char: bytes disagree, UTF-16 does not.
    const CJK: &str = "日";

    #[test]
    fn ascii_measures_the_same_every_way() {
        assert_eq!(len("hello"), 5);
        assert_eq!(truncate_at("hello", 3), "hel");
        assert_eq!(offset_of("hello", 3), 3);
    }

    #[test]
    fn an_astral_character_is_two_code_units() {
        assert_eq!(EMOJI.chars().count(), 1);
        assert_eq!(EMOJI.len(), 4);
        assert_eq!(len(EMOJI), 2);
    }

    #[test]
    fn truncating_counts_code_units_not_chars_or_bytes() {
        let line = format!("{EMOJI}ab");
        // The editor calls the position after the emoji 2, not 1 and not 4.
        assert_eq!(truncate_at(&line, 2), EMOJI);
        assert_eq!(truncate_at(&line, 3), &format!("{EMOJI}a"));
        assert_eq!(truncate_at(&line, 0), "");
    }

    #[test]
    fn cjk_is_one_code_unit_but_three_bytes() {
        let line = format!("{CJK}ab");
        assert_eq!(truncate_at(&line, 1), CJK);
        assert_eq!(truncate_at(&line, 2), &format!("{CJK}a"));
        assert_eq!(offset_of(&line, 3), 1);
    }

    #[test]
    fn byte_offsets_map_back_to_code_units() {
        let line = format!("a{EMOJI}b");
        // "b" starts at byte 5, which the editor calls 3.
        assert_eq!(line.find('b'), Some(5));
        assert_eq!(offset_of(&line, 5), 3);
    }

    #[test]
    fn a_cursor_past_the_end_takes_the_whole_line() {
        assert_eq!(truncate_at("ab", 99), "ab");
        assert_eq!(offset_of("ab", 99), 2);
    }

    #[test]
    fn round_trips_through_a_realistic_line() {
        // The case this module exists for: finding "[[" in a line that has
        // astral characters before it, and reporting where the editor thinks
        // it is.
        let line = format!("note {EMOJI} see [[Ide");
        let byte = line.find("[[").expect("brackets");
        let utf16 = offset_of(&line, byte);

        // Byte and code-unit offsets genuinely differ here.
        assert_ne!(byte, utf16);
        // And truncating at that offset lands exactly on the brackets.
        assert_eq!(truncate_at(&line, utf16), &line[..byte]);
    }
}
