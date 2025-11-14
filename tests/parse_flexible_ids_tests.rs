use rusk::parse_flexible_ids;

#[test]
fn test_parse_flexible_ids_single_id() {
    let ids = parse_flexible_ids(&["1".to_string()]);
    assert_eq!(ids, vec![1]);
}

#[test]
fn test_parse_flexible_ids_multiple_space_separated() {
    let ids = parse_flexible_ids(&["1".to_string(), "2".to_string(), "3".to_string()]);
    assert_eq!(ids, vec![1, 2, 3]);
}

#[test]
fn test_parse_flexible_ids_comma_separated() {
    let ids = parse_flexible_ids(&["1,2,3".to_string()]);
    assert_eq!(ids, vec![1, 2, 3]);
}

#[test]
fn test_parse_flexible_ids_mixed_formats() {
    let ids = parse_flexible_ids(&["1".to_string(), "2,3".to_string(), "4".to_string()]);
    assert_eq!(ids, vec![1, 2, 3, 4]);
}

#[test]
fn test_parse_flexible_ids_comma_with_spaces() {
    let ids = parse_flexible_ids(&["1, 2, 3".to_string()]);
    assert_eq!(ids, vec![1, 2, 3]);
}

#[test]
fn test_parse_flexible_ids_invalid_ids_ignored() {
    let ids = parse_flexible_ids(&["1".to_string(), "abc".to_string(), "2".to_string()]);
    assert_eq!(ids, vec![1, 2]);
}

#[test]
fn test_parse_flexible_ids_empty_input() {
    let ids = parse_flexible_ids(&[]);
    assert_eq!(ids, vec![] as Vec<u8>);
}

#[test]
fn test_parse_flexible_ids_empty_strings() {
    let ids = parse_flexible_ids(&["".to_string(), "1".to_string(), "".to_string()]);
    assert_eq!(ids, vec![1]);
}

#[test]
fn test_parse_flexible_ids_max_u8() {
    let ids = parse_flexible_ids(&["255".to_string()]);
    assert_eq!(ids, vec![255]);
}

#[test]
fn test_parse_flexible_ids_comma_separated_with_invalid() {
    let ids = parse_flexible_ids(&["1,abc,2,xyz,3".to_string()]);
    assert_eq!(ids, vec![1, 2, 3]);
}

#[test]
fn test_parse_flexible_ids_large_numbers_ignored() {
    // Numbers > 255 should fail to parse as u8
    let ids = parse_flexible_ids(&["1".to_string(), "256".to_string(), "2".to_string()]);
    assert_eq!(ids, vec![1, 2]);
}

#[test]
fn test_parse_flexible_ids_negative_numbers_ignored() {
    let ids = parse_flexible_ids(&["1".to_string(), "-1".to_string(), "2".to_string()]);
    assert_eq!(ids, vec![1, 2]);
}

#[test]
fn test_parse_flexible_ids_duplicate_ids_preserved() {
    let ids = parse_flexible_ids(&["1".to_string(), "1".to_string(), "2".to_string()]);
    assert_eq!(ids, vec![1, 1, 2]);
}

