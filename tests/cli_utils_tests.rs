use chrono::NaiveDate;
use rusk::cli::HandlerCLI;

/// `colored::control::set_override` is process-global: tests that toggle it
/// must not run concurrently with each other.
static COLOR_OVERRIDE_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn test_wrap_text_by_words_empty() {
    let result = HandlerCLI::wrap_text_by_words("", 10);
    assert_eq!(result, vec![""]);
}

#[test]
fn test_wrap_text_by_words_single_word() {
    let result = HandlerCLI::wrap_text_by_words("hello", 10);
    assert_eq!(result, vec!["hello"]);
}

#[test]
fn test_wrap_text_by_words_multiple_words() {
    let result = HandlerCLI::wrap_text_by_words("hello world test", 10);
    assert_eq!(result, vec!["hello", "world test"]);
}

#[test]
fn test_wrap_text_by_words_long_word() {
    // Word longer than width should be split character by character
    let result = HandlerCLI::wrap_text_by_words("supercalifragilisticexpialidocious", 10);
    assert_eq!(result.len(), 4);
    assert_eq!(result[0], "supercalif");
    assert_eq!(result[1], "ragilistic");
    assert_eq!(result[2], "expialidoc");
    assert_eq!(result[3], "ious");
}

#[test]
fn test_wrap_text_by_words_exact_width() {
    let result = HandlerCLI::wrap_text_by_words("hello world", 5);
    assert_eq!(result, vec!["hello", "world"]);
}

#[test]
fn test_wrap_text_by_words_zero_width_does_not_hang() {
    // Regression: a zero width used to loop forever on words longer than the
    // width; now each chunk takes at least one character.
    let result = HandlerCLI::wrap_text_by_words("abc", 0);
    assert_eq!(result, vec!["a", "b", "c"]);
}

#[test]
fn test_wrap_text_by_words_whitespace() {
    let result = HandlerCLI::wrap_text_by_words("  hello   world  ", 10);
    assert_eq!(result, vec!["hello", "world"]);
}

#[test]
fn test_wrap_text_by_words_multiple_lines() {
    let text = "This is a very long sentence that should wrap across multiple lines";
    let result = HandlerCLI::wrap_text_by_words(text, 20);
    assert!(result.len() > 1);
    // All lines should be <= 20 characters (except possibly the last)
    for line in &result {
        assert!(line.chars().count() <= 20);
    }
}

#[test]
fn test_strip_ansi_codes_no_codes() {
    let result = HandlerCLI::strip_ansi_codes("hello world");
    assert_eq!(result, "hello world");
}

#[test]
fn test_strip_ansi_codes_single_code() {
    let result = HandlerCLI::strip_ansi_codes("\x1b[31mhello\x1b[0m");
    assert_eq!(result, "hello");
}

#[test]
fn test_strip_ansi_codes_multiple_codes() {
    let result = HandlerCLI::strip_ansi_codes("\x1b[32mhello\x1b[0m \x1b[33mworld\x1b[0m");
    assert_eq!(result, "hello world");
}

#[test]
fn test_strip_ansi_codes_nested_codes() {
    let result = HandlerCLI::strip_ansi_codes("\x1b[1m\x1b[31mhello\x1b[0m");
    assert_eq!(result, "hello");
}

#[test]
fn test_extract_ansi_codes_no_codes() {
    let (prefix, suffix) = HandlerCLI::extract_ansi_codes("hello world");
    assert_eq!(prefix, "");
    assert_eq!(suffix, "");
}

#[test]
fn test_extract_ansi_codes_single_code() {
    let (prefix, suffix) = HandlerCLI::extract_ansi_codes("\x1b[31mhello");
    assert_eq!(prefix, "\x1b[31m");
    assert_eq!(suffix, "\x1b[0m");
}

#[test]
fn test_extract_ansi_codes_multiple_codes() {
    let (prefix, suffix) = HandlerCLI::extract_ansi_codes("\x1b[1m\x1b[31mhello");
    assert!(prefix.contains("\x1b[1m"));
    assert!(prefix.contains("\x1b[31m"));
    assert_eq!(suffix, "\x1b[0m");
}

