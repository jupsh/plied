//! The frontmatter subset of YAML (FORMAT.md §3.1) and its canonical writer (§3.2).

use std::sync::LazyLock;

use regex::Regex;

use crate::doc::{Map, Node, clen};

static KEY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([A-Za-z_][\w.-]*):(?:\s+(.*))?$").unwrap());
const SPECIAL_WORDS: [&str; 10] = ["true", "false", "yes", "no", "on", "off", "null", "~", "y", "n"];
static NUMBERISH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^[-+]?(\d[\d_]*)?(\.\d*)?([eE][-+]?\d+)?$|^0[xob]|^[-+]?\.(inf|nan)$").unwrap()
});
static DATEISH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{4}-\d\d?-\d\d?|^[-+]?\d+(:[0-5]?\d)+(\.\d*)?$").unwrap());

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

type R<T> = Result<T, Error>;

fn err<T>(msg: String) -> R<T> {
    Err(Error(msg))
}

/// Split a file into its frontmatter (None if there is none) and body.
pub fn split_frontmatter(text: &str) -> R<(Option<String>, String)> {
    if !(text.starts_with("---\n") || text.starts_with("---\r\n")) {
        return Ok((None, text.to_string()));
    }
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    for i in 1..lines.len() {
        if lines[i].trim_end_matches(['\r', '\n']) == "---" {
            return Ok((Some(lines[1..i].concat()), lines[i + 1..].concat()));
        }
    }
    err("frontmatter opened with --- but never closed".into())
}

pub fn join_frontmatter(meta_text: &str, body: &str) -> String {
    let nl = if meta_text.ends_with('\n') { "" } else { "\n" };
    format!("---\n{meta_text}{nl}---\n{body}")
}

// ------------------------------------------------------------------------------- parse

struct Line {
    n: usize,
    /// None for blank and comment lines, which matter only inside block scalars
    indent: Option<usize>,
    text: String,
}

struct Lines {
    items: Vec<Line>,
    i: usize,
}

impl Lines {
    fn new(text: &str) -> R<Self> {
        let mut items = vec![];
        for (n, raw) in text.lines().enumerate() {
            let n = n + 1;
            let content = raw.trim_start();
            if raw[..raw.len() - content.len()].contains('\t') {
                return err(format!("line {n}: tabs are not allowed in indentation"));
            }
            let indent = if content.trim().is_empty() || content.starts_with('#') { None } else { Some(raw.len() - raw.trim_start_matches(' ').len()) };
            let text = if indent.is_some() { raw.trim_end() } else { raw };
            items.push(Line { n, indent, text: text.to_string() });
        }
        Ok(Lines { items, i: 0 })
    }

    /// The next significant line: (number, indent, content after the indent).
    fn peek(&self) -> Option<(usize, usize, &str)> {
        self.items[self.i..].iter().find_map(|l| l.indent.map(|ind| (l.n, ind, &l.text[ind..])))
    }

    fn take(&mut self) {
        while self.items.get(self.i).is_some_and(|l| l.indent.is_none()) {
            self.i += 1;
        }
        self.i += 1;
    }
}

fn is_item(content: &str) -> bool {
    content.starts_with("- ") || content == "-"
}

fn key_value(content: &str) -> Option<(String, String)> {
    KEY.captures(content).map(|m| (m[1].to_string(), m.get(2).map_or("", |x| x.as_str()).trim().to_string()))
}

pub fn parse(text: &str) -> R<Map> {
    let mut lines = Lines::new(text)?;
    let Some((n, ind, _)) = lines.peek() else { return Ok(Map::new()) };
    if ind != 0 {
        return err(format!("line {n}: top level must start at column 0"));
    }
    let out = parse_mapping(&mut lines, 0)?;
    if let Some((n, _, _)) = lines.peek() {
        return err(format!("line {n}: unexpected indentation"));
    }
    Ok(out)
}

/// Top-level `key: rest-of-line` pairs only; for other tools' frontmatter (§3.1).
pub fn parse_lenient(text: &str) -> Map {
    let mut out = Map::new();
    for raw in text.lines() {
        if raw.is_empty() || raw.starts_with([' ', '#', '-']) {
            continue;
        }
        if let Some((k, v)) = key_value(raw.trim_end()) {
            out.insert(k, Node::Str(v));
        }
    }
    out
}

