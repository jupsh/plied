//! Errors carry the exit code the command ends with.

use std::fmt;

pub const EXIT_ERROR: i32 = 1;
pub const EXIT_USAGE: i32 = 2;
pub const EXIT_CONFLICT: i32 = 3;
pub const EXIT_INVALID: i32 = 4;
pub const EXIT_BLOCKED: i32 = 5;
pub const EXIT_GATE: i32 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    General,
    /// the literature cannot be used (not configured, manifest missing, paper not cleared)
    Literature,
    /// the access policy refuses a read
    Policy,
    /// a structured query is malformed
    Query,
}

#[derive(Debug, Clone)]
pub struct Fail {
    pub code: i32,
    pub kind: Kind,
    pub msg: String,
    /// structured facts about the failure, for `--json` (FORMAT.md §11): a conflict's current
    /// revision, a refused write's new diagnostics
    pub detail: Option<serde_json::Value>,
}

pub type Res<T> = Result<T, Fail>;

impl Fail {
    fn of(code: i32, kind: Kind, msg: impl Into<String>) -> Self {
        Fail { code, kind, msg: msg.into(), detail: None }
    }

    pub fn new(msg: impl Into<String>) -> Self {
        Fail::of(EXIT_ERROR, Kind::General, msg)
    }

    pub fn code(code: i32, msg: impl Into<String>) -> Self {
        Fail::of(code, Kind::General, msg)
    }

    pub fn lit(msg: impl Into<String>) -> Self {
        Fail::of(EXIT_ERROR, Kind::Literature, msg)
    }

    pub fn policy(msg: impl Into<String>) -> Self {
        Fail::of(EXIT_ERROR, Kind::Policy, msg)
    }

    pub fn query(msg: impl Into<String>) -> Self {
        Fail::of(EXIT_USAGE, Kind::Query, msg)
    }

    pub fn with_detail(mut self, detail: serde_json::Value) -> Self {
        self.detail = Some(detail);
        self
    }
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::General => "general",
            Kind::Literature => "literature",
            Kind::Policy => "policy",
            Kind::Query => "query",
        }
    }
}

/// The name of an exit code's class, as `--json` reports it (FORMAT.md §11).
pub fn exit_name(code: i32) -> &'static str {
    match code {
        0 => "ok",
        EXIT_USAGE => "usage",
        EXIT_CONFLICT => "conflict",
        EXIT_INVALID => "invalid",
        EXIT_BLOCKED => "blocked",
        EXIT_GATE => "gate",
        _ => "error",
    }
}

impl fmt::Display for Fail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

impl From<serde_json::Error> for Fail {
    fn from(e: serde_json::Error) -> Self {
        Fail::new(e.to_string())
    }
}

impl From<std::io::Error> for Fail {
    fn from(e: std::io::Error) -> Self {
        Fail::new(e.to_string())
    }
}

/// Attach the path to an I/O error.
pub trait IoContext<T> {
    fn at(self, path: &std::path::Path) -> Res<T>;
}

impl<T> IoContext<T> for std::io::Result<T> {
    fn at(self, path: &std::path::Path) -> Res<T> {
        self.map_err(|e| Fail::new(format!("{}: {e}", path.display())))
    }
}

pub fn bail<T>(msg: impl Into<String>) -> Res<T> {
    Err(Fail::new(msg))
}