#[test]
fn test_extract_ansi_codes_with_text() {
    let (prefix, suffix) = HandlerCLI::extract_ansi_codes("\x1b[32mhello\x1b[0m world");
    assert_eq!(prefix, "\x1b[32m");
    assert_eq!(suffix, "\x1b[0m");
}

#[test]
fn test_extract_ansi_codes_unterminated_sequence_dropped() {
    // A sequence without the closing 'm' is not part of the prefix.
    let (prefix, suffix) = HandlerCLI::extract_ansi_codes("\x1b[31");
    assert_eq!(prefix, "");
    assert_eq!(suffix, "");

    // A complete sequence followed by an unterminated one keeps only the complete part.
    let (prefix, suffix) = HandlerCLI::extract_ansi_codes("\x1b[1m\x1b[3");
    assert_eq!(prefix, "\x1b[1m");
    assert_eq!(suffix, "\x1b[0m");
}

#[test]
fn test_prev_char_boundary_start() {
    let result = HandlerCLI::prev_char_boundary("hello", 0);
    assert_eq!(result, 0);
}

#[test]
fn test_prev_char_boundary_ascii() {
    let result = HandlerCLI::prev_char_boundary("hello", 3);
    assert_eq!(result, 2);
}

#[test]
fn test_prev_char_boundary_unicode() {
    // "привет" in Cyrillic
    let text = "привет";
    let result = HandlerCLI::prev_char_boundary(text, 4);
    // Each Cyrillic character is 2 bytes, so position 4 is at the start of the 3rd character
    assert!(result <= 4);
}

#[test]
fn test_prev_char_boundary_end() {
    let text = "hello";
    let result = HandlerCLI::prev_char_boundary(text, text.len());
    assert_eq!(result, text.len() - 1);
}

#[test]
fn test_next_char_boundary_start() {
    let result = HandlerCLI::next_char_boundary("hello", 0);
    assert_eq!(result, 1);
}

#[test]
fn test_next_char_boundary_ascii() {
    let result = HandlerCLI::next_char_boundary("hello", 2);
    assert_eq!(result, 3);
}

#[test]
fn test_next_char_boundary_unicode() {
    // "привет" in Cyrillic
    let text = "привет";
    let result = HandlerCLI::next_char_boundary(text, 0);
    // First Cyrillic character is 2 bytes
    assert_eq!(result, 2);
}

#[test]
fn test_next_char_boundary_end() {
    let text = "hello";
    let result = HandlerCLI::next_char_boundary(text, text.len());
    assert_eq!(result, text.len());
}

#[test]
fn test_byte_idx_to_char_count_start() {
    let result = HandlerCLI::byte_idx_to_char_count("hello", 0);
    assert_eq!(result, 0);
}

#[test]
fn test_byte_idx_to_char_count_ascii() {
    let result = HandlerCLI::byte_idx_to_char_count("hello", 3);
    assert_eq!(result, 3);
}

#[test]
fn test_byte_idx_to_char_count_unicode() {
    // "привет" - each character is 2 bytes
    let text = "привет";
    let result = HandlerCLI::byte_idx_to_char_count(text, 4);
    assert_eq!(result, 2); // 4 bytes = 2 characters
}

#[test]
fn test_byte_idx_to_char_count_end() {
    let text = "hello";
    let result = HandlerCLI::byte_idx_to_char_count(text, text.len());
    assert_eq!(result, text.len());
}

#[test]
fn test_is_word_char_alphanumeric() {
    assert!(HandlerCLI::is_word_char('a'));
    assert!(HandlerCLI::is_word_char('Z'));
    assert!(HandlerCLI::is_word_char('5'));
    assert!(HandlerCLI::is_word_char('_'));
    assert!(HandlerCLI::is_word_char('-'));
}

#[test]
fn test_is_word_char_non_word() {
    assert!(!HandlerCLI::is_word_char(' '));
    assert!(!HandlerCLI::is_word_char('.'));
    assert!(!HandlerCLI::is_word_char(','));
    assert!(!HandlerCLI::is_word_char('!'));
    assert!(!HandlerCLI::is_word_char('@'));
}