fn parse_mapping(lines: &mut Lines, indent: usize) -> R<Map> {
    let mut out = Map::new();
    loop {
        let Some((n, ind, content)) = lines.peek() else { return Ok(out) };
        if ind < indent {
            return Ok(out);
        }
        if ind > indent {
            return err(format!("line {n}: unexpected indentation"));
        }
        if is_item(content) {
            return Ok(out); // a list at the same indent belongs to the caller's key
        }
        let Some((key, value)) = key_value(content) else {
            return err(format!("line {n}: expected 'key: value', got {content:?}"));
        };
        lines.take();
        if out.contains_key(&key) {
            return err(format!("line {n}: duplicate key {key:?}"));
        }
        let v = parse_value(lines, &value, indent, n)?;
        out.insert(key, v);
    }
}

fn parse_value(lines: &mut Lines, value: &str, indent: usize, n: usize) -> R<Node> {
    if matches!(value, "|" | "|-" | ">" | ">-") {
        return parse_block_scalar(lines, indent, value).map(Node::Str);
    }
    if !value.is_empty() {
        return scalar(value, n);
    }
    let Some((_, ind, content)) = lines.peek() else { return Ok(Node::Null) };
    let item = is_item(content);
    if ind > indent {
        return if item { parse_list(lines, ind).map(Node::List) } else { parse_mapping(lines, ind).map(Node::Map) };
    }
    if ind == indent && item {
        return parse_list(lines, ind).map(Node::List);
    }
    Ok(Node::Null)
}

fn parse_list(lines: &mut Lines, indent: usize) -> R<Vec<Node>> {
    let mut out = vec![];
    loop {
        let Some((n, ind, content)) = lines.peek() else { return Ok(out) };
        if ind > indent {
            return err(format!("line {n}: unexpected indentation in list"));
        }
        if ind < indent || !is_item(content) {
            return Ok(out);
        }
        let rest = content.get(2..).unwrap_or("").trim().to_string();
        lines.take();
        let kv = if rest.is_empty() || rest.starts_with(['"', '\'', '[']) { None } else { key_value(&rest) };
        if let Some((key, value)) = kv {
            let sub = ind + 2;
            let mut item = Map::new();
            item.insert(key, parse_value(lines, &value, sub, n)?);
            for (k, v) in parse_mapping(lines, sub)? {
                if item.contains_key(&k) {
                    return err(format!("line {n}: duplicate key {k:?}"));
                }
                item.insert(k, v);
            }
            out.push(Node::Map(item));
        } else if !rest.is_empty() {
            out.push(scalar(&rest, n)?);
        } else {
            out.push(match lines.peek() {
                Some((_, nind, c)) if nind > ind => {
                    if c.starts_with("- ") { Node::List(parse_list(lines, nind)?) } else { Node::Map(parse_mapping(lines, nind)?) }
                }
                _ => Node::Null,
            });
        }
    }
}

fn parse_block_scalar(lines: &mut Lines, indent: usize, style: &str) -> R<String> {
    let mut collected: Vec<String> = vec![];
    let mut block_indent = None;
    while let Some(line) = lines.items.get(lines.i) {
        let (n, ind, text) = match line.indent {
            Some(ind) => (line.n, ind, line.text.as_str()),
            None if line.text.trim().is_empty() => {
                collected.push(String::new());
                lines.i += 1;
                continue;
            }
            // a '#' line: text inside the block, a comment once outside its indentation
            None => (line.n, line.text.len() - line.text.trim_start_matches(' ').len(), line.text.trim_end()),
        };
        if ind <= indent {
            break;
        }
        let bi = *block_indent.get_or_insert(ind);
        if ind < bi {
            return err(format!("line {n}: block scalar indentation decreased"));
        }
        collected.push(text[bi..].to_string());
        lines.i += 1;
    }
    while collected.last().is_some_and(String::is_empty) {
        collected.pop();
    }
    let text = if style.starts_with('|') {
        collected.join("\n")
    } else {
        collected.split(|l| l.is_empty()).map(|para| para.iter().map(|l| l.trim()).collect::<Vec<_>>().join(" ")).collect::<Vec<_>>().join("\n")
    };
    Ok(if style.ends_with('-') { text } else { text + "\n" })
}

