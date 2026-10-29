//! Commitment review before a consequential action (a run, a freeze, closing work).
//!
//! A commitment applies to an affected node when it is attached to the node directly, to anything
//! the node reaches through `waiting_on` or `uses`, to an ancestor when the commitment says
//! `applies: subtree`, or everywhere when it says `applies: project`. `parent` alone never carries
//! an obligation. Each applicable commitment is shown with the chain by which it applies; recording
//! a disposition for the action is a checked write to the commitment. A commitment that names the
//! action kinds it `governs` still applies, but the gate leaves it out for any other kind. Structure alone cannot tell
//! whether a scientific trigger has occurred: that judgement is the reviewer's, recorded with its
//! reason.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use serde::Serialize;

use crate::doc::{self, Map, Node};
use crate::records::{Applies, CommitmentState, Disposition, OwnKind, Record, Store, node_ref};

/// Exact bytes explicitly read for a disposition; these are not graph references.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadInput {
    pub record: String,
    pub rev: String,
}

pub fn full_revision(rev: &str) -> bool {
    rev.len() == 64 && crate::records::is_rev(rev)
}

/// A configured id, with no locator or source-file path.
pub fn input_id(store: &Store, id: &str) -> bool {
    !id.is_empty() && crate::records::ref_id(id) == id && !id.chars().any(char::is_whitespace) &&
        (store.cfg.kinds.iter().any(|k| k.id_re.is_match(id)) || store.cfg.sources.iter().any(|s| s.id_re.is_match_start(id)))
}

impl ReadInput {
    pub fn parse(value: &str) -> crate::error::Res<Self> {
        let Some((record, rev)) = value.rsplit_once('@').filter(|(record, rev)| !record.is_empty() && full_revision(rev)) else {
            return Err(crate::error::Fail::code(crate::error::EXIT_USAGE, "--read needs ID@REV, with the full 64-character lowercase SHA-256 from the read you judged"));
        };
        Ok(Self { record: record.into(), rev: rev.into() })
    }

    pub fn from_map(input: &Map) -> Option<Self> {
        Some(Self { record: doc::str_of(input, "record")?.into(), rev: doc::str_of(input, "rev")?.into() })
    }

    pub fn node(&self) -> Node {
        Node::Map(Map::from_iter([("record".into(), Node::Str(self.record.clone())), ("rev".into(), Node::Str(self.rev.clone()))]))
    }

