//! Records: plied's own (threads, commitments, questions) and those of linked sources (FORMAT.md §3, §4).

use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::LazyLock;

use chrono::{DateTime, NaiveDate, NaiveDateTime, Timelike, Utc};
use regex::Regex;

use crate::access::Policy;
use crate::config::{Config, Source};
use crate::doc::{self, Map, Node};
use crate::error::{Fail, Res};
use crate::fsx::{self, rel};
use crate::globpat;
use crate::yamlish;

macro_rules! states {
    ($(#[$m:meta])* $name:ident { $($v:ident = $s:literal),* $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $name { $($v),* }

        impl $name {
            pub const ALL: &'static [&'static str] = &[$($s),*];

            pub fn parse(s: &str) -> Option<Self> {
                match s { $($s => Some(Self::$v),)* _ => None }
            }

            pub fn as_str(self) -> &'static str {
                match self { $(Self::$v => $s),* }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

states!(
    /// plied's own record kinds.
    OwnKind { Thread = "thread", Commitment = "commitment", Question = "question" }
);
states!(ThreadStatus { Open = "open", Waiting = "waiting", Held = "held", Parked = "parked", Closed = "closed", Group = "group" });
states!(CommitmentState {
    Open = "open",
    Due = "due",
    Fulfilled = "fulfilled",
    Waived = "waived",
    Missed = "missed",
    NotTriggered = "not-triggered",
});
states!(QuestionState { Open = "open", Answered = "answered", Withdrawn = "withdrawn" });
states!(Disposition { Satisfied = "satisfied", Acting = "acting", NotTriggered = "not-triggered", NotApplicable = "not-applicable", Blocks = "blocks" });
states!(Applies { Linked = "linked", Subtree = "subtree", Project = "project" });

impl OwnKind {
    pub const KINDS: [OwnKind; 3] = [OwnKind::Thread, OwnKind::Commitment, OwnKind::Question];

    pub fn name(self) -> &'static str {
        self.as_str()
    }

    /// Allowed fields, in canonical order.
    pub fn fields(self) -> &'static [&'static str] {
        match self {
            OwnKind::Thread => &[
                "id", "title", "parent", "status", "next", "actor", "due", "waiting_on", "uses", "approval", "outcome", "carries",
                "closes_when", "revisit", "revisit_on", "backlog", "links", "cites",
            ],
            OwnKind::Commitment => &[
                "id", "title", "wording", "summary", "made", "trigger", "state", "due", "applies", "nodes", "resolved_by", "links",
                "cites", "reviews",
            ],
            OwnKind::Question => &["id", "title", "options", "raised", "state", "due", "nodes", "answered_by", "answer", "links", "cites"],
        }
    }

    /// Reference-bearing fields (dotted for nested ones).
    pub fn ref_fields(self) -> &'static [&'static str] {
        match self {
            OwnKind::Thread => &["parent", "waiting_on", "uses", "carries", "backlog", "links", "approval.by", "outcome.ref"],
            OwnKind::Commitment => &["made", "nodes", "resolved_by", "links"],
            OwnKind::Question => &["raised", "nodes", "answered_by", "links"],
        }
    }

    pub fn state_field(self) -> &'static str {
        if self == OwnKind::Thread { "status" } else { "state" }
    }

    pub fn is_resolved(self, state: &str) -> bool {
        match self {
            OwnKind::Thread => state == "closed",
            OwnKind::Commitment => matches!(state, "fulfilled" | "waived" | "missed" | "not-triggered"),
            OwnKind::Question => matches!(state, "answered" | "withdrawn"),
        }
    }
}

pub const LIST_FIELDS: [&str; 8] = ["waiting_on", "uses", "carries", "links", "nodes", "options", "cites", "reviews"];

static HISTORY_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^- (\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}) UTC\b(.*)$").unwrap());
static REF_END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\s,;]").unwrap());
static WORDS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[a-z0-9]+").unwrap());

/// The date fields, on the kinds that have them.
pub const DATE_FIELDS: [&str; 2] = ["due", "revisit_on"];

/// A calendar date, `YYYY-MM-DD` exactly.
pub fn parse_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    (s.len() == 10 && s.as_bytes()[4] == b'-' && s.as_bytes()[7] == b'-').then(|| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()).flatten()
}

/// The id in a reference: 'D134 §2' -> 'D134' (§3.4).
pub fn ref_id(r: &str) -> String {
    let s = r.trim();
    let first = REF_END.split(s).next().unwrap_or("");
    first.trim_end_matches(['.', ':', ')']).to_string()
}

