//! Validation (FORMAT.md §8). Errors block writes and fail `plied check`; warnings change nothing.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::sync::LazyLock;

use chrono::{NaiveDate, Utc};
use regex::Regex;
use serde::Serialize;

use crate::config::Pattern;
use crate::doc::{self, Node};
use crate::fsx::rel;
use crate::literature::{Library, Status};
use crate::messages::Messages;
use crate::records::{Applies, CommitmentState, Disposition, OwnKind, QuestionState, Record, Store, ThreadStatus, is_rev, node_ref, ref_id};
use crate::textmatch;
use crate::schema::{subfields, enums};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Error,
    Warning,
}

/// A diagnostic. Its identity is (level, code, location, subject), never the message (FORMAT.md §8):
/// a write is refused when it adds an error whose identity the store did not already have.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct Problem {
    pub level: Level,
    pub code: String,
    #[serde(rename = "where")]
    pub location: String,
    /// what in `location` the problem is about: a field, `field:target`, … (empty when the code and
    /// location say it all)
    #[serde(skip_serializing_if = "String::is_empty")]
    pub subject: String,
    pub message: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = match self.level {
            Level::Error => "error",
            Level::Warning => "warning",
        };
        write!(f, "{level}: {}: {} [{}]", self.location, self.message, self.code)
    }
}

impl Problem {
    pub fn error(code: &str, location: &str, message: impl Into<String>) -> Problem {
        Problem::error_on(code, location, "", message)
    }

    pub fn error_on(code: &str, location: &str, subject: &str, message: impl Into<String>) -> Problem {
        Problem { level: Level::Error, code: code.into(), location: location.into(), subject: subject.into(), message: message.into() }
    }

    pub fn identity(&self) -> (Level, &str, &str, &str) {
        (self.level, &self.code, &self.location, &self.subject)
    }
}

/// Collects problems; warnings only when asked for.
struct Report {
    out: Vec<Problem>,
    warnings: bool,
}

impl Report {
    fn err(&mut self, code: &str, at: &str, subject: &str, msg: impl Into<String>) {
        self.out.push(Problem::error_on(code, at, subject, msg));
    }

    fn warn(&mut self, code: &str, at: &str, msg: impl Into<String>) {
        self.warn_on(code, at, "", msg);
    }

    fn warn_on(&mut self, code: &str, at: &str, subject: &str, msg: impl Into<String>) {
        if self.warnings {
            self.out.push(Problem { level: Level::Warning, code: code.into(), location: at.into(), subject: subject.into(), message: msg.into() });
        }
    }
}

pub struct Inputs<'a> {
    pub library: Option<&'a Library<'a>>,
    pub messages: Option<&'a Messages<'a>>,
    /// the view as it should be, to compare with the file
    pub view: Option<&'a str>,
    pub warnings: bool,
}

pub fn run(store: &Store, inputs: &Inputs) -> Vec<Problem> {
    let cfg = &store.cfg;
    let mut p = Report { out: vec![], warnings: inputs.warnings };
    for (subject, msg) in cfg.compat_problems() {
        p.err("version", "plied.toml", subject, msg);
    }
    for (code, at, msg) in &store.load_problems {
        if *code == "source-id" { p.warn(code, at, msg.as_str()) } else { p.err(code, at, "", msg.as_str()) }
    }
    for name in &store.empty_sources {
        p.warn_on("source-empty", "plied.toml", name, format!("source {name:?} matches no readable files; check its glob and access policy"));
    }
    for src in cfg.sources.iter().filter(|s| s.authority_field.is_some() && s.scope_field.is_none()) {
        p.err(
            "config",
            "plied.toml",
            &src.name,
            format!("sources {:?} declares authority but no scope field, so approvals by its records cannot be checked against what they approve", src.name),
        );
    }
    for r in store.own(None) {
        let at = rel(&cfg.base, &r.path);
        fields(r, &at, &mut p);
        refs(store, r, &at, &mut p);
        match r.own() {
            Some(OwnKind::Thread) => thread(store, r, &at, &mut p),
            Some(OwnKind::Commitment) => commitment(store, r, &at, &mut p),
            _ => question(store, r, &at, &mut p),
        }
        cites(r, &at, inputs.library, &mut p);
        body_mentions(store, r, &at, &mut p);
    }
    cycles(store, OwnKind::Thread, "waiting_on", &mut p);
    cycles(store, OwnKind::Thread, "parent", &mut p);
    cycles(store, OwnKind::Commitment, "superseded_by", &mut p);
    if let Some(m) = inputs.messages {
        for (at, msg) in m.verify() {
            p.err("messages", &at, "", msg);
        }
    }
    if inputs.warnings {
        dates(store, &mut p);
        stale(store, &mut p);
        supersessions(store, &mut p);
        unrecorded_commitments(store, &mut p);
        unlinked(store, &mut p);
        let vp = cfg.view_path();
        if let Some(view) = inputs.view
            && vp.exists()
            && crate::fsx::read_text(&vp).ok().as_deref() != Some(view)
        {
            p.warn("view", &rel(&cfg.base, &vp), "generated view is out of date (run `plied view`)");
        }
    }
    p.out
}

