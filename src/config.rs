//! Project configuration: `plied.toml` at the top of the canonical checkout.
//!
//! Everything project-specific (id formats of other record systems, sealed paths, the literature
//! manifest, who may approve) lives here, never in the program. Every field is type-checked and
//! every pattern compiled at load, so a slip in a hand-edited file is an error, not a quietly
//! different policy. The file is part of the trust boundary: `literature.image_command` runs
//! whatever it names.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use regex::Regex;
use toml::{Table, Value as T};

use crate::error::{Fail, Res};
use crate::fsx;
use crate::records::OwnKind;

pub const CONFIG_NAME: &str = "plied.toml";

pub struct Kind {
    pub kind: OwnKind,
    pub dir: String,
    pub prefix: String,
    pub width: usize,
    pub id_re: Regex,
}

impl Kind {
    pub fn format_id(&self, n: u64) -> String {
        format!("{}{:0w$}", self.prefix, n, w = self.width)
    }
}

/// A regular expression from plied.toml: found anywhere (`search`) or matched at the start.
pub struct Pattern {
    pub pattern: String,
    search: fancy_regex::Regex,
    anchored: fancy_regex::Regex,
}

impl Pattern {
    pub fn new(pattern: &str, flags: &str) -> Result<Self, String> {
        let search = fancy_regex::Regex::new(&format!("{flags}{pattern}")).map_err(|e| e.to_string())?;
        let anchored = fancy_regex::Regex::new(&format!(r"{flags}\A(?:{pattern})")).map_err(|e| e.to_string())?;
        Ok(Pattern { pattern: pattern.to_string(), search, anchored })
    }

    pub fn is_match_start(&self, s: &str) -> bool {
        self.anchored.is_match(s).unwrap_or(false)
    }

    pub fn search(&self, s: &str) -> bool {
        self.search.is_match(s).unwrap_or(false)
    }

    pub fn groups(&self) -> usize {
        self.search.captures_len() - 1
    }

    /// `re.search(p, s).group(1)`
    pub fn group1(&self, s: &str) -> Option<String> {
        let c = self.search.captures(s).ok()??;
        c.get(1).map(|m| m.as_str().to_string())
    }
}

/// Another record system whose records plied links to but never writes.
pub struct Source {
    pub name: String,
    pub glob: String,
    pub id_re: Pattern,
    pub id_field: String,
    pub title_field: String,
    pub date_field: Option<String>,
    pub time_field: Option<String>,
    pub status_field: Option<String>,
    pub authority_field: Option<String>,
    pub scope_field: Option<String>,
    pub scan_commitments: bool,
    pub require_linked: bool,
}

#[derive(Clone)]
pub struct Literature {
    pub manifest: String,
    pub list_key: String,
    pub file_field: String,
    pub hash_field: String,
    pub id_field: Option<String>,
    pub id_from_file: Option<String>,
    pub pdf_dir: Option<String>,
    pub text_dir: Option<String>,
    pub page_file: String,
    pub ocr_marker: Option<String>,
    pub image_command: Option<Vec<String>>,
    pub image_path: Option<String>,
}

/// The on-disk formats this version reads and writes (FORMAT.md, *Versions*).
pub const FORMATS: &[i64] = &[1];

/// `[plied] requires`: which plied versions may write this record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Requires {
    /// `"X.Y.Z"`: exactly this version
    Exact((u64, u64, u64)),
    /// `">=X.Y.Z"`: this version or a later one
    AtLeast((u64, u64, u64)),
}

fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let mut it = v.split('.').map(|p| if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) { p.parse().ok() } else { None });
    let out = (it.next()??, it.next()??, it.next()??);
    it.next().is_none().then_some(out)
}

impl Requires {
    pub fn parse(s: &str) -> Option<Requires> {
        match s.trim().strip_prefix(">=") {
            Some(v) => parse_version(v.trim()).map(Requires::AtLeast),
            None => parse_version(s.trim()).map(Requires::Exact),
        }
    }

