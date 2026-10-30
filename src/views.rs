//! Generated views: the tree, the frontier, open questions, commitments. Plain Markdown (FORMAT.md §7).

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::doc::{self, Node, clen, squash, take};
use crate::fsx::file_name;
use crate::records::{Applies, OwnKind, Record, Store, ThreadStatus, node_ref};

const TITLE_WIDTH: usize = 64;

fn short(s: &str, n: usize) -> String {
    let s = squash(s);
    if clen(&s) <= n { s } else { format!("{}…", take(&s, n - 1)) }
}

/// Text for a Markdown table cell.
fn cell(s: &str) -> String {
    squash(s).replace('|', "\\|")
}

fn field_cell(r: &Record, key: &str) -> String {
    cell(&doc::text(&r.meta, key).unwrap_or_default())
}

/// The linked records `r` references from sources that list `show_fields` (FORMAT.md §4), in
/// reference order: each with where it came from (source, path, revision, the fields referencing it)
/// and its status and listed fields as the record states them, null when absent. Orientation only:
/// nothing here is interpreted, and a value such as a claim belongs to the linked system, not plied.
pub fn linked_work(store: &Store, r: &Record) -> Vec<serde_json::Value> {
    let mut order: Vec<String> = vec![];
    let mut via: HashMap<String, Vec<&'static str>> = HashMap::new();
    for (field, id) in r.refs() {
        let fields = via.entry(id.clone()).or_default();
        if fields.is_empty() {
            order.push(id);
        }
        if !fields.contains(&field) {
            fields.push(field);
        }
    }
    let value = |l: &Record, field: &str| l.meta.get(field).map_or(serde_json::Value::Null, Node::to_json);
    order
        .iter()
        .filter_map(|id| {
            let l = store.get(id).filter(|l| !l.is_own())?;
            let src = l.source().filter(|s| !s.show_fields.is_empty())?;
            let fields: serde_json::Map<String, serde_json::Value> = src.show_fields.iter().map(|f| (f.clone(), value(l, f))).collect();
            Some(serde_json::json!({
                "record": l.id, "source": src.name, "path": crate::fsx::rel(&store.cfg.base, &l.path), "rev": l.rev,
                "via": via[id], "status": src.status_field.as_deref().map_or(serde_json::Value::Null, |f| value(l, f)), "fields": fields,
            }))
        })
        .collect()
}

/// One `linked_work` item as a line for people: `?` marks a field the record does not state.
pub fn linked_work_line(item: &serde_json::Value) -> String {
    let shown = |v: &serde_json::Value| match v {
        serde_json::Value::Null => "?".to_string(),
        serde_json::Value::String(s) => short(s, 60),
        other => short(&other.to_string(), 60),
    };
    let s = |k: &str| item[k].as_str().unwrap_or_default().to_string();
    let via: Vec<&str> = item["via"].as_array().into_iter().flatten().filter_map(|v| v.as_str()).collect();
    let mut parts = vec![];
    if !item["status"].is_null() {
        parts.push(format!("status={}", shown(&item["status"])));
    }
    for (k, v) in item["fields"].as_object().into_iter().flatten() {
        parts.push(format!("{k}={}", shown(v)));
    }
    format!("{} ({}, via {}, rev {}): {}", s("record"), s("source"), via.join(", "), take(&s("rev"), 12), parts.join(" "))
}

/// Bounded orientation aid. The full author-written section remains in the record.
pub fn summary_preview(r: &Record) -> Option<String> {
    r.body_summary().map(|s| short(&s, 240))
}

fn refs(r: &Record, key: &str) -> String {
    r.ref_list(key).join(", ")
}

/// A scheduled node's position: its `next` value.
fn next(r: &Record) -> Option<&Node> {
    r.meta.get("next").filter(|v| !matches!(v, Node::Null))
}

fn is_unscheduled_open(r: &Record) -> bool {
    r.status() == Some(ThreadStatus::Open) && next(r).is_none()
}