/// One scalar: quoted, a flow list, `{}`, or plain text.
pub fn scalar(value: &str, n: usize) -> R<Node> {
    if value.starts_with('"') {
        return match serde_json::from_str::<serde_json::Value>(value) {
            Ok(serde_json::Value::String(s)) => Ok(Node::Str(s)),
            Ok(_) => err(format!("line {n}: bad double-quoted string")),
            Err(e) => err(format!("line {n}: bad double-quoted string ({e})")),
        };
    }
    if let Some(inner) = value.strip_prefix('\'') {
        let Some(inner) = inner.strip_suffix('\'') else { return err(format!("line {n}: unterminated single-quoted string")) };
        if inner.replace("''", "").contains('\'') {
            return err(format!("line {n}: stray quote in single-quoted string"));
        }
        return Ok(Node::Str(inner.replace("''", "'")));
    }
    if value.starts_with('[') {
        return flow_list(value, n).map(Node::List);
    }
    if value == "{}" {
        return Ok(Node::Map(Map::new()));
    }
    if value.starts_with('{') {
        return err(format!("line {n}: flow mappings are not supported; use a block mapping"));
    }
    Ok(Node::Str(value.to_string()))
}

fn flow_list(value: &str, n: usize) -> R<Vec<Node>> {
    let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) else {
        return err(format!("line {n}: unterminated flow list"));
    };
    let inner: Vec<char> = inner.trim().chars().collect();
    if inner.is_empty() {
        return Ok(vec![]);
    }
    let (mut items, mut cur, mut i) = (vec![], String::new(), 0);
    while i < inner.len() {
        let c = inner[i];
        let end = match c {
            '"' => {
                let mut j = i + 1;
                while j < inner.len() && inner[j] != '"' {
                    j += if inner[j] == '\\' { 2 } else { 1 };
                }
                Some(j)
            }
            '\'' => {
                let mut j = i + 1;
                while j < inner.len() && !(inner[j] == '\'' && inner.get(j + 1) != Some(&'\'')) {
                    j += if inner[j] == '\'' { 2 } else { 1 };
                }
                Some(j)
            }
            _ => None,
        };
        if let Some(j) = end {
            cur.extend(&inner[i..(j + 1).min(inner.len())]);
            i = j + 1;
            continue;
        }
        if "[]{}".contains(c) {
            return err(format!("line {n}: nested collections in flow lists are not supported"));
        }
        if c == ',' {
            items.push(std::mem::take(&mut cur).trim().to_string());
        } else {
            cur.push(c);
        }
        i += 1;
    }
    items.push(cur.trim().to_string());
    if items.iter().any(String::is_empty) {
        return err(format!("line {n}: empty item in flow list"));
    }
    items.iter().map(|it| scalar(it, n)).collect()
}

// -------------------------------------------------------------------------------- dump

const WRAP: usize = 100;

fn printable(c: char) -> bool {
    !(c.is_control() || (c.is_whitespace() && c != ' '))
}

fn plain_ok(s: &str, flow: bool) -> bool {
    !(s.is_empty()
        || s != s.trim()
        || s.contains(['\n', '\r'])
        || s.starts_with(|c| "-?:,[]{}#&*!|>'\"%@`".contains(c))
        || s.contains(": ")
        || s.contains(" #")
        || s.ends_with(':')
        || SPECIAL_WORDS.contains(&s.to_lowercase().as_str())
        || (NUMBERISH.is_match(s) && s.chars().any(char::is_numeric))
        || DATEISH.is_match(s) // YAML 1.1 reads these as dates or base-60 numbers
        || (flow && s.contains([',', '[', ']', '{', '}']))
        || !s.chars().all(printable))
}

fn quote(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_default()
}

fn dump_scalar(v: &Node, flow: bool) -> String {
    match v {
        Node::Null => String::new(),
        Node::Int(n) => n.to_string(),
        Node::Str(s) if plain_ok(s, flow) => s.clone(),
        Node::Str(s) => quote(s),
        Node::Map(m) if m.is_empty() => "{}".into(),
        Node::List(l) if l.is_empty() => "[]".into(),
        other => quote(&other.to_string()),
    }
}