fn one_of(values: &[&str]) -> String {
    values.join(", ")
}

/// A value as it reads in a message.
fn shown(v: Option<&Node>) -> String {
    match v {
        None | Some(Node::Null) => "nothing".into(),
        Some(Node::Str(s)) => format!("{s:?}"),
        Some(other) => other.to_string(),
    }
}

// ------------------------------------------------------------------------------ per record

fn fields(r: &Record, at: &str, p: &mut Report) {
    let kind = r.own().expect("own record");
    for (k, v) in &r.meta {
        if !kind.fields().contains(&k.as_str()) {
            p.err("field", at, k, format!("unknown field {k:?}"));
            continue;
        }
        let Some(sub) = subfields(k) else { continue };
        let items = match v {
            Node::List(items) => items.iter().collect(),
            other => vec![other],
        };
        for it in items {
            match it {
                Node::Null => {}
                Node::Map(m) => {
                    for sk in m.keys().filter(|sk| !sub.contains(&sk.as_str())) {
                        p.err("field", at, &format!("{k}.{sk}"), format!("unknown field {k}.{sk}"));
                    }
                    for (sk, allowed) in enums(k) {
                        if let Some(v) = doc::set(m, sk)
                            && !v.str().is_some_and(|x| allowed.contains(&x))
                        {
                            p.err("field", at, &format!("{k}.{sk}"), format!("{k}.{sk} must be one of {}, got {}", one_of(allowed), shown(Some(v))));
                        }
                    }
                }
                _ => p.err("field", at, k, format!("{k} must be a mapping{}", if matches!(v, Node::List(_)) { " list" } else { "" })),
            }
        }
    }
    if doc::set(&r.meta, "title").is_none() {
        p.err("required", at, "title", "title is required");
    }
    for f in crate::records::DATE_FIELDS {
        if let Some(v) = doc::set(&r.meta, f).filter(|_| kind.fields().contains(&f))
            && r.date(f).is_none()
        {
            p.err("field", at, f, format!("{f} must be a date, YYYY-MM-DD, got {}", shown(Some(v))));
        }
    }
}

/// A broken reference's message, saying when the record may be sealed.
fn missing(store: &Store, f: &str, rid: &str) -> String {
    if store.maybe_sealed(rid) {
        format!("{f}: {rid} does not exist or is in a sealed path (plied cannot verify what it may not read)")
    } else {
        format!("{f}: {rid} does not exist")
    }
}

fn refs(store: &Store, r: &Record, at: &str, p: &mut Report) {
    for (f, rid) in r.refs() {
        let subject = format!("{f}:{rid}");
        let Some(t) = store.get(&rid) else {
            p.err("ref", at, &subject, missing(store, f, &rid));
            continue;
        };
        match f {
            "parent" if !t.is(OwnKind::Thread) => p.err("ref", at, &subject, format!("parent {rid} is not a thread")),
            "nodes" if !(t.is(OwnKind::Thread) || t.is(OwnKind::Question)) => p.err("ref", at, &subject, format!("nodes: {rid} is not a thread or question")),
            "backlog" if t.is_own() => p.err("ref", at, &subject, format!("backlog: {rid} is a plied record, not a linked item")),
            "waiting_on" | "uses" if rid == r.id => p.err("ref", at, &subject, format!("{f}: a node cannot depend on itself")),
            "superseded_by" if rid == r.id => p.err("ref", at, &subject, "superseded_by: a commitment cannot replace itself"),
            "superseded_by" if !t.is(OwnKind::Commitment) => p.err("ref", at, &subject, format!("superseded_by: {rid} is not a commitment")),
            _ => {}
        }
    }
}

/// A quotation must appear verbatim in the record it cites (§3.6).
fn quote(store: &Store, at: &str, p: &mut Report, quote: Option<&Node>, source: Option<&Node>, what: &str) {
    let Some(q) = quote.filter(|q| !q.is_empty()).and_then(Node::scalar) else {
        p.err("required", at, &format!("{what}.quote"), format!("{what}: the quotation is required"));
        return;
    };
    let Some(source) = source.filter(|s| !s.is_empty()).map(node_ref) else {
        p.err("required", at, &format!("{what}.source"), format!("{what}: the record it is quoted from is required"));
        return;
    };
    let Some(t) = store.get(&source) else { return }; // reported as a broken reference
    if !textmatch::find_quote(&q, &t.text) {
        let near = textmatch::closest(&q, &t.text, 60).map(|n| format!(" (nearest: “{n}…”)")).unwrap_or_default();
        p.err("quote", at, what, format!("{what} is not verbatim in {source}{near}; quote the record exactly, or record the wording first"));
    }
}