/// The id in a reference held by a frontmatter value (empty for collections).
pub fn node_ref(n: &Node) -> String {
    n.scalar().map(|s| ref_id(&s)).unwrap_or_default()
}

/// The current minute, UTC.
pub fn now_minute() -> DateTime<Utc> {
    let n = Utc::now();
    n.with_second(0).and_then(|n| n.with_nanosecond(0)).unwrap_or(n)
}

pub fn stamp() -> String {
    now_minute().format("%Y-%m-%d %H:%M UTC").to_string()
}

pub fn slugify(title: &str) -> String {
    let low = title.to_lowercase();
    let words: Vec<&str> = WORDS.find_iter(&low).map(|m| m.as_str()).take(6).collect();
    if words.is_empty() { "item".into() } else { words.join("-") }
}

fn parse_minute(s: &str) -> Option<DateTime<Utc>> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").ok().map(|d| d.and_utc())
}

#[derive(Clone)]
pub enum Kind {
    Own(OwnKind),
    Linked(Rc<Source>),
}

impl Kind {
    pub fn name(&self) -> &str {
        match self {
            Kind::Own(k) => k.name(),
            Kind::Linked(s) => &s.name,
        }
    }
}

pub struct Record {
    pub kind: Kind,
    pub id: String,
    pub path: PathBuf,
    pub meta: Map,
    pub body: String,
    pub text: String,
    pub rev: String,
    pub parse_error: Option<String>,
}

impl Record {
    pub fn own(&self) -> Option<OwnKind> {
        match self.kind {
            Kind::Own(k) => Some(k),
            Kind::Linked(_) => None,
        }
    }

    pub fn is_own(&self) -> bool {
        self.own().is_some()
    }

    pub fn is(&self, kind: OwnKind) -> bool {
        self.own() == Some(kind)
    }

    pub fn source(&self) -> Option<&Source> {
        match &self.kind {
            Kind::Linked(s) => Some(s),
            Kind::Own(_) => None,
        }
    }

    pub fn title(&self) -> String {
        let f = self.source().map_or("title", |s| s.title_field.as_str());
        doc::text(&self.meta, f).unwrap_or_default()
    }

    /// The status or state field's value.
    pub fn state(&self) -> Option<&Node> {
        let f = match &self.kind {
            Kind::Own(k) => k.state_field(),
            Kind::Linked(s) => s.status_field.as_deref()?,
        };
        self.meta.get(f)
    }

    pub fn state_str(&self) -> Option<&str> {
        self.state().and_then(Node::str)
    }

    pub fn status(&self) -> Option<ThreadStatus> {
        self.state_str().and_then(ThreadStatus::parse)
    }

    /// Resolved (§3.3): concluded own records, and every linked record.
    pub fn is_resolved(&self) -> bool {
        match self.own() {
            None => true,
            Some(k) => self.state_str().is_some_and(|s| k.is_resolved(s)),
        }
    }

    /// A date field (`due`, `revisit_on`) as a date; None when absent or not `YYYY-MM-DD` (§3.3).
    pub fn date(&self, field: &str) -> Option<NaiveDate> {
        parse_date(&doc::text(&self.meta, field)?)
    }

    /// When a date field asks for attention: `due` once the day has passed on an unresolved record,
    /// `revisit_on` from that day on a `parked` or `held` thread. Advisory only (§3.3).
    pub fn date_passed(&self, field: &str, today: NaiveDate) -> Option<bool> {
        let d = self.date(field)?;
        Some(match field {
            "revisit_on" => d <= today && matches!(self.status(), Some(ThreadStatus::Parked | ThreadStatus::Held)),
            _ => d < today && !self.is_resolved(),
        })
    }

    pub fn get(&self, dotted: &str) -> Option<&Node> {
        doc::get(&self.meta, dotted)
    }

    /// The ids referenced by a list field.
    pub fn ref_list(&self, field: &str) -> Vec<String> {
        self.meta.get(field).map(|v| v.items().iter().map(node_ref).filter(|r| !r.is_empty()).collect()).unwrap_or_default()
    }

    /// (field, referenced id) for every reference in the frontmatter.
    pub fn refs(&self) -> Vec<(&'static str, String)> {
        let Some(kind) = self.own() else { return vec![] };
        let mut out = vec![];
        for f in kind.ref_fields() {
            let vals = match self.get(f) {
                Some(Node::List(items)) => items.iter().collect(),
                Some(v) => vec![v],
                None => vec![],
            };
            for rid in vals.into_iter().map(node_ref).filter(|r| !r.is_empty()) {
                out.push((*f, rid));
            }
        }
        out
    }

