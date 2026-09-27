//! Finding a phrase in a task's text the way the list shows the text.
//!
//! The list prints a text word by word: a run of spaces, a tab or a line
//! break between two words is one space, or the end of a row. `rusk search`
//! matches the same way — the words of the query in order, where any run of
//! whitespace in the query matches any run in the text (REVIEW №36, №115).
//! Case is folded one character at a time, the same way on both sides, so a
//! text always finds itself: a final sigma (ς) is a sigma (σ), which is what
//! the capital Σ folds to, and ß is ss, like ẞ (REVIEW №37). Accents are not
//! folded: `cafe` does not find `café`, whether the é is one character or an
//! e with a combining accent — a match begins and ends between grapheme
//! clusters, never inside one (nor inside an emoji joined by ZWJ).
//!
//! A match is a byte range of the text it was found in, so the list can
//! highlight it wherever wrapping puts its words.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

/// A search phrase, folded for matching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    folded: Vec<char>,
}

/// One character of a folded text, and the source character it comes from.
#[derive(Debug, Clone, Copy)]
struct Unit {
    ch: char,
    /// Byte range of the source character (of the whole run, for the one
    /// space that stands for a run of whitespace).
    start: usize,
    end: usize,
    /// First and last of the units the source character folds to: a match
    /// takes a character whole, never half of the `ss` of an ß.
    first: bool,
    last: bool,
}

/// `text` folded for matching, one [`Unit`] at a time.
fn fold(text: &str) -> Vec<Unit> {
    let mut units: Vec<Unit> = Vec::with_capacity(text.len());
    let mut chars = text.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        let mut end = start + c.len_utf8();
        if c.is_whitespace() {
            while let Some((at, next)) = chars.next_if(|(_, next)| next.is_whitespace()) {
                end = at + next.len_utf8();
            }
            units.push(Unit { ch: ' ', start, end, first: true, last: true });
            continue;
        }
        let from = units.len();
        for lower in c.to_lowercase() {
            let folded: &[char] = match lower {
                'ς' => &['σ'],
                'ß' => &['s', 's'],
                _ => &[lower],
            };
            for &ch in folded {
                units.push(Unit { ch, start, end, first: false, last: false });
            }
        }
        units[from].first = true;
        let last = units.len() - 1;
        units[last].last = true;
    }
    units
}

impl Query {
    /// The phrase `query`: its words, with whatever whitespace between them.
    pub fn new(query: &str) -> Self {
        Self {
            folded: fold(query.trim()).iter().map(|unit| unit.ch).collect(),
        }
    }

    /// A query of nothing but whitespace finds nothing.
    pub fn is_empty(&self) -> bool {
        self.folded.is_empty()
    }

    /// Byte ranges of `text` the query matches, left to right, not
    /// overlapping.
    pub fn find_all(&self, text: &str) -> Vec<Range<usize>> {
        let n = self.folded.len();
        let mut found = Vec::new();
        if n == 0 {
            return found;
        }
        let hay = fold(text);
        let clusters: Vec<usize> = text
            .grapheme_indices(true)
            .map(|(at, _)| at)
            .chain([text.len()])
            .collect();
        let between_clusters = |at: usize| clusters.binary_search(&at).is_ok();
        let mut i = 0;
        while i + n <= hay.len() {
            let window = &hay[i..i + n];
            if window[0].first
                && window[n - 1].last
                && window.iter().zip(&self.folded).all(|(unit, &ch)| unit.ch == ch)
                && between_clusters(window[0].start)
                && between_clusters(window[n - 1].end)
            {
                found.push(window[0].start..window[n - 1].end);
                i += n;
            } else {
                i += 1;
            }
        }
        found
    }