#[test]
fn test_jump_prev_word_start() {
    let result = HandlerCLI::jump_prev_word("hello world", 0);
    assert_eq!(result, 0);
}

#[test]
fn test_jump_prev_word_middle_of_word() {
    let result = HandlerCLI::jump_prev_word("hello world", 3);
    assert_eq!(result, 0); // Should jump to start of "hello"
}

#[test]
fn test_jump_prev_word_between_words() {
    let result = HandlerCLI::jump_prev_word("hello world", 6);
    assert_eq!(result, 0); // Should jump to start of "hello"
}

#[test]
fn test_jump_prev_word_second_word() {
    let result = HandlerCLI::jump_prev_word("hello world test", 12);
    assert_eq!(result, 6); // Should jump to start of "world"
}

#[test]
fn test_jump_prev_word_with_punctuation() {
    let result = HandlerCLI::jump_prev_word("hello, world!", 10);
    assert_eq!(result, 0); // Should jump to start of "hello"
}

#[test]
fn test_jump_next_word_start() {
    let result = HandlerCLI::jump_next_word("hello world", 0);
    assert_eq!(result, 6); // Should jump to start of "world" (after space at position 5)
}

#[test]
fn test_jump_next_word_middle_of_word() {
    let result = HandlerCLI::jump_next_word("hello world", 2);
    assert_eq!(result, 6); // Should jump to start of "world"
}

#[test]
fn test_jump_next_word_end() {
    let text = "hello world";
    let result = HandlerCLI::jump_next_word(text, text.len());
    assert_eq!(result, text.len());
}

#[test]
fn test_jump_next_word_with_punctuation() {
    let result = HandlerCLI::jump_next_word("hello, world!", 5);
    assert_eq!(result, 7); // Should jump to start of "world"
}

#[test]
fn test_format_date_for_display_none() {
    let result = HandlerCLI::format_date_for_display(None);
    assert_eq!(result, "empty");
}

#[test]
fn test_format_date_for_display_some() {
    let date = NaiveDate::parse_from_str("15-06-2025", "%d-%m-%Y").unwrap();
    let result = HandlerCLI::format_date_for_display(Some(date));
    assert_eq!(result, "15-06-2025");
}

#[test]
fn test_format_date_for_display_different_date() {
    let date = NaiveDate::parse_from_str("31-12-2024", "%d-%m-%Y").unwrap();
    let result = HandlerCLI::format_date_for_display(Some(date));
    assert_eq!(result, "31-12-2024");
}

#[test]
fn test_get_max_line_width() {
    let result = HandlerCLI::get_max_line_width();
    assert!(
        (1..=80).contains(&result),
        "max line width must be normalized to 1..=80, got {result}"
    );
}

#[test]
fn test_normalize_terminal_width_zero_returns_default() {
    assert_eq!(HandlerCLI::normalize_terminal_width(0), 80);
}

#[test]
fn test_normalize_terminal_width_small() {
    assert_eq!(HandlerCLI::normalize_terminal_width(40), 40);
}

#[test]
fn test_normalize_terminal_width_exact_default() {
    assert_eq!(HandlerCLI::normalize_terminal_width(80), 80);
}

#[test]
fn test_normalize_terminal_width_large() {
    assert_eq!(HandlerCLI::normalize_terminal_width(200), 80);
}

#[test]
fn test_normalize_terminal_width_one() {
    assert_eq!(HandlerCLI::normalize_terminal_width(1), 1);
}

#[test]
fn test_trim_trailing_punctuation_basic() {
    assert_eq!(HandlerCLI::trim_trailing_punctuation("hello."), "hello");
    assert_eq!(HandlerCLI::trim_trailing_punctuation("a!?"), "a");
    assert_eq!(HandlerCLI::trim_trailing_punctuation("x…"), "x");
}

#[test]
fn test_trim_trailing_punctuation_preserves_underscore() {
    assert_eq!(
        HandlerCLI::trim_trailing_punctuation("snake_case"),
        "snake_case"
    );
}

#[test]
fn test_trim_first_line_for_compact_list_spaces() {
    assert_eq!(
        HandlerCLI::trim_first_line_for_compact_list("hello. "),
        "hello"
    );
    assert_eq!(HandlerCLI::trim_first_line_for_compact_list("a . "), "a");
}