    pub fn allows(&self, version: &str) -> bool {
        let Some(v) = parse_version(version) else { return false };
        match self {
            Requires::Exact(r) => v == *r,
            Requires::AtLeast(r) => v >= *r,
        }
    }
}

impl std::fmt::Display for Requires {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Requires::Exact((a, b, c)) => write!(f, "{a}.{b}.{c}"),
            Requires::AtLeast((a, b, c)) => write!(f, ">={a}.{b}.{c}"),
        }
    }
}

pub struct Config {
    pub path: PathBuf,
    pub base: PathBuf, // the canonical checkout; record paths are relative to it
    pub raw_bytes: Vec<u8>,
    pub format: i64,
    pub requires: Option<Requires>,
    pub root: String,
    pub canonical_branch: Option<String>,
    pub kinds: Vec<Kind>,
    pub sources: Vec<Rc<Source>>,
    pub sealed_file: Option<String>,
    pub sealed: Vec<String>,
    pub search_include: Vec<String>,
    pub search_exclude: Vec<String>,
    pub literature: Option<Literature>,
    pub approve_authorities: Vec<String>,
    pub actors: Option<Vec<String>>,
    pub commitment_phrases: Vec<String>,
    pub commitments_since: Option<String>,
    pub lease_minutes: i64,
    /// action kinds a review may name (`kind` or `kind:instance`); None: any
    pub action_kinds: Option<Vec<String>>,
    /// days a cached revision's content is kept after it was last read or written
    pub revision_days: i64,
    pub hard_exclude: Vec<String>,
}

impl Config {
    pub fn digest(&self) -> String {
        fsx::sha(&self.raw_bytes)
    }

    /// Why this plied may not write this record, if it may not: an unknown format, or a version
    /// `requires` rules out. Each is (subject, message), for refusals and for `check` (§8).
    pub fn compat_problems(&self) -> Vec<(&'static str, String)> {
        let mut out = vec![];
        if !FORMATS.contains(&self.format) {
            out.push(("format", format!("this record is in format {}, but plied {} knows format {}", self.format, crate::VERSION, FORMATS.iter().map(i64::to_string).collect::<Vec<_>>().join(", "))));
        }
        if let Some(r) = self.requires.as_ref().filter(|r| !r.allows(crate::VERSION)) {
            out.push(("requires", format!("this record requires plied {r}, but plied {} is running", crate::VERSION)));
        }
        out
    }

    /// The kind of an action name: the text before its first `:`.
    pub fn action_kind(action: &str) -> &str {
        action.split(':').next().unwrap_or(action)
    }

    /// Why `action` is not allowed by `checks.action_kinds`, if it is not.
    pub fn action_problem(&self, action: &str) -> Option<String> {
        let kinds = self.action_kinds.as_ref()?;
        let kind = Config::action_kind(action);
        (!kinds.iter().any(|k| k == kind)).then(|| format!("action {action:?} has kind {kind:?}, not one of checks.action_kinds ({})", kinds.join(", ")))
    }

    pub fn kind(&self, kind: OwnKind) -> &Kind {
        self.kinds.iter().find(|k| k.kind == kind).expect("every kind is configured")
    }

    pub fn kind_dir(&self, kind: OwnKind) -> PathBuf {
        self.base.join(&self.root).join(&self.kind(kind).dir)
    }

    pub fn messages_dir(&self) -> PathBuf {
        self.base.join(&self.root).join("messages")
    }

    pub fn view_path(&self) -> PathBuf {
        self.base.join(&self.root).join("README.md")
    }

    /// Repo-relative paths only plied may change (directories end with '/').
    pub fn managed_paths(&self) -> Vec<String> {
        let r = self.root.trim_end_matches('/');
        let mut out: Vec<String> = self.kinds.iter().map(|k| format!("{r}/{}/", k.dir)).collect();
        out.push(format!("{r}/messages/"));
        out.push(format!("{r}/README.md"));
        out
    }
}