    /// Called only against the store loaded under the transaction lock. No cache or sealed path is read.
    pub fn check(&self, store: &Store) -> crate::error::Res<()> {
        let current = store.get(&self.record);
        if current.is_some_and(|r| r.rev == self.rev) {
            return Ok(());
        }
        let message = if current.is_some() {
            format!("{} changed since the review input was read; read it again", self.record)
        } else {
            format!("{} is unavailable (missing or possibly sealed); the review input cannot be verified", self.record)
        };
        Err(crate::error::Fail::code(crate::error::EXIT_CONFLICT, message).with_detail(serde_json::json!({
            "id": self.record, "expected": self.rev, "current": current.map(|r| &r.rev)
        })))
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct InputStatus {
    pub record: String,
    pub rev: String,
    pub status: &'static str,
    pub current: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct InputReview {
    pub action: String,
    pub review_index: usize,
    /// `unbound`, `current`, `changed`, `unavailable` or `invalid`; never a gate verdict.
    pub status: &'static str,
    pub inputs: Vec<InputStatus>,
}

/// Evidence claims for the last disposition in list order for each exact action.
pub fn input_reviews(store: &Store, record: &Record, action: Option<&str>) -> Vec<InputReview> {
    let mut latest = std::collections::BTreeMap::new();
    for (index, rv) in record.meta.get("reviews").map(Node::items).unwrap_or_default().iter().enumerate() {
        if let Some(rv) = rv.map() && let Some(name) = doc::str_of(rv, "action").filter(|name| action.is_none_or(|a| *name == a)) {
            latest.insert(name, (index, rv));
        }
    }
    latest.into_iter().map(|(action, (review_index, rv))| {
        let mut inputs = vec![];
        let mut invalid = false;
        let mut seen = HashMap::new();
        if let Some(values) = doc::set(rv, "inputs") {
            invalid = !matches!(values, Node::List(_));
            for value in values.items() {
                let Some(input) = value.map().and_then(ReadInput::from_map) else {
                    invalid = true;
                    inputs.push(InputStatus { record: String::new(), rev: String::new(), status: "invalid", current: None });
                    continue;
                };
                invalid |= value.map().is_some_and(|m| m.keys().any(|key| !matches!(key.as_str(), "record" | "rev")));
                if let Some(previous) = seen.insert(input.record.clone(), input.rev.clone()) { invalid |= previous != input.rev; }
                let current = store.get(&input.record).map(|r| r.rev.clone());
                let status = if !full_revision(&input.rev) || !input_id(store, &input.record) || input.record == record.id { "invalid" }
                    else if current.is_none() { "unavailable" } else if current.as_deref() == Some(input.rev.as_str()) { "current" } else { "changed" };
                inputs.push(InputStatus { record: input.record, rev: input.rev, status, current });
            }
        }
        let status = if invalid || inputs.iter().any(|i| i.status == "invalid") { "invalid" }
            else if inputs.is_empty() { "unbound" }
            else if inputs.iter().any(|i| i.status == "unavailable") { "unavailable" }
            else if inputs.iter().any(|i| i.status == "changed") { "changed" } else { "current" };
        InputReview { action: action.into(), review_index, status, inputs }
    }).collect()
}

pub struct Applicable<'s> {
    pub commitment: &'s Record,
    pub chains: Vec<String>,
    /// existing reviews (for the action, when one is given)
    pub reviews: Vec<&'s Map>,
    /// whether the commitment governs the action (always, when none is given): the gate leaves out
    /// one that does not, and `review` still lists it
    pub governs_action: bool,
}

/// Nodes reachable from `start` through waiting_on/uses, with the shortest chain to each, in order found.
fn deps(store: &Store, start: &str) -> Vec<(String, Vec<String>)> {
    let mut chains: Vec<(String, Vec<String>)> = vec![(start.to_string(), vec![start.to_string()])];
    let mut seen: HashMap<String, usize> = HashMap::from([(start.to_string(), 0)]);
    let mut q = VecDeque::from([start.to_string()]);
    while let Some(n) = q.pop_front() {
        let Some(r) = store.get(&n).filter(|r| r.is_own()) else { continue };
        for f in ["waiting_on", "uses"] {
            for m in r.ref_list(f) {
                if !seen.contains_key(&m) {
                    let mut path = chains[seen[&n]].1.clone();
                    path.extend([f.to_string(), m.clone()]);
                    seen.insert(m.clone(), chains.len());
                    chains.push((m.clone(), path));
                    q.push_back(m);
                }
            }
        }
    }
    chains
}

fn ancestors(store: &Store, start: &str) -> Vec<String> {
    let (mut out, mut cur, mut seen) = (vec![], store.get(start), HashSet::from([start.to_string()]));
    while let Some(parent) = cur.filter(|r| r.is_own()).and_then(|r| doc::set(&r.meta, "parent")) {
        let p = node_ref(parent);
        if !seen.insert(p.clone()) {
            break;
        }
        out.push(p.clone());
        cur = store.get(&p);
    }
    out
}

/// The records each commitment is attached to: those its `nodes` names, and those whose `links` name it.
pub fn attachments(store: &Store) -> HashMap<String, HashSet<String>> {
    let mut attached: HashMap<String, HashSet<String>> =
        store.own(Some(OwnKind::Commitment)).iter().map(|c| (c.id.clone(), c.ref_list("nodes").into_iter().collect())).collect();
    for r in store.own(None) {
        for x in r.ref_list("links") {
            if let Some(set) = attached.get_mut(x.as_str()) {
                set.insert(r.id.clone());
            }
        }
    }
    attached
}

type NodeWalk = (String, Vec<(String, Vec<String>)>, Vec<String>);

fn node_walks(store: &Store, nodes: &[String]) -> Vec<NodeWalk> {
    nodes.iter().map(|n| (n.clone(), deps(store, n), ancestors(store, n))).collect()
}

/// The chains through which commitment `c` applies to any of `nodes`, whatever its state (§12).
pub fn chains(store: &Store, c: &Record, attached: &HashMap<String, HashSet<String>>, nodes: &[String]) -> Vec<String> {
    chains_for_walks(c, attached, &node_walks(store, nodes))
}

fn chains_for_walks(c: &Record, attached: &HashMap<String, HashSet<String>>, walks: &[NodeWalk]) -> Vec<String> {
    let empty = HashSet::new();
    let att = attached.get(&c.id).unwrap_or(&empty);
    let mut chains: Vec<String> = vec![];
    let scope = match c.meta.get("applies") {
        None => Some(Applies::Linked),
        Some(v) => v.str().and_then(Applies::parse),
    };
    if scope == Some(Applies::Project) {
        chains.push("project-wide".into());
    }
    for (n, deps, ancestors) in walks {
        for (m, path) in deps {
            if *m == c.id && path.len() > 1 {
                chains.push(path.join(" "));
            } else if att.contains(m) {
                chains.push(if path.len() > 1 { format!("{} ← {}", path.join(" "), c.id) } else { format!("{n} ← {} (attached)", c.id) });
            }
        }
        if scope == Some(Applies::Subtree) {
            for a in ancestors.iter().filter(|a| att.contains(a.as_str())) {
                chains.push(format!("{n} is under {a} ← {} (applies: subtree)", c.id));
            }
        }
    }
    let mut uniq: Vec<String> = chains.into_iter().collect::<BTreeSet<_>>().into_iter().collect();
    uniq.sort_by_key(|c| c.chars().count());
    uniq
}

/// Whether commitment `c` governs `action` (§12). Only a `governs` that lists kinds, all of them in
/// `checks.action_kinds`, leaves an action out: one `check` rejects governs every action, so a
/// misspelt kind holds the gate instead of opening it.
pub fn governs(store: &Store, c: &Record, action: &str) -> bool {
    let Some(v) = doc::set(&c.meta, "governs") else { return true };
    let Some(vocabulary) = store.cfg.action_kinds.as_ref() else { return true };
    let kinds: Vec<Option<String>> = v.items().iter().map(Node::scalar).collect();
    if kinds.iter().any(|k| k.as_ref().is_none_or(|k| !vocabulary.contains(k))) {
        return true;
    }
    let kind = crate::config::Config::action_kind(action);
    kinds.iter().flatten().any(|k| k == kind)
}

pub fn applicable<'s>(store: &'s Store, nodes: &[String], action: Option<&str>) -> Vec<Applicable<'s>> {
    let attached = attachments(store);
    let walks = node_walks(store, nodes);
    let mut out = vec![];
    for c in store.own(Some(OwnKind::Commitment)) {
        // only a resolved commitment stops applying: a missing or malformed state (an error in check)
        // still gates, rather than silently letting the action through
        if c.state_str().and_then(CommitmentState::parse).is_some_and(|st| OwnKind::Commitment.is_resolved(st.as_str())) {
            continue;
        }
        let chains = chains_for_walks(c, &attached, &walks);
        if !chains.is_empty() {
            let reviews = c
                .meta
                .get("reviews")
                .map(Node::items)
                .unwrap_or_default()
                .iter()
                .filter_map(Node::map)
                .filter(|rv| action.is_none_or(|a| doc::str_of(rv, "action") == Some(a)))
                .collect();
            let governs_action = action.is_none_or(|a| governs(store, c, a));
            out.push(Applicable { commitment: c, chains, reviews, governs_action });
        }
    }
    out
}

/// Reasons `action` may not proceed: a commitment without a disposition for it, or one that blocks.
pub fn gate(items: &[Applicable], action: &str) -> Vec<String> {
    let mut out = vec![];
    for a in items.iter().filter(|a| a.governs_action) {
        let latest = a.reviews.iter().rfind(|rv| doc::str_of(rv, "action") == Some(action));
        match latest {
            None => out.push(format!("{}: no disposition recorded for {action}", a.commitment.id)),
            Some(rv) if doc::str_of(rv, "disposition").and_then(Disposition::parse) == Some(Disposition::Blocks) => {
                out.push(format!("{}: disposition 'blocks' — {}", a.commitment.id, doc::text(rv, "because").unwrap_or_default()))
            }
            Some(rv) if doc::str_of(rv, "disposition").and_then(Disposition::parse).is_none() => {
                out.push(format!("{}: latest review for {action} has a missing or invalid disposition", a.commitment.id))
            }
            Some(_) => {}
        }
    }
    out
}