    pub fn history(&self) -> Vec<(DateTime<Utc>, String)> {
        HISTORY_RE
            .captures_iter(&self.body)
            .filter_map(|m| parse_minute(&format!("{} {}", &m[1], &m[2])).map(|d| (d, m[3].trim().to_string())))
            .collect()
    }

    pub fn last_update(&self) -> Option<DateTime<Utc>> {
        self.history().into_iter().map(|(d, _)| d).max()
    }

    /// Date (and time) of a linked record, when its source declares them.
    pub fn when(&self) -> Option<DateTime<Utc>> {
        let src = self.source()?;
        let d = doc::text(&self.meta, src.date_field.as_ref()?)?;
        let t = src.time_field.as_ref().and_then(|f| doc::text(&self.meta, f)).unwrap_or_else(|| "00:00".into());
        parse_minute(&format!("{} {}", d.trim(), doc::take(t.trim(), 5)))
    }

    /// The record as a JSON object: frontmatter plus what plied knows about it.
    pub fn to_json(&self, base: &Path) -> serde_json::Map<String, serde_json::Value> {
        use serde_json::Value;
        let mut o = doc::map_to_json(&self.meta);
        o.insert("id".into(), Value::from(self.id.clone()));
        o.insert("kind".into(), Value::from(self.kind.name()));
        o.insert("title".into(), Value::from(self.title()));
        o.insert("state".into(), self.state().map_or(Value::Null, Node::to_json));
        o.insert("path".into(), Value::from(rel(base, &self.path)));
        o.insert("rev".into(), Value::from(self.rev.clone()));
        o.insert("body".into(), Value::from(self.body.clone()));
        o.insert("text".into(), Value::from(self.text.clone()));
        o.insert("own".into(), Value::from(self.is_own()));
        o
    }
}

/// Canonical record text (§3.2).
pub fn render(kind: OwnKind, meta: &Map, body: &str) -> String {
    let kept: Map = meta.iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| (k.clone(), v.clone())).collect();
    yamlish::join_frontmatter(&yamlish::dump(&kept, kind.fields()), body)
}

fn digits_to_int(v: &mut Node) {
    if let Node::Str(s) = v
        && !s.trim().is_empty()
        && s.trim().bytes().all(|b| b.is_ascii_digit())
        && let Ok(n) = s.trim().parse()
    {
        *v = Node::Int(n);
    }
}

/// Drop empty values (null, "", [], {}) from mappings, innermost first: an empty value reads as absent.
fn prune(meta: &mut Map) {
    for v in meta.values_mut() {
        match v {
            Node::Map(m) => prune(m),
            Node::List(items) => items.iter_mut().filter_map(|it| if let Node::Map(m) = it { Some(m) } else { None }).for_each(prune),
            _ => {}
        }
    }
    meta.retain(|_, v| !v.is_empty());
}

/// Read the typed fields of an own record (§3.1, §3.3): empty values as absent, list fields and integers.
pub fn coerce(mut meta: Map) -> Map {
    prune(&mut meta);
    for k in LIST_FIELDS {
        if let Some(v) = meta.get_mut(k)
            && !matches!(v, Node::Null | Node::List(_))
        {
            *v = Node::List(vec![std::mem::replace(v, Node::Null)]);
        }
    }
    if let Some(n) = meta.get_mut("next") {
        digits_to_int(n);
    }
    if let Some(Node::List(cites)) = meta.get_mut("cites") {
        for c in cites {
            if let Node::Map(c) = c
                && let Some(p) = c.get_mut("page")
            {
                digits_to_int(p);
            }
        }
    }
    meta
}

/// Decode a record file (FORMAT.md §3): a leading byte-order mark is ignored and CRLF reads as LF.
/// The revision is always the hash of the raw bytes. Invalid UTF-8 is replaced, and reported.
fn decode(data: &[u8]) -> (String, Option<String>) {
    let bad = std::str::from_utf8(data).err().map(|e| format!("not valid UTF-8 (at byte {})", e.valid_up_to()));
    let text = String::from_utf8_lossy(data);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    (text.replace("\r\n", "\n"), bad)
}