#[test]
fn test_trim_first_line_strips_comma_before_zero_width_space() {
    assert_eq!(
        HandlerCLI::trim_first_line_for_compact_list("hello,\u{200b}"),
        "hello"
    );
}

#[test]
fn test_compact_list_trims_comma_at_end_of_wrapped_first_row() {
    let wrapped = HandlerCLI::wrap_text_by_words("aa, bb", 5);
    let first = wrapped.first().expect("wrapped row");
    assert!(
        first.ends_with(','),
        "first visual row should end with comma before trim: {first:?}"
    );
    assert_eq!(HandlerCLI::trim_first_line_for_compact_list(first), "aa");
}

#[test]
fn test_trim_trailing_punctuation_fullwidth_comma() {
    assert_eq!(HandlerCLI::trim_trailing_punctuation("x\u{ff0c}"), "x");
}

// ── wrap_text_by_words with hard line breaks ────────────────────────────────

#[test]
fn test_wrap_text_by_words_preserves_newlines() {
    let result = HandlerCLI::wrap_text_by_words("line one\nline two", 40);
    assert_eq!(result, vec!["line one", "line two"]);
}

#[test]
fn test_wrap_text_by_words_newlines_and_wrapping() {
    let result = HandlerCLI::wrap_text_by_words("one two three four\nfive six", 10);
    assert_eq!(result, vec!["one two", "three four", "five six"]);
}

#[test]
fn test_wrap_text_by_words_blank_line_between_paragraphs() {
    let result = HandlerCLI::wrap_text_by_words("para one\n\npara two", 20);
    assert_eq!(result, vec!["para one", "", "para two"]);
}

#[test]
fn test_wrap_text_by_words_trailing_newline() {
    let result = HandlerCLI::wrap_text_by_words("hello\n", 20);
    assert_eq!(result, vec!["hello", ""]);
}

// ── Multi-line editor helpers ───────────────────────────────────────────────

#[test]
fn test_split_multi_line_prefill_empty() {
    let result = HandlerCLI::split_multi_line_prefill("");
    assert!(result.is_empty());
}

#[test]
fn test_split_multi_line_prefill_single_line() {
    let result = HandlerCLI::split_multi_line_prefill("hello");
    assert_eq!(result, vec!["hello".to_string()]);
}

#[test]
fn test_split_multi_line_prefill_lf() {
    let result = HandlerCLI::split_multi_line_prefill("a\nb\nc");
    assert_eq!(
        result,
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );
}

#[test]
fn test_split_multi_line_prefill_crlf_normalized() {
    let result = HandlerCLI::split_multi_line_prefill("a\r\nb\rc\n");
    assert_eq!(
        result,
        vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "".to_string(),
        ]
    );
}

#[test]
fn test_ml_char_to_byte_ascii() {
    assert_eq!(HandlerCLI::ml_char_to_byte("hello", 0), 0);
    assert_eq!(HandlerCLI::ml_char_to_byte("hello", 3), 3);
    assert_eq!(HandlerCLI::ml_char_to_byte("hello", 10), 5);
}

#[test]
fn test_ml_char_to_byte_unicode() {
    // "привет" — 2 bytes per char.
    assert_eq!(HandlerCLI::ml_char_to_byte("привет", 0), 0);
    assert_eq!(HandlerCLI::ml_char_to_byte("привет", 3), 6);
    assert_eq!(HandlerCLI::ml_char_to_byte("привет", 6), 12);
    assert_eq!(HandlerCLI::ml_char_to_byte("привет", 100), 12);
}

#[test]
fn test_ml_move_left_within_line() {
    let lines = vec!["hello".to_string(), "world".to_string()];
    assert_eq!(HandlerCLI::ml_move_left(&lines, 0, 3), (0, 2));
}

#[test]
fn test_ml_move_left_wraps_to_prev_line_end() {
    let lines = vec!["hi".to_string(), "there".to_string()];
    assert_eq!(HandlerCLI::ml_move_left(&lines, 1, 0), (0, 2));
}