static AUTHORITY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\w[\w-]*)(?:\s*(?:by|:)?\s*(\S+))?").unwrap());
static SCOPE_SPLIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\s,\[\]]+").unwrap());

/// Check that `by` names a record with authority over `node` (§4).
fn authority(store: &Store, by: &Node, node: &str, at: &str, p: &mut Report, what: &str) {
    let Some(t) = store.get(&node_ref(by)) else { return };
    let Some(src) = t.source() else {
        p.err("authority", at, &format!("{what}:{}", t.id), format!("{what}: {} is a plied record; cite the decision record itself", t.id));
        return;
    };
    let Some(af) = &src.authority_field else {
        p.warn("authority", at, format!("{what}: {} cannot be verified ({} records declare no authority field)", t.id, src.name));
        return;
    };
    let Some(raw) = doc::text(&t.meta, af).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) else {
        p.warn("authority", at, format!("{what}: {} has no {af} field, so its authority cannot be verified", t.id));
        return;
    };
    let (who, via) = match AUTHORITY.captures(&raw) {
        Some(m) => (m[1].to_string(), m.get(2).map(|x| x.as_str().to_string())),
        None => (raw.clone(), None),
    };
    let auths = &store.cfg.approve_authorities;
    if !auths.contains(&who) {
        p.err("authority", at, &format!("{what}:{}", t.id), format!("{what}: {} has authority {raw:?}, not one of {}", t.id, auths.join(", ")));
        return;
    }
    if who == "delegated" && via.as_deref().is_none_or(|v| store.get(v).is_none()) {
        p.err("authority", at, &format!("{what}:{}", t.id), format!("{what}: {} is delegated but names no existing delegating record", t.id));
    }
    let Some(sf) = &src.scope_field else {
        // a config gap, reported once by run()
        p.warn("authority", at, format!("{what}: the scope of {} cannot be checked ({} records declare no scope field)", t.id, src.name));
        return;
    };
    let scope: BTreeSet<String> = match t.meta.get(sf) {
        Some(Node::List(items)) => items.iter().map(node_ref).filter(|x| !x.is_empty()).collect(),
        other => {
            let text = other.and_then(Node::scalar).unwrap_or_default();
            SCOPE_SPLIT.split(&text).map(ref_id).filter(|x| !x.is_empty()).collect()
        }
    };
    if scope.is_empty() {
        p.err("authority", at, &format!("{what}:{}", t.id), format!("{what}: {} declares no scope, so it cannot approve {node}", t.id));
    } else if !scope.contains(node) {
        let list: Vec<&str> = scope.iter().map(String::as_str).collect();
        p.err("authority", at, &format!("{what}:{}", t.id), format!("{what}: the scope of {} ({}) does not include {node}", t.id, list.join(", ")));
    }
}