pub fn parse_bytes(data: &[u8], path: &Path, kind: Kind) -> Record {
    let (text, bad_utf8) = decode(data);
    let own = matches!(kind, Kind::Own(_));
    let (mut parse_error, mut meta, mut body) = (None, Map::new(), text.clone());
    match yamlish::split_frontmatter(&text) {
        Ok((None, b)) => {
            body = b;
            parse_error = Some("no frontmatter".to_string());
        }
        Ok((Some(fm), b)) => {
            body = b;
            match yamlish::parse(&fm) {
                Ok(m) => meta = m,
                Err(e) if own => parse_error = Some(format!("frontmatter: {e}")),
                Err(_) => meta = yamlish::parse_lenient(&fm),
            }
        }
        Err(e) => parse_error = Some(e.to_string()),
    }
    let id_field = match &kind {
        Kind::Linked(s) => s.id_field.as_str(),
        Kind::Own(_) => "id",
    };
    let id = doc::text(&meta, id_field).map(|s| s.trim().to_string()).unwrap_or_default();
    if own {
        meta = coerce(meta);
        // a linked source is read as best we can; plied's own records must be text plied can rewrite
        parse_error = bad_utf8.or(parse_error);
    }
    Record { kind, id, path: path.to_path_buf(), meta, body, text, rev: fsx::sha(data), parse_error }
}

/// Proposed contents by path (None: the file is absent), for validating a write.
pub type Overrides = HashMap<PathBuf, Option<Vec<u8>>>;

pub struct Store {
    pub cfg: Rc<Config>,
    /// accepted records, in load order
    pub records: Vec<Record>,
    index: HashMap<String, usize>,
    /// (kind, id) of every own file parsed, duplicates and bad ids included
    file_ids: Vec<(OwnKind, String)>,
    /// (code, where, message)
    pub load_problems: Vec<(&'static str, String, String)>,
    /// kinds and sources with files the access policy kept unread
    sealed_kinds: HashSet<String>,
    token: OnceCell<fancy_regex::Regex>,
}

impl Store {
    /// Load all records. Files the access policy refuses (sealed, or resolving outside the
    /// repository) are never read: they are not records, so nothing shows them (§9).
    pub fn load(cfg: &Rc<Config>, overrides: Option<&Overrides>, policy: &Policy) -> Res<Store> {
        let mut st = Store {
            cfg: cfg.clone(),
            records: vec![],
            index: HashMap::new(),
            file_ids: vec![],
            load_problems: vec![],
            sealed_kinds: HashSet::new(),
            token: OnceCell::new(),
        };
        let ov: Overrides = overrides.map(|o| o.iter().map(|(p, v)| (fsx::resolve(p), v.clone())).collect()).unwrap_or_default();
        for kind in OwnKind::KINDS {
            let d = cfg.kind_dir(kind);
            let dres = fsx::resolve(&d);
            let mut paths: Vec<PathBuf> = md_files(&d).iter().map(|p| fsx::resolve(p)).collect();
            paths.extend(ov.keys().filter(|p| p.parent() == Some(dres.as_path())).cloned());
            paths.sort();
            paths.dedup();
            for p in paths {
                if matches!(ov.get(&p), Some(None)) {
                    continue;
                }
                if !policy.readable(&p) {
                    st.sealed_kinds.insert(kind.name().into());
                    continue;
                }
                let rec = read_record(&p, Kind::Own(kind), &ov)?;
                st.add(rec);
            }
        }
        for src in &cfg.sources {
            let mut names = globpat::glob(&cfg.base, &src.glob);
            names.sort();
            for name in names {
                let joined = cfg.base.join(&name);
                if !policy.readable(&joined) {
                    st.sealed_kinds.insert(src.name.clone());
                    continue;
                }
                let p = fsx::resolve(&joined);
                if !p.is_file() {
                    continue;
                }
                let rec = read_record(&p, Kind::Linked(src.clone()), &ov)?;
                if rec.id.is_empty() {
                    continue; // generated indexes, templates: not records
                }
                if !src.id_re.is_match_start(&rec.id) {
                    st.load_problems.push(("source-id", rel(&cfg.base, &p), format!("id {:?} does not match {}", rec.id, src.id_re.pattern)));
                    continue;
                }
                st.add(rec);
            }
        }
        Ok(st)
    }

