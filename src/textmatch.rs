//! Verbatim quotation matching.
//!
//! A quotation matches when, after the same normalisation of both sides, each of its parts appears
//! in the source in order. Parts are separated by an ellipsis ("…" or "...") standing for omitted
//! text. Normalisation: Unicode NFKC, curly quotes made straight, Markdown emphasis and code marks
//! (`*`, backtick) and line-leading blockquote prefixes removed, hyphenation at line ends joined,
//! runs of whitespace collapsed. A match
//! says the words are there; it does not say they support the claim.

use std::sync::LazyLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

static ELLIPSIS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*(?:…|\.\.\.)\s*").unwrap());
static BLOCKQUOTE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^ {0,3}(?:>[ \t]?)+").unwrap());
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
    normalise_with_blockquotes(text, true)
}

fn normalise_with_blockquotes(text: &str, strip: bool) -> String {
    let t = nfkc_quotes(text).replace(['*', '`'], "");
    let t = if strip { BLOCKQUOTE.replace_all(&t, "") } else { t.into() };
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

/// A bounded passage around the first divergence in the first unmatched quote part.
/// Earlier ellipsis-separated parts are consumed just as in `find_quote`.
pub fn closest(quote: &str, source: &str, width: usize) -> Option<String> {
    let src = normalise(source);
    let mut pos = 0;
    for part in quote_parts(quote) {
        if let Some(i) = src[pos..].find(&part) {
            pos += i + part.len();
            continue;
        }
        let chars: Vec<char> = part.chars().collect();
        for n in (5..=chars.len().min(40)).rev() {
            let prefix: String = chars[..n].iter().collect();
            let mut best = None;
            for (i, _) in src[pos..].match_indices(&prefix) {
                let shared = src[pos + i..].chars().zip(&chars).take_while(|(a, b)| a == *b).count();
                if best.is_none_or(|(_, longest)| shared > longest) {
                    best = Some((pos + i, shared));
                }
            }
            if let Some((i, shared)) = best {
                let start = src[..i].chars().count();
                let divergence = start + shared;
                let offset = divergence.saturating_sub(width / 2).max(start);
                let excerpt: String = src.chars().skip(offset).take(width).collect();
                let before = if offset > 0 { "…" } else { "" };
                let after = if offset + excerpt.chars().count() < src.chars().count() { "…" } else { "" };
                return Some(format!("{before}{excerpt}{after}"));
            }
        }
        return None;
    }
    None
}

/// Explain blockquote removal only when retaining the source's markers would make the quote match.
pub fn blockquote_mismatch(quote: &str, source: &str) -> bool {
    let src = normalise_with_blockquotes(source, false);
    let mut pos = 0;
    let parts = quote_parts(quote);
    if parts.is_empty() || find_quote(quote, source) {
        return false;
    }
    for part in parts {
        let Some(i) = src[pos..].find(&part) else { return false };
        pos += i + part.len();
    }
    true
}

/// Human-readable mismatch context, shared by record and literature diagnostics.
pub fn mismatch_hint(quote: &str, source: &str) -> String {
    let near = closest(quote, source, 60).map(|n| format!("; nearest normalised text: “{n}”")).unwrap_or_default();
    let marker = if blockquote_mismatch(quote, source) {
        "; a line-leading `>` in the source was treated as a blockquote marker; rewrap a comparison so `>` stays inline"
    } else {
        ""
    };
    format!("{near}{marker}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mismatch_context_reaches_the_divergence_including_later_parts() {
        let shared = "the first rule applies to every run, and the second applies only if ";
        let quote = format!("{shared}> 5 % of visits end early.");
        let source = format!("- Rules: {shared}\n  > 5 % of visits end early.");
        let near = closest(&quote, &source, 60).unwrap();
        assert!(near.contains("applies only if 5 % of visits"), "{near}");
        assert!(blockquote_mismatch(&quote, &source));
        let later = closest(&format!("Rules … {quote}"), &source, 60).unwrap();
        assert_eq!(near, later);
        assert!(!blockquote_mismatch("Rules … an unrelated mistake", &source));
        assert!(closest("Rules … an unrelated mistake", &source, 60).is_none());
        assert!(closest("Rules", &source, 60).is_none());
    }

    #[test]
    fn mismatch_context_uses_the_longest_candidate_and_unicode_boundaries() {
        let first = "évidence has a repeated and sufficiently long introduction, then ";
        let quote = format!("{first}a mistaken conclusion.");
        let source = format!("évidence has a repeated and wrong start. {first}the actual conclusion.");
        let near = closest(&quote, &source, 60).unwrap();
        assert!(near.contains("then the actual conclusion"), "{near}");
        assert!(near.chars().count() <= 62);
        assert!(!blockquote_mismatch(&quote, &source));
    }

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

    #[test]
    fn blockquote_prefixes_are_removed_before_joining_lines() {
        let source = "> Keep the defaults as drafted. The second\n> rule applies to every run, not only the first.";
        assert!(find_quote("The second rule applies to every run, not only the first.", source));
        assert!(find_quote("> The second\n> rule applies to every run, not only the first.", source));
        assert!(find_quote("Keep defaults, with x > y.", "  > > Keep de-\n  >> faults, with x > y."));
        assert!(find_quote("A tab follows the marker.", ">\tA tab follows\n>the marker."));
    }

    #[test]
    fn comparison_signs_and_indented_code_are_preserved() {
        assert_eq!(normalise("x > y and x >> y"), "x > y and x >> y");
        assert_eq!(normalise("    > quoted code"), "> quoted code");
        assert_eq!(normalise("\t> quoted code"), "> quoted code");
        assert!(!find_quote("x y", "x > y"));
        assert!(!find_quote("The rule applies", "> The second\n> rule applies"));
    }
}