fn thread(store: &Store, r: &Record, at: &str, p: &mut Report) {
    // an invalid status skips only the checks that depend on it
    let m = &r.meta;
    let status = r.status();
    if status.is_none() {
        p.err("state", at, "status", format!("status must be one of {}, got {}", one_of(ThreadStatus::ALL), shown(r.state())));
    }
    let is = |s: ThreadStatus| status == Some(s);
    let waiting = r.ref_list("waiting_on");
    let unresolved: Vec<&str> = waiting.iter().filter(|x| store.get(x).is_some_and(|t| !t.is_resolved())).map(String::as_str).collect();
    let next = m.get("next").filter(|v| !matches!(v, Node::Null));
    if let Some(next) = next {
        if let Some(st) = status.filter(|s| *s != ThreadStatus::Open) {
            p.err("state", at, "next", format!("next is set but status is {st}; only open nodes are scheduled"));
        }
        if !next.int().is_some_and(|n| n >= 1) {
            p.err("field", at, "next", "next must be a positive integer");
        }
    }
    if is(ThreadStatus::Waiting) && waiting.is_empty() {
        p.err("state", at, "waiting_on", "status waiting needs waiting_on");
    }
    if is(ThreadStatus::Open) && !unresolved.is_empty() {
        // one error per dependency, so resolving one of several is never "adding" an error
        for u in &unresolved {
            p.err("state", at, &format!("waiting_on:{u}"), format!("status open, but waiting_on {u} is unresolved"));
        }
    }
    if is(ThreadStatus::Waiting) && !waiting.is_empty() && unresolved.is_empty() {
        p.warn("unblocked", at, format!("everything in waiting_on ({}) is resolved; the node may be open", waiting.join(", ")));
    }
    if is(ThreadStatus::Group) && (!waiting.is_empty() || next.is_some()) {
        p.err("state", at, "group", "a group node carries no waiting_on or next");
    }
    if let (Some(actor), Some(actors)) = (doc::set(m, "actor"), store.cfg.actors.as_ref().filter(|a| !a.is_empty()))
        && !actor.str().is_some_and(|a| actors.iter().any(|x| x == a))
    {
        p.err("field", at, "actor", format!("actor {} is not one of {}", shown(Some(actor)), actors.join(", ")));
    }
    let outcome = m.get("outcome");
    if is(ThreadStatus::Closed) {
        match outcome {
            Some(Node::Map(o)) => quote(store, at, p, o.get("quote"), o.get("ref"), "outcome"),
            _ => p.err("required", at, "outcome", "a closed node needs outcome (quote and ref)"),
        }
    } else if let Some(st) = status.filter(|_| outcome.is_some_and(|o| !o.is_empty())) {
        p.err("state", at, "outcome", format!("outcome is set but status is {st}"));
    }
    if doc::set(m, "carries").is_some() && status.is_some_and(|s| s != ThreadStatus::Closed) {
        p.err("state", at, "carries", "carries is for closed nodes");
    }
    // scan reviews are only ever appended, so an index names the same one in every later state
    for (i, sc) in m.get("scanned").map(Node::items).unwrap_or_default().iter().enumerate() {
        let Some(sc) = sc.map() else { continue };
        match doc::set(sc, "record").map(node_ref).filter(|x| !x.is_empty()) {
            None => p.err("field", at, &format!("scanned[{i}].record"), "a scan review needs the record it read"),
            Some(rid) => match store.get(&rid) {
                None => p.err("ref", at, &format!("scanned:{rid}"), missing(store, "scanned", &rid)),
                Some(t) if t.is_own() => p.err("ref", at, &format!("scanned:{rid}"), format!("scanned: {rid} is a plied record, not a linked one")),
                Some(_) => {}
            },
        }
        if !doc::text(sc, "rev").is_some_and(|x| is_rev(x.trim())) {
            p.err("field", at, &format!("scanned[{i}].rev"), "a scan review needs the revision it read (at least 8 lowercase hex digits)");
        }
    }
    if let Some(Node::Map(ap)) = m.get("approval") {
        match ap.get("state").and_then(Node::str) {
            Some("approved") => match ap.get("by").filter(|b| !b.is_empty()) {
                None => p.err("required", at, "approval.by", "approval: approved needs by (the decision record)"),
                Some(by) => authority(store, by, &r.id, at, p, "approval"),
            },
            Some("not approved") => {}
            _ => p.err("field", at, "approval.state", "approval.state must be 'approved' or 'not approved'"),
        }
    }
}

fn commitment(store: &Store, r: &Record, at: &str, p: &mut Report) {
    let m = &r.meta;
    let state = r.state_str().and_then(CommitmentState::parse);
    if state.is_none() {
        p.err("state", at, "state", format!("state must be one of {}, got {}", one_of(CommitmentState::ALL), shown(r.state())));
    }
    quote(store, at, p, m.get("wording"), m.get("made"), "wording");
    let resolved_by = doc::set(m, "resolved_by");
    if let Some(st) = state.filter(|s| OwnKind::Commitment.is_resolved(s.as_str()))
        && resolved_by.is_none()
    {
        p.err("required", at, "resolved_by", format!("state {st} needs resolved_by (the record that says so)"));
    }
    // releasing an obligation, or moving it into another commitment, is a decision (§4)
    let decided = match state {
        Some(CommitmentState::Waived) => Some("waiver"),
        Some(CommitmentState::Superseded) => Some("supersession"),
        _ => None,
    };
    if let (Some(what), Some(rb)) = (decided, resolved_by) {
        authority(store, rb, &r.id, at, p, what);
    }
    let superseded_by = doc::set(m, "superseded_by");
    if state == Some(CommitmentState::Superseded) && superseded_by.is_none() {
        p.err("required", at, "superseded_by", "state superseded needs superseded_by (the commitment that now holds the obligation)");
    } else if let Some(st) = state.filter(|s| *s != CommitmentState::Superseded && superseded_by.is_some()) {
        p.err("state", at, "superseded_by", format!("superseded_by is set but state is {st}"));
    }
    governs(store, r, at, p);
    let applies = match m.get("applies") {
        None => Some(Applies::Linked),
        Some(v) => v.str().and_then(Applies::parse),
    };
    if applies.is_none() {
        p.err("field", at, "applies", "applies must be linked, subtree or project");
    }
    if applies != Some(Applies::Project) && doc::set(m, "nodes").is_none() {
        p.warn("unattached", at, "no nodes: this commitment surfaces in no review (attach it, or set applies: project)");
    }
    // reviews are only ever appended, so an index names the same review in every later state
    for (i, rv) in m.get("reviews").map(Node::items).unwrap_or_default().iter().enumerate() {
        let Some(rv) = rv.map() else { continue };
        review_input_fields(store, r, rv, i, at, p);
        if rv.get("disposition").and_then(Node::str).and_then(Disposition::parse).is_none() {
            p.err("field", at, &format!("reviews[{i}].disposition"), format!("review disposition must be one of {}", one_of(Disposition::ALL)));
        }
        match doc::text(rv, "action") {
            None => p.err("field", at, &format!("reviews[{i}].action"), "a review needs its action"),
            // a warning, so adopting a vocabulary never blocks writes to records that predate it
            Some(a) => {
                if let Some(msg) = store.cfg.action_problem(&a) {
                    p.warn("action", at, format!("reviews[{i}]: {msg}"));
                }
            }
        }
    }
    for claim in crate::review::input_reviews(store, r, None) {
        for (j, input) in claim.inputs.iter().enumerate().filter(|(_, input)| matches!(input.status, "changed" | "unavailable")) {
            let message = if input.status == "changed" {
                format!("review for {}: input {} changed since it was read; append a new disposition after reviewing it", claim.action, input.record)
            } else {
                format!("review for {}: input {} is unavailable (missing or possibly sealed)", claim.action, input.record)
            };
            p.warn_on("review-input", at, &format!("reviews[{}].inputs[{j}]", claim.review_index), message);
        }
    }
}