fn is_scalar(v: &Node) -> bool {
    matches!(v, Node::Str(_) | Node::Int(_))
}

fn foldable(v: &Node) -> bool {
    let Node::Str(s) = v else { return false };
    clen(s) > WRAP && !s.contains('\n') && !s.contains("  ") && s == s.trim() && s.chars().all(printable)
}

fn fold(s: &str, indent: usize) -> Vec<String> {
    let width = 40.max(WRAP.saturating_sub(indent));
    let mut lines: Vec<String> = vec![];
    let mut cur = String::new();
    for w in s.split(' ') {
        if !cur.is_empty() && clen(&cur) + 1 + clen(w) > width {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(w);
    }
    lines.push(cur);
    lines.into_iter().map(|l| format!("{}{l}", " ".repeat(indent))).collect()
}

/// Canonical frontmatter text: `order` first, then any other keys in their own order.
pub fn dump(data: &Map, order: &[&str]) -> String {
    let mut out = vec![];
    for k in order.iter().filter(|k| data.contains_key(**k)) {
        dump_pair(&mut out, k, &data[*k], 0);
    }
    for (k, v) in data.iter().filter(|(k, _)| !order.contains(&k.as_str())) {
        dump_pair(&mut out, k, v, 0);
    }
    out.join("\n") + "\n"
}

fn dump_pair(out: &mut Vec<String>, key: &str, v: &Node, indent: usize) {
    let pad = " ".repeat(indent);
    match v {
        Node::Map(m) if !m.is_empty() => {
            out.push(format!("{pad}{key}:"));
            for (k, v) in m {
                dump_pair(out, k, v, indent + 2);
            }
        }
        Node::List(items) if !items.is_empty() => {
            if items.iter().all(is_scalar) && !items.iter().any(foldable) {
                let flow = format!("[{}]", items.iter().map(|x| dump_scalar(x, true)).collect::<Vec<_>>().join(", "));
                if indent + clen(key) + 2 + clen(&flow) <= WRAP {
                    out.push(format!("{pad}{key}: {flow}"));
                    return;
                }
            }
            out.push(format!("{pad}{key}:"));
            for x in items {
                dump_item(out, x, indent + 2);
            }
        }
        Node::Str(s) if foldable(v) && indent + clen(key) + 2 + clen(s) > WRAP => {
            out.push(format!("{pad}{key}: >-"));
            out.extend(fold(s, indent + 2));
        }
        other => {
            let s = dump_scalar(other, false);
            out.push(if s.is_empty() { format!("{pad}{key}:") } else { format!("{pad}{key}: {s}") });
        }
    }
}

fn dump_item(out: &mut Vec<String>, x: &Node, indent: usize) {
    let pad = " ".repeat(indent);
    match x {
        Node::Map(m) if !m.is_empty() => {
            for (i, (k, v)) in m.iter().enumerate() {
                let mut tmp = vec![];
                dump_pair(&mut tmp, k, v, indent + 2);
                if i == 0 {
                    tmp[0] = format!("{pad}- {}", &tmp[0][indent + 2..]);
                }
                out.extend(tmp);
            }
        }
        Node::List(items) if !items.is_empty() => {
            out.push(format!("{pad}-"));
            for y in items {
                dump_item(out, y, indent + 2);
            }
        }
        Node::Str(s) if foldable(x) => out.push(format!("{pad}- {}", quote(s))),
        other => out.push(format!("{pad}- {}", dump_scalar(other, false)).trim_end().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pairs: &[(&str, Node)]) -> Map {
        pairs.iter().cloned().map(|(k, v)| (k.to_string(), v)).collect()
    }

    fn s(x: &str) -> Node {
        Node::Str(x.into())
    }

    fn cases() -> Vec<Map> {
        vec![
            m(&[("id", s("T017")), ("title", s("Contact split: during vs after")), ("next", Node::Int(1))]),
            m(&[("links", Node::List(vec![s("D134 §2"), s("F093"), s("a, b")])), ("empty", Node::List(vec![])), ("none", Node::Null)]),
            m(&[("approval", Node::Map(m(&[("state", s("not approved")), ("by", s("D138"))])))]),
            m(&[(
                "cites",
                Node::List(vec![
                    Node::Map(m(&[("paper", s("Smith2020")), ("page", Node::Int(3)), ("quote", s("x: y"))])),
                    Node::Map(m(&[("paper", s("A")), ("page", Node::Int(1))])),
                ]),
            )]),
            m(&[("wording", s(&("word ".repeat(40) + "end")))]),
            m(&[(
                "tricky",
                Node::List(["no", "yes", "1.10", "2026-10-24", "17:18", "-x", "#y", "@z", "a #b", "trailing:"].iter().map(|x| s(x)).collect()),
            )]),
            m(&[("multi", s("line one\nline two")), ("spaces", s("two  spaces")), ("quote", s("say \"hi\"")), ("unicode", s("≈ µL — “q”"))]),
        ]
    }

    /// What the parser gives back: integers come back as their text.
    fn as_parsed(v: &Node) -> Node {
        match v {
            Node::Int(n) => Node::Str(n.to_string()),
            Node::List(l) => Node::List(l.iter().map(as_parsed).collect()),
            Node::Map(m) => Node::Map(m.iter().map(|(k, v)| (k.clone(), as_parsed(v))).collect()),
            other => other.clone(),
        }
    }

    #[test]
    fn round_trip() {
        for data in cases() {
            let text = dump(&data, &[]);
            let back = parse(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
            assert_eq!(back, as_parsed(&Node::Map(data.clone())).map().unwrap().clone(), "{text}");
        }
    }

    fn yaml_strings(v: &serde_yaml::Value) -> Node {
        match v {
            serde_yaml::Value::Null => Node::Null,
            serde_yaml::Value::Bool(b) => Node::Str(b.to_string()),
            serde_yaml::Value::Number(n) => Node::Str(n.to_string()),
            serde_yaml::Value::String(s) => Node::Str(s.clone()),
            serde_yaml::Value::Sequence(x) => Node::List(x.iter().map(yaml_strings).collect()),
            serde_yaml::Value::Mapping(m) => Node::Map(m.iter().map(|(k, v)| (k.as_str().unwrap_or("").to_string(), yaml_strings(v))).collect()),
            serde_yaml::Value::Tagged(t) => yaml_strings(&t.value),
        }
    }

    #[test]
    fn standard_yaml_reads_the_same() {
        for data in cases() {
            let text = dump(&data, &[]);
            let theirs: serde_yaml::Value = serde_yaml::from_str(&text).unwrap();
            assert_eq!(Node::Map(parse(&text).unwrap()), yaml_strings(&theirs), "{text}");
        }
    }

    #[test]
    fn plain_scalars_keep_colons() {
        assert_eq!(parse("title: Record tooling: build it\n").unwrap()["title"], s("Record tooling: build it"));
    }

    #[test]
    fn folded_block() {
        assert_eq!(parse("w: >-\n  one two\n  three\n").unwrap()["w"], s("one two three"));
    }

    #[test]
    fn list_at_key_indent() {
        let p = parse("a:\n- x\n- y\nb: z\n").unwrap();
        assert_eq!(p["a"], Node::List(vec![s("x"), s("y")]));
        assert_eq!(p["b"], s("z"));
    }

    #[test]
    fn errors() {
        for bad in ["a: 1\na: 2\n", "  a: 1\n", "a: [x, [y]]\n", "a: {b: c}\n", "a: \"unterminated\n", "a:\n\t- x\n"] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn lenient_reads_other_tools_frontmatter() {
        let p = parse_lenient("id: B023\nnote:\ntitle: a: b\n  nested: x\n");
        assert_eq!(p, m(&[("id", s("B023")), ("note", s("")), ("title", s("a: b"))]));
    }

    #[test]
    fn hash_lines_inside_a_block_scalar_are_text() {
        let p = parse("a: |\n  one\n  # two\n\n  three\n# a comment\nb: x\n").unwrap();
        assert_eq!(p["a"], s("one\n# two\n\nthree\n"));
        assert_eq!(p["b"], s("x"));
    }
}
