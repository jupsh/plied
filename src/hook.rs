//! Agent hook entry points: renew this session's claims and deliver its unread
//! messages. A hook must never break the agent, so failures are silent
//! (exit 0, no output) unless `--debug` is passed.

use std::io::Read;
use std::path::PathBuf;

use serde_json::{Value, json};

use crate::fsx::{self, FileLock};
use crate::messages::Messages;
use crate::repo;

const CONTEXT_EVENTS: [&str; 3] = ["SessionStart", "UserPromptSubmit", "PostToolUse"];

pub fn claude(stdin_text: &str, debug: bool) -> Option<String> {
    match run(stdin_text) {
        Ok(out) => out,
        Err(e) => {
            if debug {
                crate::errln!("plied hook: {e}");
            }
            None
        }
    }
}

fn run(stdin_text: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let data: Value = serde_json::from_str(if stdin_text.trim().is_empty() { "{}" } else { stdin_text })?;
    // a session id names one state file under `sessions/`; one carrying a path separator
    // (or `.`/`..`) would name some other file, so it is not a session plied tracks
    let Some(session) = data
        .get("session_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && *s != "." && *s != ".." && !s.contains(['/', '\\']))
    else {
        return Ok(None);
    };
    let session = session.to_string();
    let event = data.get("hook_event_name").and_then(Value::as_str).unwrap_or("").to_string();
    let cwd = PathBuf::from(data.get("cwd").and_then(Value::as_str).unwrap_or("."));
    let Some(repo) = repo::locate_fast(&cwd).or_else(|| repo::locate(Some(&cwd), None).ok()) else {
        return Ok(None);
    };
    let lock = FileLock::new(repo.runtime.join("lock"), 3.0);
    let policy = crate::access::Policy::new(&repo.cfg).map_err(|e| e.msg)?;
    let msgs = Messages::new(&repo.cfg, &policy, &repo.runtime, &lock);
    let state_p = repo.runtime.join("sessions").join(format!("{session}.json"));
    let mut state: serde_json::Map<String, Value> = std::fs::read_to_string(&state_p)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    let now = fsx::now_secs();
    let mut renewed = false;
    let last_renew = state.get("renewed").and_then(Value::as_f64).unwrap_or(0.0);
    if now - last_renew > repo.cfg.lease_minutes as f64 * 60.0 / 4.0 {
        renewed = msgs.renew_session(&session).map(|n| n > 0).unwrap_or(false);
        state.insert("renewed".into(), json!(now));
    }
    let sig = msgs.signature();
    if state.get("sig").and_then(Value::as_str) == Some(sig.as_str()) && !renewed {
        save(&state_p, &state);
        return Ok(None);
    }
    if !event.is_empty() && !CONTEXT_EVENTS.contains(&event.as_str()) {
        save(&state_p, &state);
        return Ok(None);
    }
    let pending = msgs.undelivered_for_session(&session);
    let mut out = None;
    if !pending.is_empty() {
        let mut lines = vec![format!("plied: {} new message(s) on nodes you have claimed:", pending.len())];
        for (agent, m) in &pending {
            let get = |k: &str| m.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
            let reply = if m.get("reply_to").and_then(Value::as_str).is_some_and(|r| !r.is_empty()) {
                format!(" (reply to {})", get("reply_to"))
            } else {
                String::new()
            };
            lines.push(format!(
                "- [{}] to {agent} on {}, from {} at {}{reply}:\n  {}",
                get("id"),
                get("node"),
                get("from"),
                get("at"),
                get("text").replace('\n', "\n  ")
            ));
        }
        lines.push("Acknowledge receipt with `plied ack <id>` (receipt only, not approval); reply with `plied send <node> --reply-to <id> \"…\"`.".into());
        let mut agents: Vec<String> = pending.iter().map(|(a, _)| a.clone()).collect();
        agents.sort();
        agents.dedup();
        for agent in agents {
            let ids: Vec<String> = pending
                .iter()
                .filter(|(a, _)| *a == agent)
                .map(|(_, m)| m.get("id").and_then(Value::as_str).unwrap_or_default().to_string())
                .collect();
            let _ = msgs.mark("delivered", &ids, &agent, Some(&session), Some(&format!("hook:{event}")));
        }
        out = Some(
            json!({"hookSpecificOutput": {"hookEventName": if event.is_empty() { "UserPromptSubmit".into() } else { event.clone() }, "additionalContext": lines.join("\n")}})
                .to_string(),
        );
    }
    state.insert("sig".into(), json!(msgs.signature()));
    save(&state_p, &state);
    Ok(out)
}

fn save(p: &std::path::Path, state: &serde_json::Map<String, Value>) {
    let _ = fsx::atomic_write(p, Value::Object(state.clone()).to_string().as_bytes());
}

pub fn main_claude(debug: bool) -> i32 {
    let mut text = String::new();
    let _ = std::io::stdin().read_to_string(&mut text);
    if let Some(out) = claude(&text, debug) {
        crate::outln!("{out}");
    }
    0
}
