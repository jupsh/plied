//! Messages addressed to nodes, and presence (expiring claims).
//!
//! Messages are durable and tracked: one append-only JSON-lines log per node,
//! `<root>/messages/<node>.jsonl`, holding message, delivery, read, acknowledgement and
//! subscription events. Every event has a unique id chosen by the writer (pass the same id to retry
//! safely; reusing it for different content is refused). Appends happen under the store lock; a torn
//! last line left by an interrupted write is cut before the next append, and an event whose id is
//! already present is skipped. A sealed log (FORMAT.md §9) is neither read nor appended to.
//!
//! Claims are presence, not record: they live in the untracked runtime directory and expire unless
//! renewed. A lease is an estimate that an agent is on a node, not proof that it is running.
//! Acknowledging a message means it was received, never that anything in it is approved.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde_json::{Map, Value, json};

use crate::access::Policy;
use crate::config::Config;
use crate::error::{EXIT_CONFLICT, Fail, IoContext, Res};
use crate::fsx::{self, FileLock, atomic_write};
use crate::val::{self, Meta, truthy, truthy_opt};

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
pub const EVENT_TYPES: [&str; 6] = ["message", "delivered", "read", "ack", "subscribe", "unsubscribe"];

pub fn ulid() -> String {
    let ms = Utc::now().timestamp_millis() as u128;
    let r = fsx::random_bytes::<10>();
    let rand = r.iter().fold(0u128, |a, b| (a << 8) | *b as u128);
    let n = (ms << 80) | rand;
    (0..26).rev().map(|i| CROCKFORD[((n >> (5 * i)) & 31) as usize] as char).collect()
}

pub fn iso_at(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn iso() -> String {
    iso_at(Utc::now())
}

pub fn parse_iso(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
}

/// `str(datetime)` of an aware UTC time.
pub fn py_datetime(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M:%S+00:00").to_string()
}

#[derive(Clone)]
pub struct Claim {
    pub node: String,
    pub agent: String,
    pub session: Option<String>,
    pub since: String,
    pub renewed: String,
    pub ttl_min: i64,
}

impl Claim {
    fn from_json(v: &Value) -> Option<Claim> {
        let o = v.as_object()?;
        let keys: BTreeSet<&str> = o.keys().map(String::as_str).collect();
        if keys != BTreeSet::from(["node", "agent", "session", "since", "renewed", "ttl_min"]) {
            return None;
        }
        let c = Claim {
            node: o["node"].as_str()?.into(),
            agent: o["agent"].as_str()?.into(),
            session: match &o["session"] {
                Value::Null => None,
                Value::String(s) => Some(s.clone()),
                _ => return None,
            },
            since: o["since"].as_str()?.into(),
            renewed: o["renewed"].as_str()?.into(),
            ttl_min: o["ttl_min"].as_i64()?,
        };
        parse_iso(&c.renewed)?;
        Some(c)
    }

    pub fn to_json(&self) -> Value {
        json!({"node": self.node, "agent": self.agent, "session": self.session, "since": self.since,
               "renewed": self.renewed, "ttl_min": self.ttl_min})
    }

    pub fn expires(&self) -> DateTime<Utc> {
        parse_iso(&self.renewed).unwrap_or_default() + Duration::minutes(self.ttl_min)
    }

    pub fn active(&self) -> bool {
        self.expires() > Utc::now()
    }
}

pub struct Messages<'a> {
    cfg: &'a Config,
    policy: &'a Policy,
    runtime: PathBuf,
    lock: &'a FileLock,
    pub dir: PathBuf,
}

fn s<'v>(e: &'v Meta, k: &str) -> &'v str {
    e.get(k).and_then(Value::as_str).unwrap_or("")
}