/// One node's state line. `linked_state` adds the state of a `backlog` record, which is linked:
/// the generated view leaves it out, because a linked file can change without a plied write (§7).
pub fn node_summary(store: &Store, r: &Record, linked_state: bool) -> String {
    let m = &r.meta;
    let st = r.state_str().filter(|s| !s.is_empty());
    let mut bits: Vec<String> = vec![];
    match st {
        Some("group") => bits.push("(group)".into()),
        Some(s) => bits.push(s.into()),
        None => {}
    }
    if let Some(n) = next(r).filter(|_| st == Some("open")) {
        bits.push(format!("next {n}"));
    }
    if let Some(actor) = doc::text(m, "actor") {
        bits.push(actor);
    }
    let waiting = refs(r, "waiting_on");
    if !waiting.is_empty() {
        bits.push(format!("⟵ {waiting}"));
    }
    let uses = refs(r, "uses");
    if !uses.is_empty() {
        bits.push(format!("⇠ {uses}"));
    }
    if let Some(Node::Map(ap)) = m.get("approval")
        && let Some(state) = doc::text(ap, "state")
    {
        let by = doc::set(ap, "by").map(|b| format!(" ({})", node_ref(b))).unwrap_or_default();
        bits.push(format!("approval: {state}{by}"));
    }
    if st == Some("closed")
        && let Some(Node::Map(o)) = m.get("outcome")
        && let Some(oref) = doc::set(o, "ref")
    {
        bits.push(format!("outcome {}", node_ref(oref)));
    }
    let carries = refs(r, "carries");
    if !carries.is_empty() {
        bits.push(format!("carries {carries}"));
    }
    if let Some(b) = doc::set(m, "backlog") {
        let id = node_ref(b);
        if linked_state {
            let state = store.get(&id).and_then(Record::state).filter(|s| !s.is_empty()).map_or("?".into(), Node::to_string);
            bits.push(format!("[{id}: {state}]"));
        } else {
            bits.push(format!("[{id}]"));
        }
    }
    if let Some(summary) = summary_preview(r) {
        bits.push(format!("summary: {summary}"));
    }
    bits.join(", ")
}

pub fn tree_lines(store: &Store, root: Option<&str>, linked_state: bool) -> Vec<String> {
    let threads = store.own(Some(OwnKind::Thread));
    let ids: HashSet<&str> = threads.iter().map(|r| r.id.as_str()).collect();
    let mut kids: HashMap<Option<String>, Vec<&Record>> = HashMap::new();
    for r in &threads {
        let p = r.meta.get("parent").map(node_ref).filter(|p| ids.contains(p.as_str()));
        kids.entry(p).or_default().push(r);
    }

    struct Walk<'s> {
        store: &'s Store,
        kids: HashMap<Option<String>, Vec<&'s Record>>,
        rows: Vec<(String, String)>,
        seen: HashSet<String>,
        linked_state: bool,
    }
    impl<'s> Walk<'s> {
        fn visit(&mut self, r: &'s Record, prefix: &str, last: bool, top: bool) {
            let branch = if top { "" } else if last { "└── " } else { "├── " };
            if !self.seen.insert(r.id.clone()) {
                self.rows.push((format!("{prefix}{branch}{} (cycle)", r.id), String::new()));
                return;
            }
            self.rows.push((format!("{prefix}{branch}{}  {}", r.id, short(&r.title(), TITLE_WIDTH)), node_summary(self.store, r, self.linked_state)));
            let children = self.kids.get(&Some(r.id.clone())).cloned().unwrap_or_default();
            let sub = format!("{prefix}{}", if top { "" } else if last { "    " } else { "│   " });
            for (i, c) in children.iter().enumerate() {
                self.visit(c, &sub, i == children.len() - 1, false);
            }
        }
    }

    let starts: Vec<&Record> = match root {
        Some(id) => store.get(id).into_iter().collect(),
        None => kids.get(&None).cloned().unwrap_or_default(),
    };
    let mut walk = Walk { store, kids, rows: vec![], seen: HashSet::new(), linked_state };
    for (i, r) in starts.iter().enumerate() {
        walk.visit(r, "", true, true);
        if i < starts.len() - 1 {
            walk.rows.push((String::new(), String::new()));
        }
    }
    let width = walk.rows.iter().map(|(a, _)| clen(a)).max().unwrap_or(0);
    walk.rows.iter().map(|(a, b)| format!("{a}{}   {b}", " ".repeat(width - clen(a))).trim_end().to_string()).collect()
}

