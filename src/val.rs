//! Small helpers over `serde_json::Value` for message events (FORMAT.md §5).
//! JSON objects are the event maps; truthiness and text conversion follow the
//! usual dynamic-language reading so hand-written logs behave predictably.

use serde_json::Value;

pub type Meta = serde_json::Map<String, Value>;

/// Python-style truthiness: null, false, 0, "" and empty containers are false.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

pub fn truthy_opt(v: Option<&Value>) -> bool {
    v.is_some_and(truthy)
}

/// `str(value)` for display in messages.
pub fn py_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "None".into(),
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        Value::Number(n) => n.to_string(),
        other => dumps(other, LINE),
    }
}

/// Text of an optional value; missing/null reads as empty.
pub fn str_opt(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => py_str(other),
    }
}

/// Items of a list value; anything else has none.
pub fn list_items(v: Option<&Value>) -> Vec<Value> {
    match v {
        Some(Value::Array(a)) => a.clone(),
        _ => vec![],
    }
}

pub fn s(v: impl Into<String>) -> Value {
    Value::String(v.into())
}

#[derive(Clone, Copy)]
pub struct Dump {
    pub indent: Option<usize>,
}

pub const DEFAULT: Dump = Dump { indent: None };
/// One compact line, for JSON-lines logs.
pub const LINE: Dump = Dump { indent: None };

pub fn dumps(v: &Value, style: Dump) -> String {
    match style.indent {
        Some(_) => serde_json::to_string_pretty(v).unwrap_or_default(),
        None => serde_json::to_string(v).unwrap_or_default(),
    }
}

pub fn splitlines(text: &str) -> Vec<&str> {
    text.lines().collect()
}

pub fn strip(s: &str) -> &str {
    s.trim()
}