const DEFAULT_KINDS: [(OwnKind, &str, &str, i64); 3] =
    [(OwnKind::Thread, "threads", "T", 3), (OwnKind::Commitment, "commitments", "P", 2), (OwnKind::Question, "questions", "Q", 2)];

const DEFAULT_PHRASES: [&str; 4] = [r"\bdeferred\b", r"\bpending\b", r"\bif\b[^.]{0,80}\bthen\b", r"\bbefore\b[^.]{0,40}\bany\b"];

// ------------------------------------------------------------------------ typed access

fn err(msg: impl Into<String>) -> Fail {
    Fail::new(msg)
}

fn got(v: &T) -> String {
    v.to_string()
}

fn get_str(d: &Table, key: &str, default: Option<&str>, w: &str) -> Res<Option<String>> {
    match d.get(key) {
        None => Ok(default.map(String::from)),
        Some(T::String(s)) => Ok(Some(s.clone())),
        Some(v) => Err(err(format!("{w}{key}: expected a string, got {}", got(v)))),
    }
}

fn get_bool(d: &Table, key: &str, default: bool, w: &str) -> Res<bool> {
    match d.get(key) {
        None => Ok(default),
        Some(T::Boolean(b)) => Ok(*b),
        Some(v) => Err(err(format!("{w}{key}: expected true or false, got {}", got(v)))),
    }
}

fn get_positive(d: &Table, key: &str, default: i64, w: &str) -> Res<i64> {
    let v = match d.get(key) {
        None => default,
        Some(T::Integer(i)) => *i,
        Some(v) => return Err(err(format!("{w}{key}: expected an integer, got {}", got(v)))),
    };
    if v < 1 {
        return Err(err(format!("{w}{key}: expected a positive integer, got {v}")));
    }
    Ok(v)
}

fn get_table<'a>(d: &'a Table, key: &str, w: &str) -> Res<std::borrow::Cow<'a, Table>> {
    match d.get(key) {
        None => Ok(std::borrow::Cow::Owned(Table::new())),
        Some(T::Table(t)) => Ok(std::borrow::Cow::Borrowed(t)),
        Some(v) => Err(err(format!("{w}{key}: expected a table, got {}", got(v)))),
    }
}

/// A list of strings; a bare string is refused, never split into characters.
fn get_strs(d: &Table, key: &str, default: Option<Vec<String>>, w: &str) -> Res<Option<Vec<String>>> {
    match d.get(key) {
        None => Ok(default),
        Some(T::Array(a)) if a.iter().all(|x| matches!(x, T::String(_))) => {
            Ok(Some(a.iter().filter_map(|x| x.as_str().map(String::from)).collect()))
        }
        Some(v) => Err(err(format!("{w}{key}: expected a list of strings, got {}", got(v)))),
    }
}

fn pattern(p: &str, flags: &str, what: &str) -> Res<Pattern> {
    Pattern::new(p, flags).map_err(|e| err(format!("{what}: invalid regular expression {p:?}: {e}")))
}

// ------------------------------------------------------------------------ templates

pub enum Arg<'a> {
    S(&'a str),
    I(i64),
}

/// Path templates (FORMAT.md §2.1): `{name}` or `{name:spec}`, with `{{` and `}}` for braces.
pub fn render_template(t: &str, args: &[(&str, Arg)]) -> Result<String, String> {
    let mut out = String::new();
    let mut it = t.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '{' if it.peek() == Some(&'{') => {
                it.next();
                out.push('{');
            }
            '}' if it.peek() == Some(&'}') => {
                it.next();
                out.push('}');
            }
            '}' => return Err("Single '}' encountered in format string".into()),
            '{' => {
                let mut field = String::new();
                loop {
                    match it.next() {
                        Some('}') => break,
                        Some(ch) => field.push(ch),
                        None => return Err("expected '}' before end of string".into()),
                    }
                }
                let (name, spec) = field.split_once(':').unwrap_or((&field, ""));
                let arg = args.iter().find(|(n, _)| *n == name).map(|(_, a)| a).ok_or_else(|| format!("unknown field {name:?}"))?;
                out.push_str(&format_spec(arg, spec)?);
            }
            c => out.push(c),
        }
    }
    Ok(out)
}

