// Task id lists on the command line (src/parser/ids.rs): one comma-separated
// word, glued back together when the shell split it at a comma, nothing
// dropped silently, every id once.

use rusk::{IdListError, parse_edit_args, parse_id_args, parse_id_list, split_leading_ids};

fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| w.to_string()).collect()
}

// --- parse_id_list: one word ------------------------------------------------

#[test]
fn list_single_id() {
    assert_eq!(parse_id_list("1"), Ok(vec![1]));
}

#[test]
fn list_comma_separated() {
    assert_eq!(parse_id_list("1,2,3"), Ok(vec![1, 2, 3]));
}

#[test]
fn list_tolerates_blanks_and_empty_parts() {
    assert_eq!(parse_id_list(" 1, 2 ,3 "), Ok(vec![1, 2, 3]));
    assert_eq!(parse_id_list("1,,3"), Ok(vec![1, 3]));
    assert_eq!(parse_id_list("1, ,3"), Ok(vec![1, 3]));
    assert_eq!(parse_id_list(",1,"), Ok(vec![1]));
}

#[test]
fn list_keeps_input_order_and_drops_repeats() {
    assert_eq!(parse_id_list("3,1,3,2,1"), Ok(vec![3, 1, 2]));
}

#[test]
fn list_with_a_word_that_is_not_an_id_is_an_error() {
    assert_eq!(
        parse_id_list("1,abc,2"),
        Err(IdListError::NotAnId {
            part: "abc".into(),
            list: "1,abc,2".into()
        })
    );
    assert_eq!(
        parse_id_list("abc"),
        Err(IdListError::NotAnId {
            part: "abc".into(),
            list: "abc".into()
        })
    );
}

#[test]
fn list_rejects_negative_range_and_overflow() {
    assert!(matches!(parse_id_list("-1"), Err(IdListError::NotAnId { .. })));
    assert!(matches!(parse_id_list("+1"), Err(IdListError::NotAnId { .. })));
    assert!(matches!(parse_id_list("1-3"), Err(IdListError::NotAnId { .. })));
    assert!(matches!(
        parse_id_list("1,4294967296"),
        Err(IdListError::NotAnId { part, .. }) if part == "4294967296"
    ));
    assert_eq!(parse_id_list("4294967295"), Ok(vec![u32::MAX]));
}

#[test]
fn list_takes_zero_as_a_number() {
    // No task has id 0, so a command reports it as not found; it is a
    // number all the same, not a word to drop.
    assert_eq!(parse_id_list("0"), Ok(vec![0]));
    assert_eq!(parse_id_list("0,1"), Ok(vec![0, 1]));
    assert_eq!(parse_id_list("007"), Ok(vec![7]));
}

#[test]
fn list_without_an_id_is_empty() {
    assert_eq!(parse_id_list(""), Err(IdListError::Empty { list: "".into() }));
    assert_eq!(parse_id_list(","), Err(IdListError::Empty { list: ",".into() }));
    assert_eq!(parse_id_list(" , "), Err(IdListError::Empty { list: ",".into() }));
}

#[test]
fn list_errors_read_well() {
    let msg = |r: Result<Vec<u32>, IdListError>| r.unwrap_err().to_string();
    assert_eq!(msg(parse_id_list("")), "no task ids given");
    assert_eq!(msg(parse_id_list(",")), "no task id in ','");
    assert_eq!(msg(parse_id_list("abc")), "'abc' is not a task id");
    assert_eq!(msg(parse_id_list("1,abc")), "'abc' in '1,abc' is not a task id");
    assert_eq!(
        msg(parse_id_args(&argv(&["1", "2"]))),
        "unexpected argument '2' after the ids '1' (task ids are one comma-separated list)"
    );
}

// --- split_leading_ids: gluing across a comma -------------------------------

#[test]
fn split_glues_words_at_a_comma() {
    // `1, 2, 3` as the shell hands it over.
    let words = argv(&["1,", "2,", "3"]);
    let (ids, list, rest) = split_leading_ids(&words).unwrap();
    assert_eq!(ids, vec![1, 2, 3]);
    assert_eq!(list, "1,2,3");
    assert!(rest.is_empty());

    // `1 ,2 ,3` and `1,5,4 ,6`.
    assert_eq!(split_leading_ids(&argv(&["1", ",2", ",3"])).unwrap().0, vec![1, 2, 3]);
    assert_eq!(split_leading_ids(&argv(&["1,5,4", " ,6"])).unwrap().0, vec![1, 5, 4, 6]);
    // A comma on both sides of the split.
    assert_eq!(split_leading_ids(&argv(&["1,", ",2"])).unwrap().0, vec![1, 2]);
}