#[test]
fn test_ml_move_left_at_buffer_start_is_noop() {
    let lines = vec!["x".to_string()];
    assert_eq!(HandlerCLI::ml_move_left(&lines, 0, 0), (0, 0));
}

#[test]
fn test_ml_move_right_within_line() {
    let lines = vec!["hello".to_string()];
    assert_eq!(HandlerCLI::ml_move_right(&lines, 0, 2), (0, 3));
}

#[test]
fn test_ml_move_right_wraps_to_next_line_start() {
    let lines = vec!["hi".to_string(), "there".to_string()];
    assert_eq!(HandlerCLI::ml_move_right(&lines, 0, 2), (1, 0));
}

#[test]
fn test_ml_move_right_at_buffer_end_is_noop() {
    let lines = vec!["ab".to_string()];
    assert_eq!(HandlerCLI::ml_move_right(&lines, 0, 2), (0, 2));
}

#[test]
fn test_ml_word_left_within_line() {
    // jump_prev_word skips the current word and stops at the start of the previous one.
    let lines = vec!["one two three".to_string()];
    assert_eq!(HandlerCLI::ml_word_left(&lines, 0, 10), (0, 4));
}

#[test]
fn test_ml_word_left_jumps_to_prev_line_end() {
    let lines = vec!["abc".to_string(), "def".to_string()];
    assert_eq!(HandlerCLI::ml_word_left(&lines, 1, 0), (0, 3));
}

#[test]
fn test_ml_word_right_within_line() {
    let lines = vec!["hello world".to_string()];
    assert_eq!(HandlerCLI::ml_word_right(&lines, 0, 0), (0, 6));
}

#[test]
fn test_ml_word_right_jumps_to_next_line_start() {
    let lines = vec!["abc".to_string(), "def".to_string()];
    assert_eq!(HandlerCLI::ml_word_right(&lines, 0, 3), (1, 0));
}

#[test]
fn test_ml_backspace_within_line() {
    let mut lines = vec!["hello".to_string()];
    let mut row = 0usize;
    let mut col = 3usize;
    HandlerCLI::ml_backspace(&mut lines, &mut row, &mut col);
    assert_eq!(lines, vec!["helo".to_string()]);
    assert_eq!((row, col), (0, 2));
}

#[test]
fn test_ml_backspace_joins_lines() {
    let mut lines = vec!["hi".to_string(), "there".to_string()];
    let mut row = 1usize;
    let mut col = 0usize;
    HandlerCLI::ml_backspace(&mut lines, &mut row, &mut col);
    assert_eq!(lines, vec!["hithere".to_string()]);
    assert_eq!((row, col), (0, 2));
}

#[test]
fn test_ml_backspace_at_start_is_noop() {
    let mut lines = vec!["hi".to_string()];
    let mut row = 0usize;
    let mut col = 0usize;
    HandlerCLI::ml_backspace(&mut lines, &mut row, &mut col);
    assert_eq!(lines, vec!["hi".to_string()]);
    assert_eq!((row, col), (0, 0));
}

#[test]
fn test_ml_delete_within_line() {
    let mut lines = vec!["hello".to_string()];
    let mut col = 2usize;
    HandlerCLI::ml_delete(&mut lines, 0, &mut col);
    assert_eq!(lines, vec!["helo".to_string()]);
    assert_eq!(col, 2);
}

#[test]
fn test_ml_delete_joins_with_next_line() {
    let mut lines = vec!["hi".to_string(), "there".to_string()];
    let mut col = 2usize;
    HandlerCLI::ml_delete(&mut lines, 0, &mut col);
    assert_eq!(lines, vec!["hithere".to_string()]);
    assert_eq!(col, 2);
}

#[test]
fn test_ml_delete_word_left_within_line() {
    // Using a 3-word buffer so jump_prev_word lands inside the second word,
    // leaving the first word and the trailing space untouched.
    let mut lines = vec!["one two three".to_string()];
    let mut row = 0usize;
    let mut col = 13usize;
    HandlerCLI::ml_delete_word_left(&mut lines, &mut row, &mut col);
    assert_eq!(lines, vec!["one ".to_string()]);
    assert_eq!((row, col), (0, 4));
}

