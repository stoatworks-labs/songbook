//! A structural diff between two shows, for the history view.
//!
//! Works on the JSON form so it never lags a model change, and reports paths
//! in the same spelling the inspector uses (`channels[ch:1].sends[bus:aux:2].levelDb`)
//! by keying array items on their `id` when they have one.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeKind {
    Added,
    Removed,
    Changed,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub kind: ChangeKind,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<Value>,
}

/// Diff two JSON documents. `meta.modified` is ignored: it changes on every save.
pub fn diff(before: &Value, after: &Value) -> Vec<Change> {
    let mut out = vec![];
    walk("", before, after, &mut out);
    out.retain(|c| c.path != "meta.modified");
    out
}

fn item_key(v: &Value, index: usize) -> String {
    match v.get("id").and_then(Value::as_str) {
        Some(id) => format!("[{id}]"),
        None => format!("[{index}]"),
    }
}

fn join(path: &str, seg: &str) -> String {
    if path.is_empty() {
        seg.trim_start_matches('.').to_string()
    } else if seg.starts_with('[') {
        format!("{path}{seg}")
    } else {
        format!("{path}.{seg}")
    }
}

fn walk(path: &str, a: &Value, b: &Value, out: &mut Vec<Change>) {
    match (a, b) {
        (Value::Object(ao), Value::Object(bo)) => {
            for (k, av) in ao {
                match bo.get(k) {
                    Some(bv) => walk(&join(path, k), av, bv, out),
                    None => out.push(Change {
                        kind: ChangeKind::Removed,
                        path: join(path, k),
                        before: Some(av.clone()),
                        after: None,
                    }),
                }
            }
            for (k, bv) in bo {
                if !ao.contains_key(k) {
                    out.push(Change {
                        kind: ChangeKind::Added,
                        path: join(path, k),
                        before: None,
                        after: Some(bv.clone()),
                    });
                }
            }
        }
        (Value::Array(aa), Value::Array(ba)) => {
            let keyed_a: Vec<(String, &Value)> = aa.iter().enumerate().map(|(i, v)| (item_key(v, i), v)).collect();
            let keyed_b: Vec<(String, &Value)> = ba.iter().enumerate().map(|(i, v)| (item_key(v, i), v)).collect();
            for (k, av) in &keyed_a {
                match keyed_b.iter().find(|(kb, _)| kb == k) {
                    Some((_, bv)) => walk(&join(path, k), av, bv, out),
                    None => out.push(Change {
                        kind: ChangeKind::Removed,
                        path: join(path, k),
                        before: Some((*av).clone()),
                        after: None,
                    }),
                }
            }
            for (k, bv) in &keyed_b {
                if !keyed_a.iter().any(|(ka, _)| ka == k) {
                    out.push(Change {
                        kind: ChangeKind::Added,
                        path: join(path, k),
                        before: None,
                        after: Some((*bv).clone()),
                    });
                }
            }
        }
        _ => {
            if a != b {
                out.push(Change {
                    kind: ChangeKind::Changed,
                    path: path.to_string(),
                    before: Some(a.clone()),
                    after: Some(b.clone()),
                });
            }
        }
    }
}

/// One line per change, for a commit message or a log.
pub fn describe(changes: &[Change]) -> Vec<String> {
    changes
        .iter()
        .map(|c| match c.kind {
            ChangeKind::Added => format!("+ {}", c.path),
            ChangeKind::Removed => format!("- {}", c.path),
            ChangeKind::Changed => format!(
                "~ {}: {} → {}",
                c.path,
                short(c.before.as_ref()),
                short(c.after.as_ref())
            ),
        })
        .collect()
}

fn short(v: Option<&Value>) -> String {
    match v {
        None => "∅".into(),
        Some(Value::String(s)) => format!("\"{}\"", s),
        Some(other) => {
            let s = other.to_string();
            if s.len() > 40 {
                format!("{}…", &s[..40])
            } else {
                s
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keys_arrays_by_id() {
        let a = json!({"channels": [{"id": "ch:1", "label": "Kick"}, {"id": "ch:2", "label": "Snare"}]});
        let b = json!({"channels": [{"id": "ch:1", "label": "Kick in"}, {"id": "ch:3", "label": "Tom"}]});
        let d = diff(&a, &b);
        let paths: Vec<_> = d.iter().map(|c| (c.kind.clone(), c.path.clone())).collect();
        assert!(paths.contains(&(ChangeKind::Changed, "channels[ch:1].label".into())));
        assert!(paths.contains(&(ChangeKind::Removed, "channels[ch:2]".into())));
        assert!(paths.contains(&(ChangeKind::Added, "channels[ch:3]".into())));
        assert_eq!(d.len(), 3);
    }

    #[test]
    fn ignores_modified_timestamp() {
        let a = json!({"meta": {"modified": "1", "name": "x"}});
        let b = json!({"meta": {"modified": "2", "name": "x"}});
        assert!(diff(&a, &b).is_empty());
    }
}
