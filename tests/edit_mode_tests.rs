use rusk::{BareEditDateFlag, parse_edit_args, strip_edit_date_flag};

#[test]
fn test_parse_edit_args_date_tokens_become_text() {
    // `rusk edit` no longer treats --date specially; these are text tokens (CLI rejects -d for edit).
    let (ids, text) = parse_edit_args(vec![
        "3".to_string(),
        "--date".to_string(),
        "2025-12-31".to_string(),
    ]);
    assert_eq!(ids, vec![3]);
    assert_eq!(
        text,
        Some(vec!["--date".to_string(), "2025-12-31".to_string()])
    );
}

#[test]
fn test_parse_edit_args_only_id_text_none() {
    let (ids, text) = parse_edit_args(vec!["3".to_string()]);
    assert_eq!(ids, vec![3]);
    assert!(text.is_none());
}

#[test]
fn test_parse_edit_args_dash_d_is_text_after_id() {
    let (ids, text) = parse_edit_args(vec!["3".to_string(), "-d".to_string()]);
    assert_eq!(ids, vec![3]);
    assert_eq!(text, Some(vec!["-d".to_string()]));
}

#[test]
fn test_parse_edit_args_mixed_ids_and_text_with_date_tokens() {
    let (ids, text) = parse_edit_args(vec![
        "1,2".to_string(),
        "new".to_string(),
        "text".to_string(),
        "--date".to_string(),
        "2025-06-15".to_string(),
    ]);
    assert_eq!(ids, vec![1, 2]);
    assert_eq!(
        text,
        Some(vec![
            "new".to_string(),
            "text".to_string(),
            "--date".to_string(),
            "2025-06-15".to_string(),
        ])
    );
}

#[test]
fn test_parse_edit_args_comma_separated_ids_and_text_including_dash_d() {
    let (ids, text) = parse_edit_args(vec![
        "1,2,3".to_string(),
        "some".to_string(),
        "words".to_string(),
        "-d".to_string(),
        "2025-01-01".to_string(),
    ]);
    assert_eq!(ids, vec![1, 2, 3]);
    assert_eq!(
        text,
        Some(vec![
            "some".to_string(),
            "words".to_string(),
            "-d".to_string(),
            "2025-01-01".to_string(),
        ])
    );
}

#[test]
fn test_parse_edit_args_comma_separated_ids_no_text() {
    let (ids, text) = parse_edit_args(vec!["1,2,3".to_string()]);
    assert_eq!(ids, vec![1, 2, 3]);
    assert!(text.is_none());
}

#[test]
fn test_parse_edit_args_text_only_words() {
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
fn test_parse_edit_args_dash_d_with_value_is_text() {
    let (ids, text) = parse_edit_args(vec!["7".to_string(), "-d".to_string(), "some".to_string()]);
    assert_eq!(ids, vec![7]);
    assert_eq!(text, Some(vec!["-d".to_string(), "some".to_string()]));
}

#[test]
fn test_parse_edit_args_with_space_before_comma() {
    let (ids, text) = parse_edit_args(vec!["1,5,4".to_string(), " ,6".to_string()]);
    assert_eq!(ids, vec![1, 5, 4, 6]);
    assert!(text.is_none());
}

#[test]
fn test_parse_edit_args_with_space_before_comma_and_text() {
    let (ids, text) = parse_edit_args(vec![
        "1,5,4".to_string(),
        " ,6".to_string(),
        "new".to_string(),
        "text".to_string(),
    ]);
    assert_eq!(ids, vec![1, 5, 4, 6]);
    assert_eq!(text, Some(vec!["new".to_string(), "text".to_string()]));
}

#[test]
fn test_parse_edit_args_with_comma_at_start() {
    let (ids, text) = parse_edit_args(vec!["1,2".to_string(), ",3".to_string(), ",4".to_string()]);
    assert_eq!(ids, vec![1, 2, 3, 4]);
    assert!(text.is_none());
}

#[test]
fn test_parse_edit_args_with_comma_at_start_and_text() {
    let (ids, text) = parse_edit_args(vec![
        "1,2".to_string(),
        ",3".to_string(),
        ",4".to_string(),
        "new".to_string(),
        "text".to_string(),
    ]);
    assert_eq!(ids, vec![1, 2, 3, 4]);
    assert_eq!(text, Some(vec!["new".to_string(), "text".to_string()]));
}

#[test]
fn test_parse_edit_args_with_space_before_comma_and_extra_tokens() {
    let (ids, text) = parse_edit_args(vec![
        "1,5,4".to_string(),
        " ,6".to_string(),
        "-d".to_string(),
        "2025-01-01".to_string(),
    ]);
    assert_eq!(ids, vec![1, 5, 4, 6]);
    assert_eq!(text, Some(vec!["-d".to_string(), "2025-01-01".to_string()]));
}

#[test]
fn test_parse_edit_args_with_space_before_comma_text_and_date_tokens() {
    let (ids, text) = parse_edit_args(vec![
        "1,5,4".to_string(),
        " ,6".to_string(),
        "new".to_string(),
        "text".to_string(),
        "--date".to_string(),
        "2025-01-01".to_string(),
    ]);
    assert_eq!(ids, vec![1, 5, 4, 6]);
    assert_eq!(
        text,
        Some(vec![
            "new".to_string(),
            "text".to_string(),
            "--date".to_string(),
            "2025-01-01".to_string(),
        ])
    );
}

#[test]
fn test_parse_edit_args_empty_parts_in_comma_separated() {
    let (ids, text) = parse_edit_args(vec!["1,,3".to_string()]);
    assert_eq!(ids, vec![1, 3]);
    assert!(text.is_none());
}

#[test]
fn test_parse_edit_args_multiple_comma_args() {
    let (ids, text) = parse_edit_args(vec!["1,2".to_string(), "3,4".to_string()]);
    assert_eq!(ids, vec![1, 2, 3, 4]);
    assert!(text.is_none());
}

#[test]
fn test_strip_edit_date_flag_removes_d_and_value() {
    let (out, d) = strip_edit_date_flag(vec![
        "1".to_string(),
        "a".to_string(),
        "b".to_string(),
        "-d".to_string(),
        "2w".to_string(),
    ])
    .unwrap();
    assert_eq!(d, Some("2w".to_string()));
    assert_eq!(out, vec!["1", "a", "b"]);
}

#[test]
fn test_strip_edit_date_flag_bare_returns_error() {
    assert_eq!(
        strip_edit_date_flag(vec!["1".to_string(), "-d".to_string()]),
        Err(BareEditDateFlag)
    );
}
