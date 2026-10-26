//! Commitment review before a consequential action (a run, a freeze, closing work).
//!
//! A commitment applies to an affected node when it is attached to the node directly, to anything
//! the node reaches through `waiting_on` or `uses`, to an ancestor when the commitment says
//! `applies: subtree`, or everywhere when it says `applies: project`. `parent` alone never carries
//! an obligation. Each applicable commitment is shown with the chain by which it applies; recording
//! a disposition for the action is a checked write to the commitment. Structure alone cannot tell
//! whether a scientific trigger has occurred: that judgement is the reviewer's, recorded with its
//! reason.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use crate::doc::{self, Map, Node};
use crate::records::{Applies, CommitmentState, Disposition, OwnKind, Record, Store, node_ref};

pub struct Applicable<'s> {
    pub commitment: &'s Record,
    pub chains: Vec<String>,
    /// existing reviews (for the action, when one is given)
    pub reviews: Vec<&'s Map>,
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

pub fn applicable<'s>(store: &'s Store, nodes: &[String], action: Option<&str>) -> Vec<Applicable<'s>> {
    let commitments = store.own(Some(OwnKind::Commitment));
    let mut attached: HashMap<&str, HashSet<String>> =
        commitments.iter().map(|c| (c.id.as_str(), c.ref_list("nodes").into_iter().collect())).collect();
    for r in store.own(None) {
        for x in r.ref_list("links") {
            if let Some(set) = attached.get_mut(x.as_str()) {
                set.insert(r.id.clone());
            }
        }
    }
    // the graph walks depend only on the nodes named, never on the commitment: walk each
    // node once, not once per commitment (this ran `deps`/`ancestors` per commitment)
    type NodeWalk = (String, Vec<(String, Vec<String>)>, Vec<String>);
    let walks: Vec<NodeWalk> = nodes.iter().map(|n| (n.clone(), deps(store, n), ancestors(store, n))).collect();
    let mut out = vec![];
    for c in commitments {
        // only a resolved commitment stops applying: a missing or malformed state (an error in check)
        // still gates, rather than silently letting the action through
        if c.state_str().and_then(CommitmentState::parse).is_some_and(|st| OwnKind::Commitment.is_resolved(st.as_str())) {
            continue;
        }
        let att = &attached[c.id.as_str()];
        let mut chains: Vec<String> = vec![];
        let scope = match c.meta.get("applies") {
            None => Some(Applies::Linked),
            Some(v) => v.str().and_then(Applies::parse),
        };
        if scope == Some(Applies::Project) {
            chains.push("project-wide".into());
        }
        for (n, deps, ancestors) in &walks {
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
        if !chains.is_empty() {
            let mut uniq: Vec<String> = chains.into_iter().collect::<BTreeSet<_>>().into_iter().collect();
            uniq.sort_by_key(|c| c.chars().count());
            let reviews = c
                .meta
                .get("reviews")
                .map(Node::items)
                .unwrap_or_default()
                .iter()
                .filter_map(Node::map)
                .filter(|rv| action.is_none_or(|a| doc::str_of(rv, "action") == Some(a)))
                .collect();
            out.push(Applicable { commitment: c, chains: uniq, reviews });
        }
    }
    out
}

/// Reasons `action` may not proceed: a commitment without a disposition for it, or one that blocks.
pub fn gate(items: &[Applicable], action: &str) -> Vec<String> {
    let mut out = vec![];
    for a in items {
        let latest = a.reviews.iter().rfind(|rv| doc::str_of(rv, "action") == Some(action));
        match latest {
            None => out.push(format!("{}: no disposition recorded for {action}", a.commitment.id)),
            Some(rv) if doc::str_of(rv, "disposition").and_then(Disposition::parse) == Some(Disposition::Blocks) => {
                out.push(format!("{}: disposition 'blocks' — {}", a.commitment.id, doc::text(rv, "because").unwrap_or_default()))
            }
            Some(_) => {}
        }
    }
    out
}