/// The work tree as JSON (FORMAT.md §11): threads nested under their parent, in the order the text tree
/// shows them. A thread reached a second time (a `parent` cycle) appears as `{"id", "cycle": true}`.
pub fn tree_json(store: &Store, root: Option<&str>) -> serde_json::Value {
    use serde_json::{Value, json};
    let threads = store.own(Some(OwnKind::Thread));
    let ids: HashSet<&str> = threads.iter().map(|r| r.id.as_str()).collect();
    let mut kids: HashMap<Option<String>, Vec<&Record>> = HashMap::new();
    for r in &threads {
        let p = r.meta.get("parent").map(node_ref).filter(|p| ids.contains(p.as_str()));
        kids.entry(p).or_default().push(r);
    }
    fn visit(r: &Record, kids: &HashMap<Option<String>, Vec<&Record>>, seen: &mut HashSet<String>) -> Value {
        if !seen.insert(r.id.clone()) {
            return json!({"id": r.id, "cycle": true});
        }
        let children: Vec<Value> = kids.get(&Some(r.id.clone())).into_iter().flatten().map(|c| visit(c, kids, seen)).collect();
        json!({"id": r.id, "title": r.title(), "status": r.state_str(), "rev": r.rev, "body_summary": r.body_summary(), "children": children})
    }
    let starts: Vec<&Record> = match root {
        Some(id) => store.get(id).into_iter().collect(),
        None => kids.get(&None).cloned().unwrap_or_default(),
    };
    let mut seen = HashSet::new();
    Value::Array(starts.iter().map(|r| visit(r, &kids, &mut seen)).collect())
}

/// Every date field on an unresolved record, earliest first, with whether it asks for attention
/// today (`Record::date_passed`). Not part of the generated view, which must not depend on the day.
pub fn dates(store: &Store, today: chrono::NaiveDate) -> Vec<(&Record, &'static str, chrono::NaiveDate, bool)> {
    let mut out = vec![];
    for r in store.own(None).into_iter().filter(|r| !r.is_resolved()) {
        for f in crate::records::DATE_FIELDS {
            if let (Some(d), Some(passed)) = (r.date(f), r.date_passed(f, today)) {
                out.push((r, f, d, passed));
            }
        }
    }
    out.sort_by(|a, b| (a.2, &a.0.id).cmp(&(b.2, &b.0.id)));
    out
}

/// Scheduled open nodes by actor, in `next` order.
pub fn frontier(store: &Store) -> BTreeMap<String, Vec<&Record>> {
    let mut out: BTreeMap<String, Vec<&Record>> = BTreeMap::new();
    for r in store.own(Some(OwnKind::Thread)) {
        if r.status() == Some(ThreadStatus::Open) && next(r).is_some() {
            let actor = doc::text(&r.meta, "actor").unwrap_or_else(|| "unassigned".into());
            out.entry(actor).or_default().push(r);
        }
    }
    for v in out.values_mut() {
        v.sort_by_key(|r| (next(r).and_then(Node::int).unwrap_or(i64::MAX), r.id.clone()));
    }
    out
}

/// Open threads with no `next`, so in no queue, in id order.
pub fn unscheduled(store: &Store) -> Vec<&Record> {
    store.own(Some(OwnKind::Thread)).into_iter().filter(|r| is_unscheduled_open(r)).collect()
}

