//! Optional author-written thread synopsis in the Markdown body (FORMAT.md §3.7).
//! This extracts prose; it does not establish its accuracy or freshness.

/// The first `## Summary` section, before History, outside fenced/indented code.
/// A heading of level 1 or 2 ends it; an empty first section is absent.
pub fn extract(body: &str) -> Option<String> {
    let mut collecting = false;
    let mut lines = vec![];
    let mut fence: Option<(char, usize)> = None;
    for line in body.lines() {
        let indent = line.bytes().take_while(|b| *b == b' ').count();
        let block = &line[indent..];
        let eligible = indent <= 3 && !block.starts_with('\t');
        let marker = block.chars().next().filter(|c| matches!(c, '`' | '~'));
        let count = marker.map_or(0, |c| block.chars().take_while(|x| *x == c).count());
        if let Some((c, n)) = fence {
            if eligible && marker == Some(c) && count >= n && block[count..].trim().is_empty() {
                fence = None;
            }
            if collecting { lines.push(line); }
            continue;
        }
        if eligible && count >= 3 && (marker != Some('`') || !block[count..].contains('`')) {
            fence = marker.map(|c| (c, count));
            if collecting { lines.push(line); }
            continue;
        }
        let level = block.bytes().take_while(|b| *b == b'#').count();
        let heading = eligible && (1..=6).contains(&level)
            && (block.len() == level || block[level..].starts_with(char::is_whitespace));
        if heading && level <= 2 {
            if collecting { break; }
            match (level, block[level..].trim()) {
                (2, "History") => break,
                (2, "Summary") => { collecting = true; continue; }
                _ => {}
            }
        }
        if collecting { lines.push(line); }
    }
    let text = lines.join("\n");
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::extract;

    #[test]
    fn fenced_examples_indented_code_and_history_are_not_summaries() {
        let body = "```md\n## Summary\nexample\n```\n    ## Summary\n    code\n~~~md\n## Summary\nexample\n~~~\n## History\n## Summary\nhistorical note";
        assert_eq!(extract(body), None);
    }

    #[test]
    fn first_section_keeps_conditions_subheadings_and_fenced_examples() {
        let body = "Intro\n## Summary\n\nInteger case only.\n### Limit\nReal case unresolved.\n```md\n## Details\n```\n\n## Evidence\nnot in synopsis\n## Summary\nlater replacement";
        assert_eq!(extract(body).as_deref(), Some("Integer case only.\n### Limit\nReal case unresolved.\n```md\n## Details\n```"));
        assert_eq!(extract("## Summary\n\n## Summary\nlater"), None);
    }

    #[test]
    fn heading_spelling_boundaries_and_fence_lengths_are_explicit() {
        assert_eq!(extract("## summary\nlowercase\n### Summary\nnested"), None);
        assert_eq!(extract("  ## Summary  \nαβγ\n# Next\nexcluded").as_deref(), Some("αβγ"));
        assert_eq!(extract("````\n```\n## Summary\nexample\n````\n## Summary\nactual").as_deref(), Some("actual"));
        assert_eq!(extract("## Summary\n##not-a-heading\n## History\nold").as_deref(), Some("##not-a-heading"));
    }
}
