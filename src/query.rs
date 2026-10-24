//! `plied find`: a filter over records (FORMAT.md §11).
//!
//! Each key is a field (dotted for nested ones), each value a condition; all
//! must hold. Operators: `$eq $ne $in $nin $gt $gte $lt $lte $regex` (with
//! `$options: "i"`) `$exists $contains $size $not`; top level `$and`/`$or`.

use serde_json::Value;

use crate::error::{Fail, Res};

fn qerr(msg: impl Into<String>) -> Fail {
    Fail::query(msg)
}

fn get<'a>(doc: &'a Value, key: &str) -> Option<&'a Value> {
    let mut cur = doc;
    for p in key.split('.') {
        cur = cur.as_object()?.get(p)?;
    }
    Some(cur)
}

fn as_f64(v: &Value) -> Option<f64> {
    v.as_f64()
}

fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "None".into(),
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        other => other.to_string(),
    }
}

fn eq(v: &Value, x: &Value) -> bool {
    if let Value::Array(items) = v
        && !matches!(x, Value::Array(_))
    {
        return items.iter().any(|e| eq(e, x));
    }
    // a number equals a string of digits
    if let (Some(n), Value::String(s)) = (as_f64(v), x)
        && !s.is_empty()
        && s.bytes().all(|b| b.is_ascii_digit())
    {
        return s.parse::<f64>().map(|m| m == n).unwrap_or(false);
    }
    if let (Value::String(s), Some(n)) = (v, as_f64(x))
        && !s.is_empty()
        && s.bytes().all(|b| b.is_ascii_digit())
    {
        return s.parse::<f64>().map(|m| m == n).unwrap_or(false);
    }
    match (v, x) {
        (Value::Number(a), Value::Number(b)) => as_f64(&Value::Number(a.clone())) == as_f64(&Value::Number(b.clone())),
        _ => v == x,
    }
}

fn cmp(v: &Value, x: &Value, op: &str) -> bool {
    let ord = if let (Some(a), Some(b)) = (as_f64(v), as_f64(x)) {
        a.partial_cmp(&b)
    } else {
        Some(text_of(v).cmp(&text_of(x)))
    };
    let Some(ord) = ord else { return false };
    use std::cmp::Ordering::*;
    match op {
        "$gt" => ord == Greater,
        "$gte" => matches!(ord, Greater | Equal),
        "$lt" => ord == Less,
        "$lte" => matches!(ord, Less | Equal),
        _ => false,
    }
}

fn match_value(v: Option<&Value>, cond: &Value) -> Res<bool> {
    let is_ops = cond.as_object().is_some_and(|o| !o.is_empty() && o.keys().all(|k| k.starts_with('$')));
    if !is_ops {
        return Ok(v.is_some_and(|v| eq(v, cond)));
    }
    let ops = cond.as_object().unwrap();
    for (op, arg) in ops {
        match op.as_str() {
            "$options" => {}
            "$eq" => {
                if !v.is_some_and(|v| eq(v, arg)) {
                    return Ok(false);
                }
            }
            "$ne" => {
                if v.is_some_and(|v| eq(v, arg)) {
                    return Ok(false);
                }
            }
            "$in" | "$nin" => {
                let Some(list) = arg.as_array() else { return Err(qerr(format!("{op} takes a list"))) };
                let hit = v.is_some_and(|v| list.iter().any(|a| eq(v, a)));
                if (op == "$in") != hit {
                    return Ok(false);
                }
            }
            "$gt" | "$gte" | "$lt" | "$lte" => {
                let bad = match v {
                    None | Some(Value::Null) => true,
                    Some(v) => !cmp(v, arg, op),
                };
                if bad {
                    return Ok(false);
                }
            }
            "$regex" => {
                let case_i = ops.get("$options").and_then(Value::as_str).is_some_and(|o| o.contains('i'));
                let pat = text_of(arg);
                let pat = if case_i { format!("(?i){pat}") } else { pat };
                let re = regex::Regex::new(&pat).map_err(|e| qerr(format!("$regex {arg}: {e}")))?;
                let hit = match v {
                    Some(Value::Array(items)) => items.iter().any(|x| re.is_match(&text_of(x))),
                    Some(v) => re.is_match(&text_of(v)),
                    None => false,
                };
                if !hit {
                    return Ok(false);
                }
            }
            "$exists" => {
                let exists = v.is_some_and(|v| !matches!(v, Value::Null));
                if exists != truthy_arg(arg) {
                    return Ok(false);
                }
            }
            "$contains" => {
                let hit = matches!(v, Some(Value::Array(items)) if items.iter().any(|e| eq(e, arg)));
                if !hit {
                    return Ok(false);
                }
            }
            "$size" => {
                let hit = matches!(v, Some(Value::Array(items)) if arg.as_u64().is_some_and(|n| items.len() as u64 == n));
                if !hit {
                    return Ok(false);
                }
            }
            "$not" => {
                if match_value(v, arg)? {
                    return Ok(false);
                }
            }
            other => return Err(qerr(format!("unknown operator {other}"))),
        }
    }
    Ok(true)
}

fn truthy_arg(v: &Value) -> bool {
    crate::val::truthy(v)
}

pub fn matches(doc: &Value, q: &Value) -> Res<bool> {
    let Some(obj) = q.as_object() else { return Err(qerr("a query is a JSON object")) };
    for (k, cond) in obj {
        match k.as_str() {
            "$and" | "$or" => {
                let Some(list) = cond.as_array() else { return Err(qerr(format!("{k} takes a list of queries"))) };
                let mut results = Vec::new();
                for sub in list {
                    results.push(matches(doc, sub)?);
                }
                let ok = if k == "$and" { results.iter().all(|r| *r) } else { results.iter().any(|r| *r) };
                if !ok {
                    return Ok(false);
                }
            }
            _ if k.starts_with('$') => return Err(qerr(format!("unknown top-level operator {k}"))),
            _ => {
                if !match_value(get(doc, k), cond)? {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}