fn format_spec(arg: &Arg, spec: &str) -> Result<String, String> {
    let ch: Vec<char> = spec.chars().collect();
    let (mut fill, mut align, mut i) = (' ', None, 0);
    if ch.len() >= 2 && "<>^=".contains(ch[1]) {
        fill = ch[0];
        align = Some(ch[1]);
        i = 2;
    } else if !ch.is_empty() && "<>^=".contains(ch[0]) {
        align = Some(ch[0]);
        i = 1;
    }
    if i < ch.len() && ch[i] == '0' {
        if align.is_none() {
            fill = '0';
            align = Some('=');
        }
        i += 1;
    }
    let mut width = 0usize;
    while i < ch.len() && ch[i].is_ascii_digit() {
        width = width * 10 + ch[i].to_digit(10).unwrap_or(0) as usize;
        i += 1;
    }
    let ty = ch.get(i).copied();
    if ch.len() > i + 1 || ty.is_some_and(|t| !"ds".contains(t)) {
        return Err(format!("invalid format spec {spec:?}"));
    }
    let (text, num) = match arg {
        Arg::S(s) if ty != Some('d') && align != Some('=') => (s.to_string(), false),
        Arg::I(n) if ty != Some('s') => (n.to_string(), true),
        _ => return Err(format!("format spec {spec:?} does not fit this field")),
    };
    let n = text.chars().count();
    if n >= width {
        return Ok(text);
    }
    let pad = |k: usize| fill.to_string().repeat(k);
    Ok(match align.unwrap_or(if num { '>' } else { '<' }) {
        '<' => format!("{text}{}", pad(width - n)),
        '^' => format!("{}{text}{}", pad((width - n) / 2), pad(width - n - (width - n) / 2)),
        '=' if text.starts_with('-') => format!("-{}{}", pad(width - n), &text[1..]),
        _ => format!("{}{text}", pad(width - n)),
    })
}

fn template(t: Option<String>, what: &str, args: &[(&str, Arg)]) -> Res<Option<String>> {
    if let Some(ref s) = t
        && let Err(e) = render_template(s, args)
    {
        let names: Vec<&str> = args.iter().map(|(n, _)| *n).collect();
        return Err(err(format!("{what}: bad template {s:?} (fields: {}): {e}", names.join(", "))));
    }
    Ok(t)
}

// ---------------------------------------------------------------------------------- load

/// Every table and key plied.toml may hold. Anything else is an error, so a misspelt key (`seald`)
/// cannot silently leave its default in force.
const KEYS: &[(&str, &[&str])] = &[
    ("plied", &["format", "requires", "root", "canonical_branch"]),
    ("ids", &["thread", "commitment", "question"]),
    ("sources", &["name", "glob", "id", "id_field", "title_field", "date", "time", "status", "authority", "scope", "scan_commitments", "require_linked"]),
    ("access", &["sealed", "sealed_file"]),
    ("search", &["include", "exclude"]),
    ("literature", &["manifest", "list", "file_field", "hash_field", "id_field", "id_from_file", "pdf_dir", "text_dir", "page_file", "ocr_marker", "image_command", "image_path"]),
    ("checks", &["approve_authorities", "actors", "commitment_phrases", "commitments_since", "action_kinds"]),
    ("messages", &["lease_minutes"]),
    ("runtime", &["revision_days"]),
];
const ID_KEYS: &[&str] = &["dir", "prefix", "width"];