    /// Whether the query occurs in `text`.
    pub fn matches(&self, text: &str) -> bool {
        !self.find_all(text).is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found<'a>(query: &str, text: &'a str) -> Vec<&'a str> {
        Query::new(query)
            .find_all(text)
            .into_iter()
            .map(|range| &text[range])
            .collect()
    }

    #[test]
    fn case_is_folded_on_both_sides_alike() {
        assert_eq!(found("groc", "Buy Groceries"), ["Groc"]);
        assert_eq!(found("ΩΜΈΓΑ", "table by Ωμέγα-3"), ["Ωμέγα"]);
        assert_eq!(found("foo", "foo bar Foo"), ["foo", "Foo"]);
        assert_eq!(found("world", "hello"), Vec::<&str>::new());
    }

    /// REVIEW №37: `str::to_lowercase` made the final Σ of a query a ς,
    /// the text folded char by char had σ: "ΟΔΟΣ" did not find itself.
    #[test]
    fn a_greek_word_in_capitals_finds_itself() {
        assert_eq!(found("ΟΔΟΣ", "ΟΔΟΣ ΕΡΜΟΥ"), ["ΟΔΟΣ"]);
        assert_eq!(found("οδοσ", "ΟΔΟΣ ΕΡΜΟΥ"), ["ΟΔΟΣ"]);
        assert_eq!(found("ΟΔΌΣ", "οδός ερμού"), ["οδός"]);
        assert_eq!(found("οδός", "ΟΔΌΣ"), ["ΟΔΌΣ"]);
        // Accents are not folded.
        assert_eq!(found("ΟΔΟΣ", "οδός"), Vec::<&str>::new());
    }

    #[test]
    fn sharp_s_is_ss() {
        assert_eq!(found("STRASSE", "Hauptstraße 5"), ["straße"]);
        assert_eq!(found("straße", "HAUPTSTRASSE"), ["STRASSE"]);
        assert_eq!(found("ß", "GROẞ"), ["ẞ"]);
        // Never half of one: "s" alone does not end inside the ß.
        assert_eq!(found("aß", "as"), Vec::<&str>::new());
        assert_eq!(found("s", "ß"), Vec::<&str>::new());
    }

    /// A character that lowercases to several (İ is i + a combining dot) is
    /// matched whole or not at all.
    #[test]
    fn a_match_takes_a_character_whole() {
        assert_eq!(found("i", "İ"), Vec::<&str>::new());
        assert_eq!(found("i\u{307}", "İx"), ["İ"]);
    }

    /// REVIEW №36, №115: the list shows a run of whitespace as one space (or
    /// a row break), and a phrase matches whatever run is between its words.
    #[test]
    fn whitespace_runs_match_each_other() {
        assert_eq!(found("alpha beta", "double  space alpha  beta"), ["alpha  beta"]);
        assert_eq!(found("alpha  beta", "alpha beta"), ["alpha beta"]);
        assert_eq!(found(" buy\tmilk ", "  buy   milk  "), ["buy   milk"]);
        assert_eq!(found("first second", "first\nsecond"), ["first\nsecond"]);
        assert_eq!(found("for  comparison", "ingredients for comparison"), ["for comparison"]);
    }

    /// Review of R19: `cafe` found the `cafe` of an e with a combining
    /// accent (NFD), and the highlight split the letter from its accent.
    #[test]
    fn a_match_is_whole_grapheme_clusters() {
        assert_eq!(found("cafe", "cafe\u{301} au lait"), Vec::<&str>::new());
        assert_eq!(found("cafe", "café au lait"), Vec::<&str>::new());
        assert_eq!(found("cafe\u{301}", "CAFE\u{301}!"), ["CAFE\u{301}"]);
        assert_eq!(found("👨", "👨\u{200d}👩\u{200d}👧"), Vec::<&str>::new());
        assert_eq!(found("👨", "👨 alone"), ["👨"]);
    }

    #[test]
    fn matches_do_not_overlap_and_blank_queries_find_nothing() {
        assert_eq!(found("aa", "aaaa a"), ["aa", "aa"]);
        assert_eq!(found("abc", "abc abc"), ["abc", "abc"]);
        assert!(Query::new("  \t").is_empty());
        assert_eq!(found("   ", "a b"), Vec::<&str>::new());
        assert!(!Query::new("").matches("anything"));
    }

    #[test]
    fn ranges_are_byte_ranges_of_the_text() {
        let text = "ωμέγα Straße";
        let ranges = Query::new("STRASSE").find_all(text);
        let strasse = text.find('S').unwrap()..text.len();
        assert_eq!(ranges, [strasse]);
    }
}
