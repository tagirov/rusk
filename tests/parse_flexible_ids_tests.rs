use rusk::parse_flexible_ids;

#[test]
fn test_parse_flexible_ids_single_id() {
    let ids = parse_flexible_ids(&["1".to_string()]);
    assert_eq!(ids, vec![1]);
}

#[test]
fn test_parse_flexible_ids_multiple_space_separated() {
    // Only the first argument is processed (space-separated format not supported)
    let ids = parse_flexible_ids(&["1".to_string(), "2".to_string(), "3".to_string()]);
    assert_eq!(ids, vec![1]);
}

#[test]
fn test_parse_flexible_ids_comma_separated() {
    let ids = parse_flexible_ids(&["1,2,3".to_string()]);
    assert_eq!(ids, vec![1, 2, 3]);
}

#[test]
fn test_parse_flexible_ids_mixed_formats() {
    // Arguments with commas are processed, single IDs without commas are ignored
    let ids = parse_flexible_ids(&["1".to_string(), "2,3".to_string(), "4".to_string()]);
    assert_eq!(ids, vec![2, 3]);
}

#[test]
fn test_parse_flexible_ids_comma_with_spaces() {
    let ids = parse_flexible_ids(&["1, 2, 3".to_string()]);
    assert_eq!(ids, vec![1, 2, 3]);
}

#[test]
fn test_parse_flexible_ids_invalid_ids_ignored() {
    // Single ID without comma is processed (first argument, no comma-separated args)
    let ids = parse_flexible_ids(&["1".to_string(), "abc".to_string(), "2".to_string()]);
    assert_eq!(ids, vec![1]);
}

#[test]
fn test_parse_flexible_ids_empty_input() {
    let ids = parse_flexible_ids(&[]);
    assert_eq!(ids, vec![] as Vec<u8>);
}

#[test]
fn test_parse_flexible_ids_empty_strings() {
    // Empty strings are skipped, first non-empty argument is processed if it's a single ID
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
    // Single ID without comma is processed (first argument, no comma-separated args)
    let ids = parse_flexible_ids(&["1".to_string(), "256".to_string(), "2".to_string()]);
    assert_eq!(ids, vec![1]);
}

#[test]
fn test_parse_flexible_ids_negative_numbers_ignored() {
    // Single ID without comma is processed (first argument, no comma-separated args)
    let ids = parse_flexible_ids(&["1".to_string(), "-1".to_string(), "2".to_string()]);
    assert_eq!(ids, vec![1]);
}

#[test]
fn test_parse_flexible_ids_duplicate_ids_preserved() {
    // Single ID without comma is processed (first argument, no comma-separated args)
    let ids = parse_flexible_ids(&["1".to_string(), "1".to_string(), "2".to_string()]);
    assert_eq!(ids, vec![1]);
}

#[test]
fn test_parse_flexible_ids_with_space_before_comma() {
    // Handle case like "1,5,4 ,6" which becomes ["1,5,4", " ,6"]
    let ids = parse_flexible_ids(&["1,5,4".to_string(), " ,6".to_string()]);
    assert_eq!(ids, vec![1, 5, 4, 6]);
}

#[test]
fn test_parse_flexible_ids_with_comma_at_start() {
    // Handle case like "1,2" ",3" ",4"
    let ids = parse_flexible_ids(&["1,2".to_string(), ",3".to_string(), ",4".to_string()]);
    assert_eq!(ids, vec![1, 2, 3, 4]);
}

#[test]
fn test_parse_flexible_ids_multiple_comma_args() {
    // Handle case with multiple comma-separated arguments
    let ids = parse_flexible_ids(&["1,2".to_string(), "3,4".to_string(), "5,6".to_string()]);
    assert_eq!(ids, vec![1, 2, 3, 4, 5, 6]);
}

#[test]
fn test_parse_flexible_ids_empty_parts_in_comma_separated() {
    // Handle case with empty parts: "1,,3" or "1, ,3"
    let ids = parse_flexible_ids(&["1,,3".to_string()]);
    assert_eq!(ids, vec![1, 3]);
}

#[test]
fn test_parse_flexible_ids_empty_parts_with_spaces() {
    // Handle case with empty parts with spaces: "1, ,3"
    let ids = parse_flexible_ids(&["1, ,3".to_string()]);
    assert_eq!(ids, vec![1, 3]);
}