fn known_keys(data: &Table) -> Res<()> {
    let unknown = |w: &str, k: &str| err(format!("{w}{k}: not a plied.toml setting (a misspelling?)"));
    for (table, value) in data {
        let Some((_, keys)) = KEYS.iter().find(|(t, _)| t == table) else { return Err(unknown("", table)) };
        let tables: Vec<(String, &Table)> = match value {
            T::Table(t) => vec![(format!("{table}."), t)],
            T::Array(a) => a.iter().enumerate().filter_map(|(i, x)| x.as_table().map(|t| (format!("{table}[{i}]."), t))).collect(),
            _ => vec![],
        };
        for (w, t) in tables {
            for (k, v) in t {
                if !keys.contains(&k.as_str()) {
                    return Err(unknown(&w, k));
                }
                if *table == "ids"
                    && let Some(sub) = v.as_table()
                    && let Some(bad) = sub.keys().find(|sk| !ID_KEYS.contains(&sk.as_str()))
                {
                    return Err(unknown(&format!("{w}{k}."), bad));
                }
            }
        }
    }
    Ok(())
}

pub fn load(path: &Path, base: Option<&Path>) -> Res<Config> {
    let raw = std::fs::read(path).map_err(|e| err(format!("{}: {e}", path.display())))?;
    let text = std::str::from_utf8(&raw).map_err(|e| err(format!("{}: {e}", path.display())))?;
    let data: Table = text.parse().map_err(|e: toml::de::Error| err(format!("{}: {}", path.display(), e.to_string().trim())))?;
    known_keys(&data)?;
    let top = get_table(&data, "plied", "")?;
    let ids = get_table(&data, "ids", "")?;
    let mut kinds = vec![];
    for (kind, d, p, w) in DEFAULT_KINDS {
        let name = kind.name();
        let o = get_table(&ids, name, "ids.")?;
        let wh = format!("ids.{name}.");
        let prefix = get_str(&o, "prefix", Some(p), &wh)?.unwrap_or_default();
        let width = get_positive(&o, "width", w, &wh)? as usize;
        let id_re = Regex::new(&format!(r"^{}\d{{{width},}}$", regex::escape(&prefix))).map_err(|e| err(e.to_string()))?;
        kinds.push(Kind { kind, dir: get_str(&o, "dir", Some(d), &wh)?.unwrap_or_default(), prefix, width, id_re });
    }
    let mut prefixes: Vec<&str> = kinds.iter().map(|k| k.prefix.as_str()).collect();
    prefixes.sort();
    prefixes.dedup();
    if prefixes.len() != kinds.len() {
        return Err(err("ids: kinds must have distinct prefixes"));
    }

    let mut sources = vec![];
    let raw_sources = match data.get("sources") {
        None => vec![],
        Some(T::Array(a)) => a.clone(),
        Some(v) => return Err(err(format!("sources: expected a list, got {}", got(v)))),
    };
    for (i, s) in raw_sources.iter().enumerate() {
        let w = format!("sources[{i}].");
        let T::Table(s) = s else { return Err(err(format!("sources[{i}]: expected a table ([[sources]])"))) };
        let required = |k: &str| -> Res<String> {
            get_str(s, k, None, &w)?.filter(|v| !v.is_empty()).ok_or_else(|| err(format!("{w}{k} is required")))
        };
        let (name, glob, id) = (required("name")?, required("glob")?, required("id")?);
        sources.push(Rc::new(Source {
            name,
            glob,
            id_re: pattern(&id, "", &format!("{w}id"))?,
            id_field: get_str(s, "id_field", Some("id"), &w)?.unwrap_or_default(),
            title_field: get_str(s, "title_field", Some("title"), &w)?.unwrap_or_default(),
            date_field: get_str(s, "date", None, &w)?,
            time_field: get_str(s, "time", None, &w)?,
            status_field: get_str(s, "status", None, &w)?,
            authority_field: get_str(s, "authority", None, &w)?,
            scope_field: get_str(s, "scope", None, &w)?,
            scan_commitments: get_bool(s, "scan_commitments", false, &w)?,
            require_linked: get_bool(s, "require_linked", false, &w)?,
        }));
    }

    let mut literature = None;
    if data.contains_key("literature") {
        let l = get_table(&data, "literature", "")?;
        let w = "literature.";
        if !l.contains_key("manifest") {
            return Err(err("literature.manifest is required"));
        }
        let id_from_file = get_str(&l, "id_from_file", None, w)?;
        if let Some(ref p) = id_from_file
            && pattern(p, "", &format!("{w}id_from_file"))?.groups() < 1
        {
            return Err(err(format!("{w}id_from_file: needs a capture group for the id")));
        }
        let id_default = if id_from_file.as_deref().is_some_and(|s| !s.is_empty()) { None } else { Some("id") };
        let image_command = match get_strs(&l, "image_command", None, w)? {
            Some(cmd) => {
                for a in &cmd {
                    template(Some(a.clone()), &format!("{w}image_command"), &[("id", Arg::S("x")), ("page", Arg::I(1)), ("file", Arg::S("x"))])?;
                }
                if cmd.is_empty() { None } else { Some(cmd) }
            }
            None => None,
        };
        literature = Some(Literature {
            manifest: get_str(&l, "manifest", None, w)?.unwrap_or_default(),
            list_key: get_str(&l, "list", Some("papers"), w)?.unwrap_or_default(),
            file_field: get_str(&l, "file_field", Some("file"), w)?.unwrap_or_default(),
            hash_field: get_str(&l, "hash_field", Some("sha256"), w)?.unwrap_or_default(),
            id_field: get_str(&l, "id_field", id_default, w)?,
            id_from_file,
            pdf_dir: get_str(&l, "pdf_dir", None, w)?,
            text_dir: template(get_str(&l, "text_dir", None, w)?, &format!("{w}text_dir"), &[("id", Arg::S("x"))])?,
            page_file: template(get_str(&l, "page_file", Some("p{page:02d}.txt"), w)?, &format!("{w}page_file"), &[("page", Arg::I(1))])?
                .unwrap_or_default(),
            ocr_marker: get_str(&l, "ocr_marker", None, w)?,
            image_command,
            image_path: template(get_str(&l, "image_path", None, w)?, &format!("{w}image_path"), &[("id", Arg::S("x")), ("page", Arg::I(1))])?,
        });
    }

    let access = get_table(&data, "access", "")?;
    let search = get_table(&data, "search", "")?;
    let checks = get_table(&data, "checks", "")?;
    let msgs = get_table(&data, "messages", "")?;
    let runtime = get_table(&data, "runtime", "")?;
    let format = match top.get("format") {
        None => 1,
        Some(T::Integer(i)) if *i >= 1 => *i,
        Some(v) => return Err(err(format!("plied.format: expected a positive integer, got {}", got(v)))),
    };
    let requires = match get_str(&top, "requires", None, "plied.")? {
        None => None,
        Some(r) => Some(Requires::parse(&r).ok_or_else(|| err(format!("plied.requires: expected \"X.Y.Z\" or \">=X.Y.Z\", got {r:?}")))?),
    };
    let action_kinds = get_strs(&checks, "action_kinds", None, "checks.")?;
    for k in action_kinds.iter().flatten() {
        if k.is_empty() || k.contains(':') || k.chars().any(char::is_whitespace) {
            return Err(err(format!("checks.action_kinds: {k:?} is not a kind (no spaces or `:`)")));
        }
    }
    let revision_days = match runtime.get("revision_days") {
        None => 14,
        Some(T::Integer(i)) if *i >= 0 => *i,
        Some(v) => return Err(err(format!("runtime.revision_days: expected a whole number of days (0 or more), got {}", got(v)))),
    };
    let root = get_str(&top, "root", Some("plied"), "plied.")?.unwrap_or_default();
    if root.starts_with('/') || Path::new(&root).components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err(err("plied.root must be a relative path inside the repository"));
    }
    let phrases =
        get_strs(&checks, "commitment_phrases", Some(DEFAULT_PHRASES.iter().map(|s| s.to_string()).collect()), "checks.")?.unwrap_or_default();
    for p in &phrases {
        pattern(p, "(?i)", "checks.commitment_phrases")?;
    }
    let since = get_str(&checks, "commitments_since", None, "checks.")?;
    if let Some(ref s) = since
        && !Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap().is_match(s)
    {
        return Err(err(format!("checks.commitments_since: expected a date (YYYY-MM-DD), got {s:?}")));
    }
    let base = base.map(Path::to_path_buf).unwrap_or_else(|| path.parent().unwrap_or(Path::new(".")).to_path_buf());
    Ok(Config {
        path: path.to_path_buf(),
        base: fsx::resolve(&base),
        raw_bytes: raw.clone(),
        format,
        requires,
        canonical_branch: get_str(&top, "canonical_branch", None, "plied.")?,
        kinds,
        sources,
        sealed_file: get_str(&access, "sealed_file", None, "access.")?,
        sealed: get_strs(&access, "sealed", Some(vec![]), "access.")?.unwrap_or_default(),
        search_include: get_strs(&search, "include", Some(vec![format!("{root}/**/*.md")]), "search.")?.unwrap_or_default(),
        search_exclude: get_strs(&search, "exclude", Some(vec![]), "search.")?.unwrap_or_default(),
        literature,
        approve_authorities: get_strs(&checks, "approve_authorities", Some(vec!["user".into(), "delegated".into()]), "checks.")?
            .unwrap_or_default(),
        actors: get_strs(&checks, "actors", None, "checks.")?,
        commitment_phrases: phrases,
        commitments_since: since,
        lease_minutes: get_positive(&msgs, "lease_minutes", 30, "messages.")?,
        action_kinds,
        revision_days,
        hard_exclude: vec![".git/**".into(), "**/.git/**".into()],
        root,
    })
}