/// The fields of an event that are missing or of the wrong type (§5). Unknown fields are allowed.
fn misshapen(e: &Meta) -> Vec<&'static str> {
    enum T {
        Text,
        TextOrNull,
        Time,
        Texts,
        List,
    }
    let fields: &[(&str, T)] = match s(e, "type") {
        "message" => &[
            ("node", T::Text),
            ("at", T::Time),
            ("from", T::Text),
            ("session", T::TextOrNull),
            ("to", T::Texts),
            ("present", T::List),
            ("reply_to", T::TextOrNull),
            ("text", T::Text),
        ],
        "delivered" | "read" | "ack" => &[("msg", T::Text), ("agent", T::Text), ("session", T::TextOrNull), ("at", T::Time)],
        _ => &[("node", T::Text), ("agent", T::Text), ("at", T::Time)],
    };
    fields
        .iter()
        .filter(|(k, t)| {
            let v = e.get(*k);
            !match t {
                T::Text => v.is_some_and(Value::is_string),
                T::TextOrNull => v.is_some_and(|v| v.is_string() || v.is_null()),
                T::Time => v.and_then(Value::as_str).and_then(parse_iso).is_some(),
                T::Texts => v.and_then(Value::as_array).is_some_and(|a| a.iter().all(Value::is_string)),
                T::List => v.is_some_and(Value::is_array),
            }
        })
        .map(|(k, _)| *k)
        .collect()
}

fn id_key(v: Option<&Value>) -> String {
    v.map(|x| val::dumps(x, val::LINE)).unwrap_or_default()
}

impl<'a> Messages<'a> {
    pub fn new(cfg: &'a Config, policy: &'a Policy, runtime: &Path, lock: &'a FileLock) -> Self {
        Messages { cfg, policy, runtime: runtime.to_path_buf(), lock, dir: cfg.messages_dir() }
    }

    // ---------------------------------------------------------------------------- logs

    pub fn path(&self, node: &str) -> PathBuf {
        self.dir.join(format!("{node}.jsonl"))
    }

    /// The log for `node`, when `node` names one file inside the messages directory. A node is a
    /// record id, never a path: anything with a separator in it (or `.`/`..`) is not a log plied
    /// may read or append to, however it arrived (a command argument, a crafted event's `node`).
    fn log_path(&self, node: &str) -> Option<PathBuf> {
        if node.is_empty() || node == "." || node == ".." || node.contains(['/', '\\']) {
            return None;
        }
        Some(self.path(node))
    }

    pub fn events(&self, node: &str) -> Vec<Meta> {
        let Some(p) = self.log_path(node) else { return vec![] };
        if !self.policy.readable(&p) {
            return vec![];
        }
        let Ok(text) = fsx::read_text(&p) else { return vec![] };
        let mut out = vec![];
        for line in val::splitlines(&text) {
            if val::strip(line).is_empty() {
                continue;
            }
            // a torn last line, or anything that is not an event: verify() reports it
            if let Ok(Value::Object(e)) = serde_json::from_str::<Value>(line)
                && e.get("v") == Some(&json!(1))
                && e.get("type").and_then(Value::as_str).is_some_and(|t| EVENT_TYPES.contains(&t))
                && truthy_opt(e.get("id"))
            {
                out.push(e);
            }
        }
        out
    }