fn review_input_fields(store: &Store, record: &Record, review: &crate::doc::Map, index: usize, at: &str, p: &mut Report) {
    let Some(values) = doc::set(review, "inputs") else { return };
    let field = format!("reviews[{index}].inputs");
    let Node::List(inputs) = values else {
        p.err("field", at, &field, "review inputs must be a list of record/rev mappings");
        return;
    };
    let mut seen = HashMap::new();
    for (j, input) in inputs.iter().enumerate() {
        let field = format!("{field}[{j}]");
        let Some(input) = input.map() else { p.err("field", at, &field, "a review input must be a record/rev mapping"); continue };
        for key in input.keys().filter(|key| !subfields("reviews.inputs").unwrap().contains(&key.as_str())) {
            p.err("field", at, &format!("{field}.{key}"), format!("unknown review input field {key}"));
        }
        let id = doc::str_of(input, "record").filter(|id| crate::review::input_id(store, id));
        if id.is_none() {
            p.err("field", at, &format!("{field}.record"), "a review input needs an exact configured record id, without a locator");
        } else if id == Some(record.id.as_str()) {
            p.err("field", at, &format!("{field}.record"), "a review cannot pin its own commitment; --expect already guards it");
        }
        let rev = doc::str_of(input, "rev").filter(|rev| crate::review::full_revision(rev));
        if rev.is_none() {
            p.err("field", at, &format!("{field}.rev"), "a review input needs a full 64-character lowercase raw-byte SHA-256");
        }
        if let (Some(id), Some(rev)) = (id, rev) && let Some(previous) = seen.insert(id, rev) && previous != rev {
            p.err("field", at, &format!("{field}.rev"), format!("conflicting review input revisions for {id}"));
        }
    }
}

/// `governs` names action kinds from `checks.action_kinds` (§3.3). Without that vocabulary a misspelt
/// kind would leave the commitment governing nothing, so `governs` needs it.
fn governs(store: &Store, r: &Record, at: &str, p: &mut Report) {
    let Some(v) = doc::set(&r.meta, "governs") else { return };
    let Some(vocabulary) = store.cfg.action_kinds.as_ref() else {
        p.err("field", at, "governs", "governs needs checks.action_kinds, so that a misspelt kind cannot leave a commitment governing nothing");
        return;
    };
    for k in v.items() {
        match k.scalar() {
            Some(k) if vocabulary.contains(&k) => {}
            Some(k) => p.err("field", at, &format!("governs:{k}"), format!("governs: {k:?} is not one of checks.action_kinds ({})", vocabulary.join(", "))),
            None => p.err("field", at, "governs", "governs must be a list of action kinds"),
        }
    }
}

fn question(store: &Store, r: &Record, at: &str, p: &mut Report) {
    let m = &r.meta;
    let state = r.state_str().and_then(QuestionState::parse);
    if state.is_none() {
        p.err("state", at, "state", format!("state must be one of {}, got {}", one_of(QuestionState::ALL), shown(r.state())));
    }
    if doc::set(m, "raised").is_none() {
        p.err("required", at, "raised", "raised (the record that raised it) is required");
    }
    if state == Some(QuestionState::Answered) {
        match doc::set(m, "answered_by") {
            None => p.err("required", at, "answered_by", "an answered question needs answered_by"),
            Some(by) => authority(store, by, &r.id, at, p, "answer"),
        }
    }
}

