//! The frontmatter data model (FORMAT.md §3.1): strings, integers, lists and mappings, in order.
//! Every scalar parses as a string; only the integer fields become numbers (§3.1).

use std::fmt;

use indexmap::IndexMap;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    Null,
    Str(String),
    Int(i64),
    List(Vec<Node>),
    Map(Map),
}

pub type Map = IndexMap<String, Node>;

impl Node {
    pub fn str(&self) -> Option<&str> {
        match self {
            Node::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn int(&self) -> Option<i64> {
        match self {
            Node::Int(n) => Some(*n),
            _ => None,
        }
    }

    pub fn map(&self) -> Option<&Map> {
        match self {
            Node::Map(m) => Some(m),
            _ => None,
        }
    }

    /// The items of a list; nothing for anything else.
    pub fn items(&self) -> &[Node] {
        match self {
            Node::List(v) => v,
            _ => &[],
        }
    }

    /// Null, `""`, `[]` or `{}`: a field with such a value is not set.
    pub fn is_empty(&self) -> bool {
        match self {
            Node::Null => true,
            Node::Str(s) => s.is_empty(),
            Node::Int(_) => false,
            Node::List(v) => v.is_empty(),
            Node::Map(m) => m.is_empty(),
        }
    }

    /// The text of a scalar.
    pub fn scalar(&self) -> Option<String> {
        match self {
            Node::Str(s) => Some(s.clone()),
            Node::Int(n) => Some(n.to_string()),
            _ => None,
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            Node::Null => Value::Null,
            Node::Str(s) => Value::String(s.clone()),
            Node::Int(n) => Value::from(*n),
            Node::List(v) => Value::Array(v.iter().map(Node::to_json).collect()),
            Node::Map(m) => Value::Object(map_to_json(m)),
        }
    }
}

pub fn map_to_json(m: &Map) -> serde_json::Map<String, Value> {
    m.iter().map(|(k, v)| (k.clone(), v.to_json())).collect()
}

/// Scalars as their text, collections as JSON: for messages and plain-text output.
impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Node::Null => Ok(()),
            Node::Str(s) => f.write_str(s),
            Node::Int(n) => write!(f, "{n}"),
            other => write!(f, "{}", other.to_json()),
        }
    }
}

impl From<&str> for Node {
    fn from(s: &str) -> Self {
        Node::Str(s.to_string())
    }
}

impl From<String> for Node {
    fn from(s: String) -> Self {
        Node::Str(s)
    }
}

/// A field by dotted path (`approval.by`).
pub fn get<'a>(m: &'a Map, dotted: &str) -> Option<&'a Node> {
    let mut parts = dotted.split('.');
    let mut cur = m.get(parts.next()?)?;
    for p in parts {
        cur = cur.map()?.get(p)?;
    }
    Some(cur)
}

/// A field that is present and not empty.
pub fn set<'a>(m: &'a Map, key: &str) -> Option<&'a Node> {
    m.get(key).filter(|v| !v.is_empty())
}

/// The text of a set scalar field.
pub fn text(m: &Map, key: &str) -> Option<String> {
    set(m, key).and_then(Node::scalar)
}

/// A string field's value, when it is a string.
pub fn str_of<'a>(m: &'a Map, key: &str) -> Option<&'a str> {
    m.get(key).and_then(Node::str)
}

// ------------------------------------------------------------------------------- text

/// The first `n` characters.
pub fn take(s: &str, n: usize) -> &str {
    match s.char_indices().nth(n) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

pub fn clen(s: &str) -> usize {
    s.chars().count()
}

/// Runs of whitespace collapsed to one space, ends trimmed.
pub fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