    pub fn nodes(&self) -> Vec<String> {
        let mut out: Vec<String> = std::fs::read_dir(&self.dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter_map(|n| n.strip_suffix(".jsonl").map(String::from))
                    .filter(|n| self.policy.readable(&self.path(n)))
                    .collect()
            })
            .unwrap_or_default();
        out.sort();
        out
    }

    pub fn all_events(&self) -> Vec<Meta> {
        self.nodes().iter().flat_map(|n| self.events(n)).collect()
    }

    /// Append events to a node's log; returns those actually written (duplicates skipped).
    pub fn append(&self, node: &str, events: Vec<Meta>) -> Res<Vec<Meta>> {
        let _g = self.lock.lock()?;
        let Some(p) = self.log_path(node) else {
            return Err(Fail::policy(format!("{node:?} is not a node id")));
        };
        self.policy.check_readable(&p)?; // appending means reading the log first
        std::fs::create_dir_all(&self.dir).at(&self.dir)?;
        let mut data = if p.exists() { std::fs::read(&p).at(&p)? } else { vec![] };
        if !data.is_empty() && !data.ends_with(b"\n") {
            let keep = data.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
            data.truncate(keep); // cut a torn last line
            fsx::remember(&self.runtime, &data, true)?; // before the write, so the guard never sees it unlisted
            atomic_write(&p, &data)?;
        }
        let text = String::from_utf8(data).map_err(|e| Fail::new(format!("{}: {e}", p.display())))?;
        let mut seen = HashSet::new();
        for line in val::splitlines(&text) {
            if let Ok(Value::Object(o)) = serde_json::from_str::<Value>(line)
                && let Some(id) = o.get("id")
            {
                seen.insert(id_key(Some(id)));
            }
        }
        let fresh: Vec<Meta> = events.into_iter().filter(|e| !seen.contains(&id_key(e.get("id")))).collect();
        if !fresh.is_empty() {
            let lines: String = fresh.iter().map(|e| val::dumps(&Value::Object(e.clone()), val::LINE) + "\n").collect();
            fsx::remember(&self.runtime, (text + &lines).as_bytes(), true)?; // the log as it will be, listed first
            let mut fh = std::fs::OpenOptions::new().create(true).append(true).open(&p).at(&p)?;
            fh.write_all(lines.as_bytes()).at(&p)?;
            fh.sync_all().at(&p)?;
        }
        Ok(fresh)
    }

    pub fn verify(&self) -> Vec<(String, String)> {
        let mut problems = vec![];
        let mut ids: HashMap<String, String> = HashMap::new();
        for n in self.nodes() {
            let p = self.path(&n);
            let where_ = format!("{}/messages/{}", self.cfg.root, fsx::file_name(&p));
            let Ok(raw) = std::fs::read(&p) else { continue };
            if !raw.is_empty() && !raw.ends_with(b"\n") {
                problems.push((where_.clone(), "torn last line (an interrupted write); the next append repairs it".to_string()));
            }
            let n_raw_lines = raw.split(|&b| b == b'\n').count() - usize::from(raw.ends_with(b"\n") || raw.is_empty());
            let text = String::from_utf8_lossy(&raw);
            let mut msgs = HashSet::new();
            for (i, line) in val::splitlines(&text).into_iter().enumerate() {
                let i = i + 1;
                if val::strip(line).is_empty() {
                    continue;
                }
                let e = match serde_json::from_str::<Value>(line) {
                    Ok(e) => e,
                    Err(_) => {
                        if i < n_raw_lines || raw.ends_with(b"\n") {
                            problems.push((format!("{where_}:{i}"), "not valid JSON".to_string()));
                        }
                        continue;
                    }
                };
                let ok = e.as_object().filter(|o| {
                    o.get("type").and_then(Value::as_str).is_some_and(|t| EVENT_TYPES.contains(&t)) && truthy_opt(o.get("id"))
                });
                let Some(e) = ok else {
                    problems.push((format!("{where_}:{i}"), "not a plied event (needs id and a known type)".to_string()));
                    continue;
                };
                if e.get("v") != Some(&json!(1)) {
                    problems.push((format!("{where_}:{i}"), format!("event version {} is not supported (only 1 is); readers skip it", val::str_opt(e.get("v")))));
                    continue;
                }
                let bad = misshapen(e);
                if !bad.is_empty() {
                    problems.push((format!("{where_}:{i}"), format!("{} event with missing or mistyped {}", s(e, "type"), bad.join(", "))));
                }
                let id = val::py_str(&e["id"]);
                let key = id_key(e.get("id"));
                if let Some(prev) = ids.get(&key) {
                    problems.push((format!("{where_}:{i}"), format!("event id {id} repeats {prev}")));
                }
                ids.insert(key.clone(), format!("{where_}:{i}"));
                let ty = s(e, "type");
                if ty == "message" {
                    msgs.insert(key);
                    if e.get("node").and_then(Value::as_str) != Some(n.as_str()) {
                        problems.push((format!("{where_}:{i}"), format!("message for {} filed under {n}", val::str_opt(e.get("node")))));
                    }
                } else if matches!(ty, "delivered" | "read" | "ack") && !msgs.contains(&id_key(e.get("msg"))) {
                    problems.push((format!("{where_}:{i}"), format!("{ty} of unknown message {}", val::str_opt(e.get("msg")))));
                }
            }
        }
        problems
    }

    // -------------------------------------------------------------------------- claims

    fn claims_path(&self) -> PathBuf {
        self.runtime.join("claims.json")
    }

    pub fn claims(&self, include_expired: bool) -> Vec<Claim> {
        let Ok(text) = std::fs::read_to_string(self.claims_path()) else { return vec![] };
        let Ok(data) = serde_json::from_str::<Value>(&text) else { return vec![] };
        // anything that is not a claim plied wrote is skipped, and dropped at the next save
        let out: Vec<Claim> = val::list_items(data.get("claims")).iter().filter_map(Claim::from_json).collect();
        if include_expired { out } else { out.into_iter().filter(Claim::active).collect() }
    }

    fn save_claims(&self, claims: &[Claim]) -> Res<()> {
        let cut = Utc::now() - Duration::days(1);
        let keep: Vec<Value> = claims.iter().filter(|c| c.expires() > cut).map(Claim::to_json).collect();
        let text = val::dumps(&json!({"claims": keep}), val::Dump { indent: Some(1) });
        atomic_write(&self.claims_path(), text.as_bytes())
    }

    pub fn claim(&self, node: &str, agent: &str, session: Option<&str>, ttl_min: Option<i64>) -> Res<Claim> {
        if ttl_min.is_some_and(|t| t < 0) {
            return Err(Fail::code(crate::error::EXIT_USAGE, "claim --ttl must be a positive number of minutes (0 or absent uses messages.lease_minutes)"));
        }
        let ttl = ttl_min.filter(|t| *t != 0).unwrap_or(self.cfg.lease_minutes);
        let _g = self.lock.lock()?;
        let mut cs = self.claims(true);
        let now = iso();
        for c in cs.iter_mut() {
            if c.node == node && c.agent == agent && c.session.as_deref() == session {
                if !c.active() {
                    c.since = now.clone();
                }
                c.renewed = now.clone();
                c.ttl_min = ttl;
                let out = c.clone();
                self.save_claims(&cs)?;
                return Ok(out);
            }
        }
        let c = Claim { node: node.into(), agent: agent.into(), session: session.map(String::from), since: now.clone(), renewed: now, ttl_min: ttl };
        cs.push(c.clone());
        self.save_claims(&cs)?;
        Ok(c)
    }

    pub fn release(&self, node: Option<&str>, agent: &str, session: Option<&str>) -> Res<usize> {
        let _g = self.lock.lock()?;
        let cs = self.claims(true);
        let keep: Vec<Claim> = cs
            .iter()
            .filter(|c| !(c.agent == agent && node.is_none_or(|n| c.node == n) && session.is_none_or(|s| c.session.as_deref() == Some(s))))
            .cloned()
            .collect();
        self.save_claims(&keep)?;
        Ok(cs.len() - keep.len())
    }

    pub fn renew_session(&self, session: &str) -> Res<usize> {
        let _g = self.lock.lock()?;
        let mut cs = self.claims(true);
        let now = iso();
        let mut n = 0;
        for c in cs.iter_mut() {
            if c.session.as_deref() == Some(session) && c.active() {
                c.renewed = now.clone();
                n += 1;
            }
        }
        if n > 0 {
            self.save_claims(&cs)?;
        }
        Ok(n)
    }

    // ----------------------------------------------------------------------- messaging

    pub fn subscribers(&self, node: &str) -> BTreeSet<String> {
        let mut subs: HashMap<String, bool> = HashMap::new();
        for e in self.events(node) {
            let ty = s(&e, "type");
            if ty == "subscribe" || ty == "unsubscribe" {
                subs.insert(val::str_opt(e.get("agent")), ty == "subscribe");
            }
        }
        subs.into_iter().filter(|(_, on)| *on).map(|(a, _)| a).collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn send(
        &self,
        node: &str,
        text: &str,
        sender: &str,
        session: Option<&str>,
        to: &[String],
        reply_to: Option<&str>,
        mid: Option<&str>,
    ) -> Res<(Meta, bool)> {
        let _g = self.lock.lock()?;
        let present: Vec<Claim> = self.claims(false).into_iter().filter(|c| c.node == node).collect();
        let mut recipients: BTreeSet<String> = to.iter().cloned().collect();
        recipients.extend(present.iter().map(|c| c.agent.clone()));
        recipients.extend(self.subscribers(node));
        recipients.remove(sender);
        if let Some(rt) = reply_to
            && !self.all_events().iter().any(|e| e.get("id").and_then(Value::as_str) == Some(rt) && s(e, "type") == "message")
        {
            return Err(Fail::new(format!("reply_to {rt} is not a message")));
        }
        if let Some(mid) = mid
            && let Some(first) = self.all_events().into_iter().find(|e| e.get("id").and_then(Value::as_str) == Some(mid))
        {
            // a retry repeats what the caller chose; `at`, `present` and the claim-derived recipients
            // may differ between attempts and are not compared
            let recorded_to = val::list_items(first.get("to"));
            let recorded: BTreeSet<&str> = recorded_to.iter().filter_map(Value::as_str).collect();
            let same = s(&first, "type") == "message"
                && s(&first, "node") == node
                && s(&first, "from") == sender
                && s(&first, "text") == text
                && first.get("reply_to").and_then(Value::as_str) == reply_to
                && to.iter().all(|t| t == sender || recorded.contains(t.as_str()));
            if !same {
                return Err(Fail::code(
                    EXIT_CONFLICT,
                    format!("event id {mid} is already recorded (on {}) for something else; a retry must repeat the original, and a new message needs a new id", s(&first, "node")),
                ));
            }
            return Ok((first.clone(), false));
        }
        let id = mid.map(String::from).unwrap_or_else(ulid);
        let e = json!({
            "v": 1, "id": id, "type": "message", "node": node, "at": iso(), "from": sender, "session": session,
            "to": recipients.into_iter().collect::<Vec<_>>(),
            "present": present.iter().map(|c| json!({"agent": c.agent, "session": c.session})).collect::<Vec<_>>(),
            "reply_to": reply_to, "text": text,
        });
        let Value::Object(e) = e else { unreachable!() };
        let written = self.append(node, vec![e.clone()])?;
        if written.is_empty() {
            // a retry: return what was recorded the first time
            let first = self.events(node).into_iter().find(|x| x.get("id").and_then(Value::as_str) == Some(id.as_str()));
            return Ok((first.unwrap_or(e), false));
        }
        Ok((e, true))
    }

    /// Record delivered / read / ack events (idempotent per message, agent and session).
    pub fn mark(&self, kind: &str, msg_ids: &[String], agent: &str, session: Option<&str>, via: Option<&str>) -> Res<usize> {
        let _g = self.lock.lock()?;
        let all = self.all_events();
        let index: HashMap<&str, &Meta> = all.iter().filter(|e| s(e, "type") == "message").map(|e| (s(e, "id"), e)).collect();
        let sess_key = |sess: Option<&Value>| if kind == "delivered" { id_key(sess) } else { String::new() };
        let done: HashSet<(String, String, String)> = all
            .iter()
            .filter(|e| s(e, "type") == kind)
            .map(|e| (s(e, "msg").to_string(), s(e, "agent").to_string(), sess_key(Some(e.get("session").unwrap_or(&Value::Null)))))
            .collect();
        let session_v = session.map_or(Value::Null, val::s);
        let mut by_node: Vec<(String, Vec<Meta>)> = vec![];
        for mid in msg_ids {
            let Some(m) = index.get(mid.as_str()) else { return Err(Fail::new(format!("no message {mid}"))) };
            if done.contains(&(mid.clone(), agent.to_string(), sess_key(Some(&session_v)))) {
                continue;
            }
            let mut ev = Map::new();
            ev.insert("v".into(), json!(1));
            ev.insert("id".into(), val::s(ulid()));
            ev.insert("type".into(), val::s(kind));
            ev.insert("msg".into(), val::s(mid.as_str()));
            ev.insert("agent".into(), val::s(agent));
            ev.insert("session".into(), session_v.clone());
            ev.insert("at".into(), val::s(iso()));
            if let Some(v) = via {
                ev.insert("via".into(), val::s(v));
            }
            let node = s(m, "node").to_string();
            match by_node.iter_mut().find(|(n, _)| *n == node) {
                Some((_, evs)) => evs.push(ev),
                None => by_node.push((node, vec![ev])),
            }
        }
        let mut n = 0;
        for (node, evs) in by_node {
            n += self.append(&node, evs)?.len();
        }
        Ok(n)
    }

    pub fn inbox(&self, agent: &str, include_acked: bool) -> Vec<Meta> {
        let evs = self.all_events();
        let mut state: HashMap<String, HashMap<String, Value>> = HashMap::new();
        for e in &evs {
            let ty = s(e, "type");
            if matches!(ty, "delivered" | "read" | "ack") && e.get("agent").and_then(Value::as_str) == Some(agent) {
                state.entry(s(e, "msg").to_string()).or_default().insert(ty.to_string(), e.get("at").cloned().unwrap_or(Value::Null));
            }
        }
        let mut out = vec![];
        for e in &evs {
            if s(e, "type") == "message" && val::list_items(e.get("to")).iter().any(|t| t.as_str() == Some(agent)) {
                let st = state.get(s(e, "id")).cloned().unwrap_or_default();
                if st.get("ack").is_some_and(truthy) && !include_acked {
                    continue;
                }
                let mut item = e.clone();
                for (k, from) in [("delivered", "delivered"), ("read", "read"), ("acked", "ack")] {
                    item.insert(k.into(), st.get(from).cloned().unwrap_or(Value::Null));
                }
                out.push(item);
            }
        }
        out.sort_by(|a, b| s(a, "at").cmp(s(b, "at")));
        out
    }

    pub fn undelivered_for_session(&self, session: &str) -> Vec<(String, Meta)> {
        let agents: BTreeSet<String> = self.claims(false).into_iter().filter(|c| c.session.as_deref() == Some(session)).map(|c| c.agent).collect();
        if agents.is_empty() {
            return vec![];
        }
        let evs = self.all_events();
        let delivered: HashSet<(&str, &str)> = evs
            .iter()
            .filter(|e| s(e, "type") == "delivered" && e.get("session").and_then(Value::as_str) == Some(session))
            .map(|e| (s(e, "msg"), s(e, "agent")))
            .collect();
        let acked: HashSet<(&str, &str)> = evs.iter().filter(|e| s(e, "type") == "ack").map(|e| (s(e, "msg"), s(e, "agent"))).collect();
        let mut out = vec![];
        for e in &evs {
            if s(e, "type") != "message" {
                continue;
            }
            let to_items = val::list_items(e.get("to"));
            let to: BTreeSet<&str> = to_items.iter().filter_map(Value::as_str).collect();
            for a in agents.iter().filter(|a| to.contains(a.as_str())) {
                let key = (s(e, "id"), a.as_str());
                if !delivered.contains(&key) && !acked.contains(&key) {
                    out.push((a.clone(), e.clone()));
                }
            }
        }
        out
    }

    pub fn signature(&self) -> String {
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return "none".into() };
        let mut parts: Vec<String> = rd
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".jsonl"))
            .filter_map(|e| {
                let md = e.metadata().ok()?;
                Some(format!("{}:{}:{}", e.file_name().to_string_lossy(), md.len(), fsx::mtime_ns(&md)))
            })
            .collect();
        if let Ok(md) = std::fs::metadata(self.claims_path()) {
            parts.push(format!("claims:{}", fsx::mtime_ns(&md)));
        }
        parts.sort();
        parts.join("|")
    }
}