#[test]
fn test_ml_delete_word_left_at_line_start_joins_with_prev() {
    let mut lines = vec!["abc".to_string(), "def".to_string()];
    let mut row = 1usize;
    let mut col = 0usize;
    HandlerCLI::ml_delete_word_left(&mut lines, &mut row, &mut col);
    assert_eq!(lines, vec!["abcdef".to_string()]);
    assert_eq!((row, col), (0, 3));
}

// --- `rusk search` helpers: case-insensitive find and highlight ---

fn needle(q: &str) -> Vec<char> {
    q.to_lowercase().chars().collect()
}

#[test]
fn test_find_ci_ascii_case_insensitive() {
    assert_eq!(
        HandlerCLI::find_ci("Buy Groceries", &needle("groc"), 0),
        Some((4, 8))
    );
}

#[test]
fn test_find_ci_cyrillic_case_insensitive() {
    let hay = "мини-таблица по Омега-3";
    let (start, end) = HandlerCLI::find_ci(hay, &needle("ОМЕГА"), 0).unwrap();
    assert_eq!(&hay[start..end], "Омега");
}

#[test]
fn test_find_ci_no_match() {
    assert_eq!(HandlerCLI::find_ci("hello", &needle("world"), 0), None);
}

#[test]
fn test_find_ci_empty_needle() {
    assert_eq!(HandlerCLI::find_ci("hello", &[], 0), None);
}

#[test]
fn test_find_ci_from_offset_finds_next_occurrence() {
    let hay = "abc abc";
    assert_eq!(HandlerCLI::find_ci(hay, &needle("abc"), 0), Some((0, 3)));
    assert_eq!(HandlerCLI::find_ci(hay, &needle("abc"), 3), Some((4, 7)));
}

#[test]
fn test_find_ci_phrase_with_space() {
    let hay = "ингридиентами для сравнения";
    let (start, end) = HandlerCLI::find_ci(hay, &needle("для сравнения"), 0).unwrap();
    assert_eq!(&hay[start..end], "для сравнения");
}

#[test]
fn test_highlight_matches_wraps_all_occurrences() {
    let _guard = COLOR_OVERRIDE_MUTEX.lock().unwrap();
    colored::control::set_override(true);
    let out = HandlerCLI::highlight_matches("foo bar Foo", &needle("foo"));
    colored::control::unset_override();
    // Both occurrences are colored; the original casing is preserved.
    assert_eq!(out.matches("\x1b[").count(), 4);
    assert!(out.contains("Foo"));
    assert!(HandlerCLI::strip_ansi_codes(&out) == "foo bar Foo");
}

#[test]
fn test_highlight_matches_no_match_returns_line_unchanged() {
    let out = HandlerCLI::highlight_matches("foo bar", &needle("baz"));
    assert_eq!(out, "foo bar");
}

#[test]
fn test_highlight_keywords_colors_the_leading_word_only() {
    let _guard = COLOR_OVERRIDE_MUTEX.lock().unwrap();
    colored::control::set_override(true);
    let out = HandlerCLI::highlight_keywords("TEMP buy milk INFO note");
    colored::control::unset_override();
    // Only the leading keyword is colored; a keyword later in the text is not.
    assert_eq!(out.matches("\x1b[").count(), 2);
    assert!(out.starts_with("\x1b["), "{out}");
    assert_eq!(HandlerCLI::strip_ansi_codes(&out), "TEMP buy milk INFO note");

    // All default keywords are recognized at the head of the text.
    for kw in ["TEMP", "INFO", "FIXME", "WIP"] {
        let text = format!("{kw} something");
        colored::control::set_override(true);
        let out = HandlerCLI::highlight_keywords(&text);
        colored::control::unset_override();
        assert_eq!(out.matches("\x1b[").count(), 2, "{kw}");
    }

    // Substrings, other casings and non-leading positions are not keywords.
    colored::control::set_override(true);
    let out = HandlerCLI::highlight_keywords("TEMPO temp buy TEMP");
    colored::control::unset_override();
    assert_eq!(out, "TEMPO temp buy TEMP");
}