fn cites(r: &Record, at: &str, library: Option<&Library>, p: &mut Report) {
    let cites: Vec<&doc::Map> = r.meta.get("cites").map(Node::items).unwrap_or_default().iter().filter_map(Node::map).collect();
    if doc::set(&r.meta, "cites").is_none() {
        return;
    }
    let Some(lib) = library else {
        p.err("literature", at, "cites", "cites papers, but no [literature] is configured");
        return;
    };
    for c in cites {
        let field = |k: &str| c.get(k).map(Node::to_string).unwrap_or_default();
        let label = format!("{} p{}", field("paper"), field("page"));
        // "unchecked" (no page text yet; no quote for a table or figure value) is neither an error nor a warning
        for ch in lib.check_citation(c) {
            let code = format!("lit-{}", ch.aspect);
            match ch.status {
                Status::Fail => p.err(&code, at, &label, format!("{label}: {}", ch.message)),
                Status::Warn => p.warn(&code, at, format!("{label}: {}", ch.message)),
                Status::Ok | Status::Unchecked => {}
            }
        }
    }
}

fn body_mentions(store: &Store, r: &Record, at: &str, p: &mut Report) {
    let missing: BTreeSet<String> = store.mentions(&r.body).into_iter().filter(|t| !store.contains(t)).collect();
    if !missing.is_empty() {
        let list: Vec<&str> = missing.iter().map(String::as_str).collect();
        p.warn("mention", at, format!("the body mentions ids that do not exist: {}", list.join(", ")));
    }
}

// ----------------------------------------------------------------------------------- graph

fn cycles(store: &Store, kind: OwnKind, field: &str, p: &mut Report) {
    let records = store.own(Some(kind));
    let graph: HashMap<&str, Vec<String>> = records
        .iter()
        .map(|r| {
            let targets = match r.meta.get(field) {
                Some(Node::List(items)) => items.iter().map(node_ref).collect(),
                Some(v) if !v.is_empty() => vec![node_ref(v)],
                _ => vec![],
            };
            (r.id.as_str(), targets.into_iter().filter(|x| store.get(x).is_some_and(Record::is_own)).collect())
        })
        .collect();

    #[derive(PartialEq)]
    enum Mark {
        Active,
        Done,
    }
    struct Walk<'g> {
        graph: &'g HashMap<&'g str, Vec<String>>,
        marks: HashMap<String, Mark>,
        reported: HashSet<BTreeSet<String>>,
        path: Vec<String>,
    }
    impl Walk<'_> {
        fn visit(&mut self, n: &str, field: &str, p: &mut Report) {
            self.marks.insert(n.to_string(), Mark::Active);
            for m in self.graph.get(n).cloned().unwrap_or_default() {
                match self.marks.get(&m) {
                    Some(Mark::Active) => {
                        let start = self.path.iter().position(|x| *x == m).unwrap_or(self.path.len() - 1);
                        let mut cycle: Vec<String> = self.path[start..].to_vec();
                        if self.reported.insert(cycle.iter().cloned().collect()) {
                            // start at the smallest id, so the same cycle has the same location whatever
                            // order the walk found it in
                            let first = cycle.iter().enumerate().min_by_key(|(_, x)| x.as_str()).map_or(0, |(i, _)| i);
                            cycle.rotate_left(first);
                            cycle.push(cycle[0].clone());
                            p.err("cycle", &cycle.join(" → "), field, format!("{field} forms a cycle"));
                        }
                    }
                    Some(Mark::Done) => {}
                    None => {
                        self.path.push(m.clone());
                        self.visit(&m, field, p);
                        self.path.pop();
                    }
                }
            }
            self.marks.insert(n.to_string(), Mark::Done);
        }
    }

    let mut walk = Walk { graph: &graph, marks: HashMap::new(), reported: HashSet::new(), path: vec![] };
    for r in &records {
        if !walk.marks.contains_key(&r.id) {
            walk.path = vec![r.id.clone()];
            walk.visit(&r.id, field, p);
        }
    }
}

// -------------------------------------------------------------------------------- warnings

/// Dates that ask for attention. Warnings only: they depend on today, and never block (§3.3).
fn dates(store: &Store, p: &mut Report) {
    let today = Utc::now().date_naive();
    for r in store.own(None) {
        let at = rel(&store.cfg.base, &r.path);
        for f in crate::records::DATE_FIELDS {
            if r.date_passed(f, today) == Some(true) {
                let d = r.date(f).map(|d| d.to_string()).unwrap_or_default();
                let what = if f == "due" { format!("due {d} has passed") } else { format!("revisit_on {d} has come, and it is still {}", r.state_str().unwrap_or_default()) };
                p.warn("date", &at, what);
            }
        }
    }
}