#[test]
fn split_stops_at_a_word_without_a_comma_boundary() {
    let words = argv(&["3", "1,000", "units"]);
    let (ids, _, rest) = split_leading_ids(&words).unwrap();
    assert_eq!(ids, vec![3]);
    assert_eq!(rest, &words[1..]);

    let words = argv(&["1,2", "3,4", "text"]);
    let (ids, _, rest) = split_leading_ids(&words).unwrap();
    assert_eq!(ids, vec![1, 2]);
    assert_eq!(rest, &words[1..]);

    let words = argv(&["1", "2", "text"]);
    let (ids, _, rest) = split_leading_ids(&words).unwrap();
    assert_eq!(ids, vec![1]);
    assert_eq!(rest, &words[1..]);
}

#[test]
fn split_glues_only_what_looks_like_ids() {
    // A trailing comma before a word: the word is not part of the list.
    let words = argv(&["1,", "foo", "bar"]);
    let (ids, _, rest) = split_leading_ids(&words).unwrap();
    assert_eq!(ids, vec![1]);
    assert_eq!(rest, &words[1..]);

    // A leading comma on a word glues it, and then it must be an id.
    assert!(matches!(
        split_leading_ids(&argv(&["1", ",foo"])),
        Err(IdListError::NotAnId { part, list }) if part == "foo" && list == "1,foo"
    ));
    assert!(matches!(
        split_leading_ids(&argv(&["1,", "2,abc"])),
        Err(IdListError::NotAnId { part, .. }) if part == "abc"
    ));
}

#[test]
fn split_first_word_must_be_a_list() {
    assert!(matches!(
        split_leading_ids(&argv(&["abc", "1"])),
        Err(IdListError::NotAnId { part, .. }) if part == "abc"
    ));
    assert!(matches!(
        split_leading_ids(&argv(&["1 2"])),
        Err(IdListError::NotAnId { part, .. }) if part == "1 2"
    ));
    assert_eq!(
        split_leading_ids(&[]),
        Err(IdListError::Empty { list: "".into() })
    );
    assert_eq!(
        split_leading_ids(&argv(&[""])),
        Err(IdListError::Empty { list: "".into() })
    );
}

// --- parse_id_args: mark / del ----------------------------------------------

#[test]
fn id_args_accept_one_list() {
    assert_eq!(parse_id_args(&argv(&["1"])), Ok(vec![1]));
    assert_eq!(parse_id_args(&argv(&["1,2,3"])), Ok(vec![1, 2, 3]));
    assert_eq!(parse_id_args(&argv(&["1, 2, 3"])), Ok(vec![1, 2, 3]));
    assert_eq!(parse_id_args(&argv(&["1,", "2,", "3"])), Ok(vec![1, 2, 3]));
    assert_eq!(parse_id_args(&argv(&["1,1,2"])), Ok(vec![1, 2]));
}

#[test]
fn id_args_reject_a_second_word() {
    assert_eq!(
        parse_id_args(&argv(&["1", "2", "3"])),
        Err(IdListError::Trailing {
            word: "2".into(),
            list: "1".into()
        })
    );
    assert_eq!(
        parse_id_args(&argv(&["3", "1,2"])),
        Err(IdListError::Trailing {
            word: "1,2".into(),
            list: "3".into()
        })
    );
    assert_eq!(
        parse_id_args(&argv(&["1,2", "3,4"])),
        Err(IdListError::Trailing {
            word: "3,4".into(),
            list: "1,2".into()
        })
    );
    assert!(matches!(
        parse_id_args(&argv(&["1", "abc"])),
        Err(IdListError::Trailing { word, .. }) if word == "abc"
    ));
}

#[test]
fn id_args_reject_what_is_not_an_id() {
    assert!(matches!(parse_id_args(&argv(&["abc"])), Err(IdListError::NotAnId { .. })));
    assert!(matches!(parse_id_args(&argv(&["-"])), Err(IdListError::NotAnId { .. })));
    assert!(matches!(parse_id_args(&argv(&["1,abc"])), Err(IdListError::NotAnId { .. })));
    assert!(matches!(parse_id_args(&[]), Err(IdListError::Empty { .. })));
}