    fn add(&mut self, rec: Record) {
        let w = rel(&self.cfg.base, &rec.path);
        if let Some(kind) = rec.own() {
            self.file_ids.push((kind, rec.id.clone()));
            if let Some(e) = &rec.parse_error {
                self.load_problems.push(("parse", w.clone(), e.clone()));
            }
            if rec.id.is_empty() {
                self.load_problems.push(("id", w, "missing id".into()));
                return;
            }
            let k = self.cfg.kind(kind);
            if !k.id_re.is_match(&rec.id) {
                let msg = format!("id {:?} is not a {kind} id ({}{})", rec.id, k.prefix, "n".repeat(k.width));
                self.load_problems.push(("id", w, msg));
                return;
            }
            let stem = fsx::stem(&rec.path);
            if !(stem == rec.id || stem.starts_with(&format!("{}-", rec.id))) {
                self.load_problems.push(("id", w.clone(), format!("file name does not start with its id {}", rec.id)));
            }
        }
        if let Some(&i) = self.index.get(&rec.id) {
            let other = rel(&self.cfg.base, &self.records[i].path);
            self.load_problems.push(("duplicate", w, format!("id {} also used by {other}", rec.id)));
            return;
        }
        self.index.insert(rec.id.clone(), self.records.len());
        self.records.push(rec);
    }

    // ------------------------------------------------------------------------------ access

    /// The record a reference names.
    pub fn get(&self, reference: &str) -> Option<&Record> {
        self.index.get(&ref_id(reference)).map(|&i| &self.records[i])
    }

    pub fn contains(&self, id: &str) -> bool {
        self.index.contains_key(id)
    }

    /// Own records of a kind (or all), by id.
    pub fn own(&self, kind: Option<OwnKind>) -> Vec<&Record> {
        let mut v: Vec<&Record> = self.records.iter().filter(|r| r.own().is_some_and(|k| kind.is_none_or(|w| w == k))).collect();
        v.sort_by(|a, b| a.id.cmp(&b.id));
        v
    }

    pub fn linked(&self) -> impl Iterator<Item = &Record> {
        self.records.iter().filter(|r| !r.is_own())
    }

    pub fn is_resolved(&self, id: &str) -> bool {
        self.get(id).is_some_and(Record::is_resolved)
    }

    pub fn next_id(&self, kind: OwnKind) -> String {
        let k = self.cfg.kind(kind);
        let mut nums: Vec<u64> = self
            .file_ids
            .iter()
            .filter(|(kk, id)| *kk == kind && k.id_re.is_match(id))
            .filter_map(|(_, id)| id[k.prefix.len()..].parse().ok())
            .collect();
        // also never reuse ids of files that exist but failed to parse
        let re = Regex::new(&format!(r"^{}(\d+)", regex::escape(&k.prefix))).expect("escaped prefix");
        for p in md_files(&self.cfg.kind_dir(kind)) {
            if let Some(n) = re.captures(&fsx::stem(&p)).and_then(|m| m[1].parse().ok()) {
                nums.push(n);
            }
        }
        k.format_id(nums.into_iter().max().unwrap_or(0) + 1)
    }

    /// Whether a missing id could be a record the access policy kept unread.
    pub fn maybe_sealed(&self, id: &str) -> bool {
        self.sealed_kinds.iter().any(|name| match OwnKind::parse(name) {
            Some(k) => self.cfg.kind(k).id_re.is_match(id),
            None => self.cfg.sources.iter().any(|s| &s.name == name && s.id_re.is_match_start(id)),
        })
    }

    /// Matches any id of any kind or source in running text.
    pub fn token_re(&self) -> &fancy_regex::Regex {
        self.token.get_or_init(|| {
            let mut pats: Vec<String> = self.cfg.kinds.iter().map(|k| format!(r"{}\d{{{},}}", fancy_regex::escape(&k.prefix), k.width)).collect();
            for s in &self.cfg.sources {
                let p = &s.id_re.pattern;
                let p = p.strip_prefix('^').unwrap_or(p);
                pats.push(p.strip_suffix('$').unwrap_or(p).to_string());
            }
            let alt = pats.iter().map(|p| format!("(?:{p})")).collect::<Vec<_>>().join("|");
            fancy_regex::Regex::new(&format!(r"(?<!\w)(?:{alt})(?!\w)")).unwrap_or_else(|_| fancy_regex::Regex::new(r"(?!)").unwrap())
        })
    }

    /// Every id mentioned in `text`.
    pub fn mentions(&self, text: &str) -> HashSet<String> {
        self.token_re().find_iter(text).filter_map(Result::ok).map(|m| m.as_str().to_string()).collect()
    }
}

fn md_files(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "md") && p.is_file()).collect())
        .unwrap_or_default()
}

fn read_record(p: &Path, kind: Kind, ov: &Overrides) -> Res<Record> {
    if let Some(Some(data)) = ov.get(p) {
        return Ok(parse_bytes(data, p, kind));
    }
    let data = std::fs::read(p).map_err(|e| Fail::new(format!("{}: {e}", p.display())))?;
    Ok(parse_bytes(&data, p, kind))
}