/// Stale nodes (§8), in two kinds: `named`, when a newer record names the node itself, and `links`,
/// when it cites only what the node references. Named ones come first: they are the likelier to
/// call for a change rather than a note.
fn stale(store: &Store, p: &mut Report) {
    let (mut named, mut via_links) = (vec![], vec![]);
    for n in store.own(None) {
        let (mut by_name, mut by_links) = (vec![], vec![]);
        for cause in freshness(store, n) {
            let entry = format!("{} (cites {})", cause.record, cause.mentions.join(", "));
            if cause.subject == "named" { by_name.push(entry) } else { by_links.push(entry) }
        }
        let at = rel(&store.cfg.base, &n.path);
        if !by_name.is_empty() {
            named.push((at.clone(), format!("records newer than this node's last history line name it: {}", listed(by_name))));
        }
        if !by_links.is_empty() {
            via_links.push((at, format!("records newer than this node's last history line cite its links, not the node: {}", listed(by_links))));
        }
    }
    for (at, msg) in named {
        p.warn_on("stale", &at, "named", msg);
    }
    for (at, msg) in via_links {
        p.warn_on("stale", &at, "links", msg);
    }
}

/// A reference-field edge through which a newer source mentions a node's evidence.
/// This is the one-hop stale heuristic, not the commitment applicability graph (§12).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct StaleLink {
    pub field: String,
    pub id: String,
}

/// One readable source that makes a node potentially stale (§8).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StaleCause {
    pub record: String,
    pub rev: String,
    pub subject: &'static str,
    pub mentions: Vec<String>,
    pub via: Vec<StaleLink>,
}

/// The existing stale check for one node, without running unrelated checks, opening a library,
/// loading messages or writing a review. Only the access-filtered store participates.
pub fn freshness(store: &Store, n: &Record) -> Vec<StaleCause> {
    if !n.is_own() {
        return vec![];
    }
    let Some(last) = n.last_update() else { return vec![] };
    let refs = n.refs();
    let referenced: HashSet<&str> = refs.iter().map(|(_, id)| id.as_str()).collect();
    // `parent` is organisational (§3.3), and a commitment's `nodes` attaches its obligation (§12).
    // Neither makes news about that target a review of every child or attached commitment.
    // Explicit evidence fields still count, even when they name the same target as `nodes`.
    // A resolved record's links are history: only a record naming it directly (perhaps to reopen it) counts.
    let watched: Vec<_> = refs.iter()
        .filter(|(field, _)| !n.is_resolved() && *field != "parent" && !(n.is(OwnKind::Commitment) && *field == "nodes"))
        .collect();
    let mut causes = vec![];
    for r in store.linked().filter(|r| r.id != n.id && !referenced.contains(r.id.as_str()) && r.when().is_some_and(|d| d > last)) {
        let mentioned = store.mentions(&r.text);
        let named = mentioned.contains(&n.id);
        let mut via: Vec<_> = watched.iter().filter(|(_, id)| mentioned.contains(id)).map(|(field, id)| StaleLink { field: (*field).into(), id: id.clone() }).collect();
        if !named && via.is_empty() {
            continue;
        }
        via.sort();
        via.dedup();
        let mut mentions: BTreeSet<String> = via.iter().map(|link| link.id.clone()).collect();
        if named {
            mentions.insert(n.id.clone());
        }
        causes.push(StaleCause { record: r.id.clone(), rev: r.rev.clone(), subject: if named { "named" } else { "links" }, mentions: mentions.into_iter().collect(), via });
    }
    causes.sort_by(|a, b| (a.subject != "named", &a.record).cmp(&(b.subject != "named", &b.record)));
    causes
}

#[derive(Clone, Debug, Serialize)]
pub struct StaleAffected {
    pub id: String,
    pub path: String,
    pub mentions: Vec<String>,
    pub via: Vec<StaleLink>,
}

/// An optional presentation grouped by cause. The ordinary per-node diagnostics remain intact.
#[derive(Clone, Debug, Serialize)]
pub struct StaleGroup {
    pub record: String,
    pub rev: String,
    pub named: Vec<StaleAffected>,
    pub links: Vec<StaleAffected>,
}

pub fn stale_groups(store: &Store) -> Vec<StaleGroup> {
    let mut groups = BTreeMap::new();
    for n in store.own(None) {
        for cause in freshness(store, n) {
            let group = groups.entry(cause.record.clone()).or_insert_with(|| StaleGroup { record: cause.record, rev: cause.rev, named: vec![], links: vec![] });
            let affected = StaleAffected { id: n.id.clone(), path: rel(&store.cfg.base, &n.path), mentions: cause.mentions, via: cause.via };
            if cause.subject == "named" { group.named.push(affected) } else { group.links.push(affected) }
        }
    }
    groups.into_values().collect()
}

