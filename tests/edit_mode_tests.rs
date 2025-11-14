use rusk::parse_edit_args;

#[test]
fn test_parse_edit_args_date_only_long_flag() {
    // rusk edit 3 --date 2025-12-31
    let (ids, text) = parse_edit_args(vec![
        "3".to_string(),
        "--date".to_string(),
        "2025-12-31".to_string(),
    ]);
    assert_eq!(ids, vec![3]);
    assert!(text.is_none());
}

#[test]
fn test_parse_edit_args_only_id_text_none() {
    // rusk edit 3  -> interactive text-only expected; text should be None
    let (ids, text) = parse_edit_args(vec!["3".to_string()]);
    assert_eq!(ids, vec![3]);
    assert!(text.is_none());
}

#[test]
fn test_parse_edit_args_interactive_short_flag() {
    // rusk edit 3 -d (interactive expected at CLI routing; parser should not treat -d as text)
    let (ids, text) = parse_edit_args(vec!["3".to_string(), "-d".to_string()]);
    assert_eq!(ids, vec![3]);
    assert!(text.is_none());
}

#[test]
fn test_parse_edit_args_mixed_ids_and_text_with_date() {
    // rusk edit 1 2 new text --date 2025-06-15
    let (ids, text) = parse_edit_args(vec![
        "1".to_string(),
        "2".to_string(),
        "new".to_string(),
        "text".to_string(),
        "--date".to_string(),
        "2025-06-15".to_string(),
    ]);
    assert_eq!(ids, vec![1, 2]);
    assert_eq!(text, Some(vec!["new".to_string(), "text".to_string()]));
}

#[test]
fn test_parse_edit_args_comma_separated_ids_and_text_with_short_date() {
    // rusk edit 1,2,3 some words -d 2025-01-01
    let (ids, text) = parse_edit_args(vec![
        "1,2,3".to_string(),
        "some".to_string(),
        "words".to_string(),
        "-d".to_string(),
        "2025-01-01".to_string(),
    ]);
    assert_eq!(ids, vec![1, 2, 3]);
    assert_eq!(text, Some(vec!["some".to_string(), "words".to_string()]));
}

#[test]
fn test_parse_edit_args_comma_separated_ids_no_text() {
    // rusk edit 1,2,3 -> text None
    let (ids, text) = parse_edit_args(vec!["1,2,3".to_string()]);
    assert_eq!(ids, vec![1, 2, 3]);
    assert!(text.is_none());
}

#[test]
fn test_parse_edit_args_text_only_words() {
    // rusk edit 5 new title here
    let (ids, text) = parse_edit_args(vec![
        "5".to_string(),
        "new".to_string(),
        "title".to_string(),
        "here".to_string(),
    ]);
    assert_eq!(ids, vec![5]);
    assert_eq!(
        text,
        Some(vec![
            "new".to_string(),
            "title".to_string(),
            "here".to_string(),
        ])
    );
}

#[test]
fn test_parse_edit_args_short_date_without_value_then_text_ignored_as_date_value() {
    // rusk edit 7 -d some -> parser treats 'some' as date value and skips it
    // so text remains None and ids parsed
    let (ids, text) = parse_edit_args(vec!["7".to_string(), "-d".to_string(), "some".to_string()]);
    assert_eq!(ids, vec![7]);
    assert!(text.is_none());
}