pub fn render_view(store: &Store) -> String {
    let cfg = &store.cfg;
    let link = |kind: OwnKind, r: &Record| format!("[{}]({}/{})", r.id, cfg.kind(kind).dir, file_name(&r.path));
    let mut l: Vec<String> = vec![
        "# Work record".into(),
        "".into(),
        format!("<!-- Generated by plied {} from {}/ — do not edit. Change records with `plied`. -->", crate::VERSION, cfg.root),
        "".into(),
        "## Frontier".into(),
        "".into(),
    ];
    let fr = frontier(store);
    if fr.is_empty() {
        l.push("Nothing scheduled.".into());
    }
    for (actor, rs) in &fr {
        l.extend([format!("**{actor}**"), "".into()]);
        for r in rs {
            l.push(format!("{}. {} {}", next(r).map(Node::to_string).unwrap_or_default(), link(OwnKind::Thread, r), cell(&r.title())));
            if let Some(summary) = summary_preview(r) {
                l.push(format!("   Summary (author-written preview): {}", cell(&summary)));
            }
        }
        l.push("".into());
    }
    let threads = store.own(Some(OwnKind::Thread));
    let unscheduled: Vec<&str> = unscheduled(store).iter().map(|r| r.id.as_str()).collect();
    if !unscheduled.is_empty() {
        l.extend(["".into(), format!("**Open, not scheduled:** {}", unscheduled.join(", ")), "".into()]);
    }

    let questions: Vec<&Record> = store.own(Some(OwnKind::Question)).into_iter().filter(|q| q.state_str() == Some("open")).collect();
    l.extend(["".into(), "## Open questions".into(), "".into()]);
    if questions.is_empty() {
        l.push("None.".into());
    } else {
        l.extend(["| id | question | raised | bears on |".into(), "|---|---|---|---|".into()]);
        for q in questions {
            l.push(format!(
                "| {} | {} | {} | {} |",
                link(OwnKind::Question, q),
                cell(&q.title()),
                field_cell(q, "raised"),
                refs(q, "nodes")
            ));
        }
    }

    let commitments = store.own(Some(OwnKind::Commitment));
    l.extend(["".into(), "## Commitments".into(), "".into()]);
    if commitments.is_empty() {
        l.push("None.".into());
    } else {
        l.extend(["| id | commitment | made | state | applies to |".into(), "|---|---|---|---|---|".into()]);
        for c in commitments {
            let st = c.state().map(Node::to_string).unwrap_or_default();
            let st = match st.as_str() {
                "due" | "missed" => format!("**{st}**"),
                "superseded" => match doc::set(&c.meta, "superseded_by") {
                    Some(by) => format!("superseded → {}", node_ref(by)),
                    None => st,
                },
                _ => st,
            };
            let applies = c.meta.get("applies").and_then(Node::str).and_then(Applies::parse);
            let nodes = match applies {
                Some(Applies::Project) => "project-wide".to_string(),
                Some(Applies::Subtree) => format!("{} (and below)", refs(c, "nodes")),
                _ => refs(c, "nodes"),
            };
            let summary = if doc::set(&c.meta, "summary").is_some() { field_cell(c, "summary") } else { cell(&c.title()) };
            l.push(format!("| {} | {summary} | {} | {st} | {nodes} |", link(OwnKind::Commitment, c), field_cell(c, "made")));
        }
    }

    let held: Vec<&&Record> =
        threads.iter().filter(|r| matches!(r.status(), Some(ThreadStatus::Held | ThreadStatus::Parked))).collect();
    if !held.is_empty() {
        l.extend(["".into(), "## Held or parked".into(), "".into()]);
        for r in held {
            let revisit = doc::text(&r.meta, "revisit").map(|v| format!(" — revisit: {}", cell(&v))).unwrap_or_default();
            l.push(format!("- {} ({}) {}{revisit}", r.id, r.state().map(Node::to_string).unwrap_or_default(), cell(&r.title())));
            if let Some(summary) = summary_preview(r) {
                l.push(format!("  Summary (author-written preview): {}", cell(&summary)));
            }
        }
    }

    let tree = tree_lines(store, None, false);
    l.extend(["".into(), "## Tree".into(), "".into(), "```".into()]);
    if tree.is_empty() {
        l.push("(empty)".into());
    } else {
        l.extend(tree);
    }
    l.extend(["```".into(), "".into()]);
    l.join("\n")
}
