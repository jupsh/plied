//! Read-only discovery, using the same fields, nested keys and enums as validation.
use serde_json::{Value, json};
use crate::config::Config;
use crate::records::{Applies, CommitmentState, Disposition, OwnKind, QuestionState, ThreadStatus, LIST_FIELDS, DATE_FIELDS};

pub fn subfields(field: &str) -> Option<&'static [&'static str]> {
    match field {
        "approval" => Some(&["state", "by"]),
        "outcome" => Some(&["quote", "ref"]),
        "cites" => Some(&["paper", "page", "quote", "kind", "image_checked", "note"]),
        "reviews" => Some(&["action", "disposition", "at", "by", "because", "ref", "inputs"]),
        "reviews.inputs" => Some(&["record", "rev"]),
        "scanned" => Some(&["record", "rev", "at", "by", "because"]),
        "prior_outcomes" => Some(&["quote", "ref", "rev", "carries", "scope", "because", "by", "at", "replaced_by"]),
        _ => None,
    }
}

pub fn enums(field: &str) -> &'static [(&'static str, &'static [&'static str])] {
    match field {
        "cites" => &[("kind", &["table", "figure"]), ("image_checked", &["true", "false"])],
        "prior_outcomes" => &[("scope", &crate::records::PRIOR_SCOPES)],
        _ => &[],
    }
}

pub fn vocabularies(cfg: Option<&Config>) -> Value {
    json!({"actors": cfg.and_then(|c| c.actors.as_ref()), "action_kinds": cfg.and_then(|c| c.action_kinds.as_ref()),
        "approve_authorities": cfg.map(|c| &c.approve_authorities)})
}

fn targets(field: &str, cfg: Option<&Config>) -> Value {
    let linked: Vec<&str> = cfg.map(|c| c.sources.iter().map(|s| s.name.as_str()).collect()).unwrap_or_default();
    match field {
        "parent" => json!(["thread"]),
        "nodes" => json!(["thread", "question"]),
        "superseded_by" => json!(["commitment"]),
        "backlog" | "scanned.record" | "approval.by" => json!({"linked_sources": linked}),
        _ => json!({"own": ["thread", "commitment", "question"], "linked_sources": linked}),
    }
}

fn required_when(kind: OwnKind, field: &str) -> Vec<&'static str> {
    match (kind, field) {
        (OwnKind::Thread, "waiting_on") => vec!["status=waiting"],
        (OwnKind::Thread, "outcome") => vec!["status=closed: quote and ref are required, and quote must match ref"],
        (OwnKind::Thread, "approval") => vec!["when set: state required; state=approved requires by with authority over this record"],
        (OwnKind::Commitment, "resolved_by") => vec!["state is fulfilled, waived, missed, not-triggered or superseded; waived/superseded require linked authority over this commitment"],
        (OwnKind::Commitment, "superseded_by") => vec!["state=superseded"],
        (OwnKind::Question, "answered_by") => vec!["state=answered: linked authority over this question"],
        _ => vec![],
    }
}

pub fn describe(kind: OwnKind, cfg: Option<&Config>) -> Value {
    let fields: Vec<Value> = kind.fields().iter().map(|field| {
        let list = LIST_FIELDS.contains(field);
        let ty = if *field == "next" { "positive_integer" } else if DATE_FIELDS.contains(field) { "date" }
            else if list { "list" } else if subfields(field).is_some() { "mapping" } else { "string" };
        let values = match *field {
            "status" => json!(ThreadStatus::ALL),
            "state" if kind == OwnKind::Commitment => json!(CommitmentState::ALL),
            "state" => json!(QuestionState::ALL),
            "applies" => json!(Applies::ALL),
            "actor" => cfg.and_then(|c| c.actors.as_ref()).filter(|a| !a.is_empty()).map_or(Value::Null, |a| json!(a)),
            "governs" => cfg.and_then(|c| c.action_kinds.as_ref()).map_or(Value::Null, |a| json!(a)),
            _ => Value::Null,
        };
        let reference = kind.ref_fields().contains(field);
        let properties = subfields(field).map(|names| names.iter().map(|name| {
            let full = format!("{field}.{name}");
            let mut values = enums(field).iter().find(|(n, _)| n == name).map_or(Value::Null, |(_, values)| json!(values));
            if full == "approval.state" { values = json!(["approved", "not approved"]); }
            if full == "reviews.disposition" { values = json!(Disposition::ALL); }
            json!({"name": name, "type": if *name == "page" { "integer" } else if matches!(full.as_str(), "reviews.inputs" | "prior_outcomes.carries") { "list" } else { "string" }, "values": values,
                "required": matches!((*field, *name), ("approval", "state") | ("outcome", "quote" | "ref") | ("cites", "paper" | "page") | ("reviews", "action" | "disposition") | ("scanned", "record" | "rev") | ("prior_outcomes", "quote" | "ref" | "scope" | "because")),
                "references": if kind.ref_fields().contains(&full.as_str()) || full == "scanned.record" { targets(&full, cfg) } else { Value::Null },
                "properties": if full == "reviews.inputs" { Some(vec![
                    json!({"name": "record", "type": "string", "required": true, "references": targets("reviews.inputs.record", cfg)}),
                    json!({"name": "rev", "type": "sha256", "required": true, "length": 64}),
                ]) } else { None },
                "items": if full == "reviews.inputs" { Some("mapping") } else { None }})
        }).collect::<Vec<_>>());
        json!({"name": field, "type": ty, "values": values, "properties": properties,
            "items": if list { Some(if properties.is_some() { "mapping" } else if reference { "reference" } else { "string" }) } else { None },
            "references": if reference { targets(field, cfg) } else { Value::Null },
            "required": matches!(*field, "id" | "title") || *field == kind.state_field() || matches!((kind, *field), (OwnKind::Commitment, "wording" | "made") | (OwnKind::Question, "raised")),
            "default": if *field == kind.state_field() { json!("open") } else if *field == "applies" { json!("linked") } else { Value::Null },
            "required_when": required_when(kind, field), "read_only": *field == "id", "append_only": matches!(*field, "reviews" | "scanned" | "prior_outcomes")})
    }).collect();
    let thread_id = cfg.map(|c| format!("{}{:0width$}", c.kind(OwnKind::Thread).prefix, 1, width=c.kind(OwnKind::Thread).width)).unwrap_or_else(|| "T001".into());
    let thread = json!({"id": thread_id, "title": "Example task", "status": "open"});
    let example = match kind {
        OwnKind::Thread => vec![thread],
        OwnKind::Commitment => vec![thread, json!({"title": "Review the task", "wording": "Example task", "made": thread_id, "state": "open", "nodes": [thread_id]})],
        OwnKind::Question => vec![thread, json!({"title": "Which approach?", "raised": thread_id, "state": "open"})],
    };
    json!({"kind": kind.name(), "format": 1, "configured": cfg.is_some(), "fields": fields,
        "vocabularies": vocabularies(cfg), "example_records": example,
        "constraints": match kind {
            OwnKind::Thread => vec!["next is only allowed with status=open", "open nodes cannot wait on unresolved records", "group nodes cannot have next or waiting_on", "outcome and carries are only allowed when closed"],
            OwnKind::Commitment => vec!["wording must be a quotation of made", "governs requires checks.action_kinds", "superseded_by is only allowed when superseded", "without nodes a non-project commitment is unattached (warning)"],
            OwnKind::Question => vec!["answered_by is checked for linked authority"],
        }, "contract": "docs/FORMAT.md §3, §4, §8 and §12"})
}
