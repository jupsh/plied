//! Verbatim quotation matching.
//!
//! A quotation matches when, after the same normalisation of both sides, each of its parts appears
//! in the source in order. Parts are separated by an ellipsis ("…" or "...") standing for omitted
//! text. Normalisation: Unicode NFKC, curly quotes made straight, Markdown emphasis and code marks
//! (`*`, backtick) removed, hyphenation at line ends joined, runs of whitespace collapsed. A match
//! says the words are there; it does not say they support the claim.

use std::sync::LazyLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

use crate::doc::take;

static ELLIPSIS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*(?:…|\.\.\.)\s*").unwrap());
static HYPHEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\w)-\n\s*(\w)").unwrap());
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

fn nfkc_quotes(text: &str) -> String {
    text.nfkc()
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' => '\'',
            '\u{201c}' | '\u{201d}' => '"',
            '\u{a0}' => ' ',
            c => c,
        })
        .collect()
}

pub fn normalise(text: &str) -> String {
    let t = nfkc_quotes(text).replace(['*', '`'], "");
    let t = HYPHEN.replace_all(&t, "${1}${2}"); // hyphenated line breaks in page text
    SPACES.replace_all(&t, " ").trim().to_string()
}

pub fn quote_parts(quote: &str) -> Vec<String> {
    // NFKC turns "…" into "..."; split on either, then normalise each part
    let q = nfkc_quotes(quote);
    ELLIPSIS.split(&q).map(normalise).filter(|p| !p.is_empty()).collect()
}

pub fn find_quote(quote: &str, source: &str) -> bool {
    let parts = quote_parts(quote);
    if parts.is_empty() {
        return false;
    }
    let src = normalise(source);
    let mut pos = 0;
    for part in &parts {
        match src[pos..].find(part.as_str()) {
            Some(i) => pos += i + part.len(),
            None => return false,
        }
    }
    true
}

/// The source passage sharing the longest prefix with the quote's first part (for error messages).
pub fn closest(quote: &str, source: &str, width: usize) -> Option<String> {
    let parts = quote_parts(quote);
    let first = parts.first()?;
    let src = normalise(source);
    let n0 = first.chars().count().min(40);
    for n in (5..=n0).rev() {
        if let Some(i) = src.find(take(first, n)) {
            let start = src[..i].chars().count();
            let len = width.max(n + 20);
            return Some(src.chars().skip(start).take(len).collect());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "- **2026-10-24 — The revised initialization \"is accepted as a warm-up shortcut** provided the\n  original and revised starts converge to the same late-window behaviour, rechecked if walk-on\n  rates change.\" `C2` was selected provisionally.";

    #[test]
    fn whitespace_and_markdown_are_normalised() {
        assert!(find_quote("is accepted as a warm-up shortcut provided the original and revised starts converge", SRC));
        assert!(find_quote("C2 was selected provisionally", SRC));
    }

    #[test]
    fn ellipsis_marks_omitted_text_in_order() {
        assert!(find_quote("is accepted as a warm-up shortcut … rechecked if walk-on rates change", SRC));
        assert!(find_quote("accepted ... late-window behaviour", SRC));
        assert!(!find_quote("late-window behaviour … accepted", SRC));
    }

    #[test]
    fn paraphrase_and_case_changes_fail() {
        assert!(!find_quote("is accepted as a shortcut for warm-up", SRC));
        assert!(!find_quote("c2 was selected provisionally", SRC));
    }

    #[test]
    fn curly_quotes_and_hyphenated_line_breaks() {
        assert!(find_quote("in the fed condition", "lasted 12.5 s in the\nfed condi-\ntion."));
        assert!(find_quote("\u{201c}is accepted", "The revised \"is accepted"));
    }
}
