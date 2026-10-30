//! What a session first saw of plied: the binary's version and the store's `requires`. An agent
//! reads `plied skill` once, early, so when another session upgrades plied or raises `requires`
//! under it, its guidance may be stale with nothing failing. Commands and the agent hook compare
//! against this baseline and say once when it changed (FORMAT.md §6, §11).

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::fsx;

fn path(runtime: &Path, session: &str) -> Option<PathBuf> {
    // a session id names one file; one carrying a path separator (or `.`/`..`) would name another
    fsx::single_file_component(session).then(|| runtime.join("seen").join(format!("{session}.json")))
}

/// The baseline recorded for `session`, if any: `{version, requires, at}`. Reads only.
pub fn seen(runtime: &Path, session: &str) -> Option<Value> {
    let text = std::fs::read_to_string(path(runtime, session)?).ok()?;
    serde_json::from_str::<Value>(&text).ok().filter(|v| v.get("version").is_some_and(Value::is_string))
}

/// What changed since `before` was recorded, in words, or None when nothing did.
pub fn change(before: &Value, requires: Option<&str>) -> Option<String> {
    let was = before.get("version").and_then(Value::as_str)?;
    let was_requires = before.get("requires").and_then(Value::as_str);
    let mut parts = vec![];
    if was != crate::VERSION {
        parts.push(format!("plied is now {}, but this session started with {was}", crate::VERSION));
    }
    if was_requires != requires {
        let shown = |r: Option<&str>| r.map_or_else(|| "unset".to_string(), |r| format!("{r:?}"));
        parts.push(format!("the store's requires changed from {} to {}", shown(was_requires), shown(requires)));
    }
    (!parts.is_empty()).then(|| {
        format!(
            "{} since this session first ran plied. Guidance may have changed: re-read `plied skill` before going on, \
             and see the changelog for what changed",
            parts.join("; ")
        )
    })
}

/// Compare with the baseline for `session`, record the current one when it is new or changed, and
/// return the change to say, once. Failing to read or write the baseline is silent: it is a
/// courtesy, and must never stop a command.
pub fn observe(runtime: &Path, session: &str, requires: Option<&str>) -> Option<String> {
    let p = path(runtime, session)?;
    let before = seen(runtime, session);
    let said = before.as_ref().and_then(|b| change(b, requires));
    if before.is_none() || said.is_some() {
        let now = json!({"version": crate::VERSION, "requires": requires, "at": crate::messages::iso()});
        let _ = std::fs::create_dir_all(p.parent().unwrap_or(runtime));
        let _ = fsx::atomic_write(&p, now.to_string().as_bytes());
    }
    said
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_a_change_once() {
        let dir = tempfile::tempdir().unwrap();
        let rt = dir.path();
        assert_eq!(observe(rt, "s1", Some(">=0.1.0")), None, "the first sight is the baseline");
        assert_eq!(observe(rt, "s1", Some(">=0.1.0")), None);
        let msg = observe(rt, "s1", Some(">=0.2.0")).expect("requires changed");
        assert!(msg.contains(r#"from ">=0.1.0" to ">=0.2.0""#) && msg.contains("plied skill"), "{msg}");
        assert_eq!(observe(rt, "s1", Some(">=0.2.0")), None, "said once");
        assert_eq!(observe(rt, "s2", None), None, "sessions are separate");

        std::fs::write(rt.join("seen/s1.json"), r#"{"version": "0.0.1", "requires": ">=0.2.0"}"#).unwrap();
        let msg = observe(rt, "s1", Some(">=0.2.0")).expect("version changed");
        assert!(msg.contains("started with 0.0.1") && !msg.contains("requires"), "{msg}");
        assert_eq!(observe(rt, "../s1", None), None, "not a session plied tracks");
    }
}