/// The first six entries, sorted, with an ellipsis for the rest.
fn listed(mut entries: Vec<String>) -> String {
    entries.sort();
    let more = if entries.len() > 6 { " …" } else { "" };
    entries.truncate(6);
    format!("{}{more}", entries.join("; "))
}

/// A superseded commitment's obligation lives on in its replacement, followed to the end of the
/// chain. While that replacement is unresolved, it should apply wherever the old one did (§12).
fn supersessions(store: &Store, p: &mut Report) {
    let attached = crate::review::attachments(store);
    let nodes: Vec<String> = store.own(None).into_iter().filter(|r| !r.is(OwnKind::Commitment)).map(|r| r.id.clone()).collect();
    for c in store.own(Some(OwnKind::Commitment)) {
        if c.state_str() != Some("superseded") {
            continue;
        }
        let mut seen = HashSet::from([c.id.clone()]);
        let first = doc::set(&c.meta, "superseded_by").map(node_ref);
        let mut next = first.clone();
        let holder = loop {
            let Some(t) = next.as_deref().and_then(|x| store.get(x)).filter(|t| t.is(OwnKind::Commitment)) else { break None };
            if !seen.insert(t.id.clone()) {
                break None; // a cycle, reported as an error
            }
            if t.state_str() != Some("superseded") {
                break Some(t);
            }
            next = doc::set(&t.meta, "superseded_by").map(node_ref);
        };
        // a concluded replacement is history, like any resolved record's links
        let Some(t) = holder.filter(|t| !t.is_resolved()) else { continue };
        let lost: Vec<String> = nodes
            .iter()
            .filter(|n| {
                let one = std::slice::from_ref(*n);
                !crate::review::chains(store, c, &attached, one).is_empty() && crate::review::chains(store, t, &attached, one).is_empty()
            })
            .cloned()
            .collect();
        if !lost.is_empty() {
            let via = if first.as_deref() == Some(t.id.as_str()) { "" } else { " (through superseded_by)" };
            p.warn(
                "superseded",
                &rel(&store.cfg.base, &c.path),
                format!("superseded by {}{via}, which does not apply to {}: the obligation no longer reaches them", t.id, listed(lost)),
            );
        }
    }
}

fn unrecorded_commitments(store: &Store, p: &mut Report) {
    let cfg = &store.cfg;
    let phrases: Vec<Pattern> = cfg.commitment_phrases.iter().filter_map(|x| Pattern::new(x, "(?i)").ok()).collect();
    let mut linked = HashSet::new();
    for c in store.own(Some(OwnKind::Commitment)) {
        linked.extend(c.meta.get("made").map(node_ref));
        linked.extend(c.ref_list("links"));
    }
    // a scan review covers the revision it read, so an edited record is scanned again
    let mut scanned: HashMap<String, Vec<(&str, String)>> = HashMap::new();
    for t in store.own(Some(OwnKind::Thread)) {
        for (rid, rev) in t.scanned() {
            scanned.entry(rid).or_default().push((&t.id, rev));
        }
    }
    let since = cfg.commitments_since.as_deref().and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()).and_then(|d| d.and_hms_opt(0, 0, 0)).map(|d| d.and_utc());
    for r in store.linked() {
        if !r.source().is_some_and(|s| s.scan_commitments) || linked.contains(&r.id) {
            continue;
        }
        if let Some(since) = since
            && r.when().is_none_or(|d| d < since)
        {
            continue;
        }
        let Some(hit) = phrases.iter().find(|x| x.search(&r.body)) else { continue };
        let reviews = scanned.get(&r.id).map(Vec::as_slice).unwrap_or_default();
        if reviews.iter().any(|(_, rev)| r.rev.starts_with(rev.as_str())) {
            continue;
        }
        let more = match reviews.last() {
            Some((t, rev)) => format!("; {t} records a scan of revision {}, and the record has changed since", doc::take(rev, 12)),
            None => format!(" (if it holds none, record that: `plied scanned {} --rev … --on <thread> …`)", r.id),
        };
        p.warn("commitment?", &r.id, format!("contains a conditional or deferral ({}) that no commitment links to{more}", hit.pattern));
    }
}

fn unlinked(store: &Store, p: &mut Report) {
    let mut cited = HashSet::new();
    for r in store.own(None) {
        cited.extend(r.refs().into_iter().map(|(_, rid)| rid));
        cited.extend(store.mentions(&r.body));
    }
    for r in store.linked() {
        if r.source().is_some_and(|s| s.require_linked) && !cited.contains(&r.id) {
            p.warn("unlinked", &r.id, "no plied record links to it");
        }
    }
}