pub const TEMPLATE: &str = r#"# plied configuration. See https://github.com/jupsh/plied#configuration-reference
[plied]
format = 1                  # the on-disk record format
requires = ">={version}"    # plied versions that may write this record: "X.Y.Z" exactly, or ">=X.Y.Z"
root = "plied"              # where plied's own records live
# canonical_branch = "main"  # writes are refused while the canonical checkout is on another branch

# Other record systems plied links to (read only). Example:
# [[sources]]
# name = "decisions"
# glob = "docs/decisions/*.md"
# id = "^D\\d{3}$"
# date = "date"
# authority = "authority"   # frontmatter field: user | delegated by Dnnn
# scope = "scope"           # frontmatter field: the ids a decision approves

[access]
# sealed_file = ".sealed-paths"   # one substring per line; matching paths are never read
sealed = []

[search]
include = ["plied/**/*.md"]
exclude = []

# [literature]
# manifest = "literature.json"    # {"papers": [{"id": ..., "file": ..., "sha256": ...}]}
# pdf_dir = "papers"
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_forms() {
        assert!(Requires::parse("0.1.0").unwrap().allows("0.1.0"));
        assert!(!Requires::parse("0.1.0").unwrap().allows("0.1.1"));
        assert!(Requires::parse(">=0.1.0").unwrap().allows("0.10.0"));
        assert!(!Requires::parse(">= 0.2.0").unwrap().allows("0.1.9"));
        for bad in ["0.1", "^0.1.0", "0.1.0-beta", "", ">=x.y.z", "0.1.0.0"] {
            assert_eq!(Requires::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn pyformat_subset() {
        assert_eq!(render_template("p{page:02d}.txt", &[("page", Arg::I(3))]).unwrap(), "p03.txt");
        assert_eq!(render_template("{id}-{page}", &[("id", Arg::S("A")), ("page", Arg::I(12))]).unwrap(), "A-12");
        assert_eq!(render_template("{{x}}", &[]).unwrap(), "{x}");
        assert!(render_template("{paper}", &[("id", Arg::S("A"))]).is_err());
        assert!(render_template("{page:x}", &[("page", Arg::I(1))]).is_err());
    }
}
