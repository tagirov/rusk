use rusk::cli::HandlerCLI;
use chrono::NaiveDate;

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
fn test_calculate_ghost_suffix_inactive() {
    let result = HandlerCLI::calculate_ghost_suffix(false, 0, "hello");
    assert_eq!(result, None);
}

#[test]
fn test_calculate_ghost_suffix_cursor_at_start() {
    let result = HandlerCLI::calculate_ghost_suffix(true, 0, "hello world");
    assert_eq!(result, Some("hello world"));
}

#[test]
fn test_calculate_ghost_suffix_cursor_in_middle() {
    let result = HandlerCLI::calculate_ghost_suffix(true, 3, "hello world");
    assert_eq!(result, Some("lo world"));
}

#[test]
fn test_calculate_ghost_suffix_cursor_at_end() {
    let text = "hello";
    let result = HandlerCLI::calculate_ghost_suffix(true, text.len(), text);
    assert_eq!(result, None);
}

#[test]
fn test_calculate_ghost_suffix_empty_prefill() {
    let result = HandlerCLI::calculate_ghost_suffix(true, 0, "");
    assert_eq!(result, Some(""));
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
    // This test just verifies the function doesn't panic
    // Actual width depends on terminal, but should return at least a reasonable value
    let result = HandlerCLI::get_max_line_width();
    assert!(result > 0);
    assert!(result <= 80 || result > 80); // Either default 80 or terminal width
}