// --- parse_edit_args: ids, then text ----------------------------------------

/// `parse_edit_args` without anything after `--`.
fn edit(words: &[&str]) -> Result<(Vec<u32>, Option<Vec<String>>), IdListError> {
    parse_edit_args(&argv(words), &[])
}

#[test]
fn edit_args_id_only_means_the_editor() {
    assert_eq!(edit(&["3"]), Ok((vec![3], None)));
    assert_eq!(edit(&["1,2,3"]), Ok((vec![1, 2, 3], None)));
    assert_eq!(edit(&["1,", "2,", "3"]), Ok((vec![1, 2, 3], None)));
}

#[test]
fn edit_args_words_after_the_list_are_the_text() {
    assert_eq!(
        edit(&["5", "new", "title", "here"]),
        Ok((vec![5], Some(argv(&["new", "title", "here"]))))
    );
    assert_eq!(
        edit(&["1,5,4", " ,6", "new", "text"]),
        Ok((vec![1, 5, 4, 6], Some(argv(&["new", "text"]))))
    );
}

#[test]
fn edit_args_a_number_starting_the_text_is_text() {
    // `edit 1 2 text` sets "2 text" on task 1; `edit 3 1,000 units` keeps
    // task 1 alone.
    assert_eq!(edit(&["1", "2", "text"]), Ok((vec![1], Some(argv(&["2", "text"])))));
    assert_eq!(
        edit(&["3", "1,000", "units", "sold"]),
        Ok((vec![3], Some(argv(&["1,000", "units", "sold"]))))
    );
}

#[test]
fn edit_args_only_numbers_after_the_list_are_more_ids() {
    // `edit 1,2 3 -d 2w`, `edit 1 2 3`: the user forgot the commas; as the
    // text, the numbers would replace the texts of the tasks.
    assert_eq!(
        edit(&["1,2", "3"]),
        Err(IdListError::MoreIds {
            words: "3".into(),
            list: "1,2".into()
        })
    );
    assert_eq!(
        edit(&["1", "2", "3"]),
        Err(IdListError::MoreIds {
            words: "2 3".into(),
            list: "1".into()
        })
    );
    assert!(matches!(edit(&["1,2", "3,4"]), Err(IdListError::MoreIds { .. })));
    assert!(matches!(edit(&["1", "0"]), Err(IdListError::MoreIds { .. })));
    assert!(matches!(edit(&["1", "2, 3"]), Err(IdListError::MoreIds { .. })));
    // The message says how to write each of the two meanings.
    let msg = edit(&["1,2", "3"]).unwrap_err().to_string();
    assert!(msg.contains("rusk edit 1,2 -- 3"), "{msg}");
}

#[test]
fn edit_args_after_double_dash_are_text_as_they_are() {
    assert_eq!(
        parse_edit_args(&argv(&["1"]), &argv(&["42"])),
        Ok((vec![1], Some(argv(&["42"]))))
    );
    assert_eq!(
        parse_edit_args(&argv(&["1,2"]), &argv(&["3", "4"])),
        Ok((vec![1, 2], Some(argv(&["3", "4"]))))
    );
    assert_eq!(
        parse_edit_args(&argv(&["1", "run"]), &argv(&["-x", "-h"])),
        Ok((vec![1], Some(argv(&["run", "-x", "-h"]))))
    );
    // Ids go before `--`.
    assert_eq!(
        parse_edit_args(&[], &argv(&["1", "text"])),
        Err(IdListError::Empty { list: "".into() })
    );
}

#[test]
fn edit_args_flags_are_not_its_business() {
    // clap has taken `-d` / `-a` out before this runs; a leftover `-d` word
    // (after `--`) is text.
    assert_eq!(
        parse_edit_args(&argv(&["3"]), &argv(&["-d", "2w"])),
        Ok((vec![3], Some(argv(&["-d", "2w"]))))
    );
}

#[test]
fn edit_args_reject_what_is_not_an_id() {
    assert!(matches!(
        edit(&["abc", "text"]),
        Err(IdListError::NotAnId { part, .. }) if part == "abc"
    ));
    assert!(matches!(
        edit(&["1,4294967296", "zzz"]),
        Err(IdListError::NotAnId { part, .. }) if part == "4294967296"
    ));
    assert!(matches!(edit(&[]), Err(IdListError::Empty { .. })));
}
