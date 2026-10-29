//! Read-only diagnostic comparisons. A baseline never changes a validation verdict.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::check::{Level, Problem};
use crate::error::{Fail, Res, EXIT_USAGE};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Source {
    pub record: String,
    pub rev: Option<String>,
}

/// One ordinary diagnostic identity, or one source of an aggregated stale diagnostic.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Entry {
    pub level: String,
    pub code: String,
    #[serde(rename = "where")]
    pub location: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

impl Entry {
    pub fn new(problem: &Problem, source: Option<Source>) -> Entry {
        Entry {
            level: match problem.level { Level::Error => "error", Level::Warning => "warning" }.into(),
            code: problem.code.clone(), location: problem.location.clone(), subject: problem.subject.clone(), source,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Basis {
    pub v: u32,
    pub warnings: bool,
    pub policy: String,
    pub entries: BTreeSet<Entry>,
}

impl Basis {
    pub fn new(warnings: bool, policy: String, entries: impl IntoIterator<Item = Entry>) -> Basis {
        Basis { v: 1, warnings, policy, entries: entries.into_iter().collect() }
    }

    /// Accept only a complete check envelope with the same warning scope and access policy.
    /// Historical messages are never echoed; comparisons contain identities and source hashes.
    pub fn compare(&self, saved: &Value) -> Res<Comparison> {
        let invalid = || Fail::code(EXIT_USAGE, "baseline must be a saved `plied check --json` envelope with comparison_basis v1; capture a new baseline");
        if saved.get("plied").and_then(Value::as_u64) != Some(1) { return Err(invalid()); }
        let report = saved.get("result").filter(|r| r.get("diagnostics").is_some_and(Value::is_array) && r.get("records").is_some() && r.get("linked").is_some()).ok_or_else(invalid)?;
        let old: Basis = serde_json::from_value(report.get("comparison_basis").cloned().ok_or_else(invalid)?).map_err(|_| invalid())?;
        if old.v != 1 { return Err(invalid()); }
        if old.warnings != self.warnings {
            return Err(Fail::code(EXIT_USAGE, "baseline warning scope differs; use the same --errors-only setting or capture a new baseline"));
        }
        if old.policy != self.policy {
            return Err(Fail::code(EXIT_USAGE, "baseline access policy differs; capture a new baseline under the current policy"));
        }
        Ok(Comparison {
            added: self.entries.difference(&old.entries).cloned().collect(),
            persisting: self.entries.intersection(&old.entries).cloned().collect(),
            resolved: old.entries.difference(&self.entries).cloned().collect(),
        })
    }
}

#[derive(Debug, Serialize)]
pub struct Comparison {
    pub added: Vec<Entry>,
    pub persisting: Vec<Entry>,
    pub resolved: Vec<Entry>,
}
