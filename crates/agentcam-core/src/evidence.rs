//! Evidence bundles: one session, or every session in a date range, as a
//! single JSON file for an auditor or an incident review.
//!
//! A bundle records who ran the agent (OS user, machine), when, with which
//! agent, model and version, what was asked, each step with how it was
//! allowed, files changed with diffs, risk findings, commits and pull
//! requests. Secrets that the audit rules find are masked, as in exports.
//!
//! The bundle carries a SHA-256 of its own content in a canonical form
//! (object keys sorted), so [`verify`] can tell whether it was edited after
//! it was made. It proves the file wasn't changed since; it doesn't prove
//! the logs it came from were complete.

use std::collections::BTreeMap;

use rusqlite::{Connection, params_from_iter};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::audit::Rules;
use crate::index::Result;
use crate::query::{Filter, SESSION_COLS, SessionRow, session_row};

pub const FORMAT: &str = "agentcam-evidence";
pub const VERSION: u32 = 1;
/// Sessions per bundle; a bigger range should be split.
pub const MAX_SESSIONS: usize = 500;
/// Characters kept of each step's output and of each diff.
const OUTPUT_CHARS: usize = 4_000;
const DIFF_CHARS: usize = 16_000;

#[derive(Debug, Clone, Serialize)]
pub struct Scope {
    pub session_id: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub project: Option<String>,
    pub agent: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Touch {
    pub path: String,
    pub action: String,
    pub diff: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StepFinding {
    pub rule_id: String,
    pub title: String,
    pub severity: String,
    pub excerpt: String,
    pub related_step: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Step {
    /// 1-based, as the UI numbers steps.
    pub step: i64,
    pub ts: Option<String>,
    pub kind: String,
    pub tool: Option<String>,
    pub category: Option<String>,
    pub title: String,
    /// The prompt in full; for other steps, the start of the text.
    pub text: String,
    /// How a tool call was allowed or stopped (`rule`, `auto`, `mode`,
    /// `you`, `rejected`, `blocked-*`), when the agent logs it.
    pub approval: Option<String>,
    pub errored: bool,
    pub duration_ms: Option<i64>,
    pub model: Option<String>,
    pub files: Vec<Touch>,
    pub findings: Vec<StepFinding>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Commit {
    pub step: i64,
    pub hash: Option<String>,
    pub branch: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvidenceSession {
    pub session: SessionRow,
    pub machine_id: String,
    pub os_user: Option<String>,
    /// `host/owner/repo` from the project's git remote, when known.
    pub project_id: Option<String>,
    pub agent_version: Option<String>,
    pub steps: Vec<Step>,
    pub commits: Vec<Commit>,
    pub pull_requests: Vec<String>,
    /// Subagent sessions are included as their own entries, linked here.
    pub subagents: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Summary {
    pub sessions: usize,
    pub steps: usize,
    pub prompts: usize,
    pub commands: usize,
    pub files_changed: usize,
    pub commits: usize,
    pub pull_requests: usize,
    pub findings: BTreeMap<String, usize>,
    pub approvals: BTreeMap<String, usize>,
    /// True when the range held more sessions than one bundle takes.
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Integrity {
    pub algorithm: &'static str,
    /// SHA-256 of the bundle without this field, keys sorted, no spaces.
    pub content_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Evidence {
    pub format: &'static str,
    pub version: u32,
    pub generated_at: String,
    pub generated_by: String,
    pub scope: Scope,
    pub summary: Summary,
    pub sessions: Vec<EvidenceSession>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity: Option<Integrity>,
}

/// One session (with its subagents), or the top-level sessions with a step
/// in the filter's range (and their subagents).
pub fn build(conn: &Connection, scope: Scope, rules: &Rules) -> Result<Evidence> {
    let mut ids: Vec<String> = Vec::new();
    let mut truncated = false;
    if let Some(id) = &scope.session_id {
        ids.push(id.clone());
    } else {
        let f = Filter {
            since: scope.since.clone(),
            until: scope.until.clone(),
            agent: scope.agent.clone(),
            project: scope.project.clone(),
            day: None,
        };
        let (cond, args) = crate::query::conditions(&f);
        let sql = format!(
            "SELECT DISTINCT coalesce(s.parent_id, s.id) FROM events e JOIN sessions s ON s.id = e.session_id
             WHERE {cond} ORDER BY 1 LIMIT {}",
            MAX_SESSIONS + 1
        );
        let mut stmt = conn.prepare(&sql)?;
        ids = stmt
            .query_map(params_from_iter(&args), |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        if ids.len() > MAX_SESSIONS {
            ids.truncate(MAX_SESSIONS);
            truncated = true;
        }
    }
    // Each top-level session, then its subagents.
    let mut all: Vec<String> = Vec::new();
    let mut sub =
        conn.prepare("SELECT id FROM sessions WHERE parent_id = ? ORDER BY started_at")?;
    for id in &ids {
        all.push(id.clone());
        let kids: Vec<String> = sub
            .query_map([id], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        all.extend(kids);
    }
    let mut sessions = Vec::new();
    for id in &all {
        if let Some(s) = session(conn, id, rules)? {
            sessions.push(s);
        }
    }
    let mut summary = Summary {
        sessions: sessions.len(),
        truncated,
        ..Default::default()
    };
    for s in &sessions {
        summary.steps += s.steps.len();
        summary.commits += s.commits.len();
        summary.pull_requests += s.pull_requests.len();
        for st in &s.steps {
            if st.kind == "prompt" {
                summary.prompts += 1;
            }
            if st.category.as_deref() == Some("command") {
                summary.commands += 1;
            }
            summary.files_changed += st.files.iter().filter(|f| f.action != "read").count();
            if let Some(a) = &st.approval {
                *summary.approvals.entry(a.clone()).or_default() += 1;
            }
            for f in &st.findings {
                *summary.findings.entry(f.severity.clone()).or_default() += 1;
            }
        }
    }
    let mut ev = Evidence {
        format: FORMAT,
        version: VERSION,
        generated_at: crate::util::format_ts_ms(crate::util::now_ms()),
        generated_by: format!("agentcam {}", env!("CARGO_PKG_VERSION")),
        scope,
        summary,
        sessions,
        integrity: None,
    };
    // Serializing plain structs to a Value can't fail.
    let value = serde_json::to_value(&ev).unwrap_or(Value::Null);
    ev.integrity = Some(Integrity {
        algorithm: "sha256",
        content_sha256: digest(&value),
    });
    Ok(ev)
}

fn session(conn: &Connection, id: &str, rules: &Rules) -> Result<Option<EvidenceSession>> {
    use rusqlite::OptionalExtension;
    let Some(row) = conn
        .query_row(
            &format!("SELECT {SESSION_COLS} FROM sessions s WHERE s.id = ?"),
            [id],
            session_row,
        )
        .optional()?
    else {
        return Ok(None);
    };
    let (machine_id, os_user, project_id, agent_version): (
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = conn.query_row(
        "SELECT machine_id, os_user, project_remote, agent_version FROM sessions WHERE id = ?",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    let m = |s: &str| rules.mask_secrets(s).into_owned();

    let mut touches: BTreeMap<i64, Vec<Touch>> = BTreeMap::new();
    let mut stmt = conn.prepare(
        "SELECT seq, path, action, diff FROM file_touches WHERE session_id = ? ORDER BY seq, rowid",
    )?;
    for r in stmt.query_map([id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?,
        ))
    })? {
        let (seq, path, action, diff) = r?;
        touches.entry(seq).or_default().push(Touch {
            path,
            action,
            diff: diff.map(|d| crate::model::truncate(&m(&d), DIFF_CHARS)),
        });
    }
    let mut findings: BTreeMap<i64, Vec<StepFinding>> = BTreeMap::new();
    let mut stmt = conn.prepare(
        "SELECT seq, rule_id, title, severity, excerpt, related_seq FROM findings WHERE session_id = ? ORDER BY seq",
    )?;
    for r in stmt.query_map([id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, Option<i64>>(5)?,
        ))
    })? {
        let (seq, rule_id, title, sev, excerpt, related) = r?;
        findings.entry(seq).or_default().push(StepFinding {
            rule_id,
            title,
            severity: crate::query::severity_name(sev).unwrap_or_default(),
            excerpt,
            related_step: related.map(|s| s + 1),
        });
    }
    let mut stmt = conn.prepare(
        "SELECT seq, ts, kind, tool_name, category, title, body, approval, is_error, duration_ms, model
         FROM events WHERE session_id = ? ORDER BY seq",
    )?;
    // Read rows as steps with their raw text, then mask and attach files.
    let mut steps: Vec<Step> = stmt
        .query_map([id], |r| {
            Ok(Step {
                step: r.get::<_, i64>(0)? + 1,
                ts: r.get(1)?,
                kind: r.get(2)?,
                tool: r.get(3)?,
                category: r.get(4)?,
                title: r.get(5)?,
                text: r.get(6)?,
                approval: r.get(7)?,
                errored: r.get(8)?,
                duration_ms: r.get(9)?,
                model: r.get(10)?,
                files: Vec::new(),
                findings: Vec::new(),
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    for st in &mut steps {
        let seq = st.step - 1;
        st.title = m(&st.title);
        st.text = if st.kind == "prompt" {
            m(&st.text)
        } else {
            crate::model::truncate(&m(&st.text), OUTPUT_CHARS)
        };
        st.files = touches.remove(&seq).unwrap_or_default();
        st.findings = findings.remove(&seq).unwrap_or_default();
    }
    let mut stmt = conn.prepare(
        "SELECT seq, hash, branch, message FROM commits WHERE session_id = ? ORDER BY seq",
    )?;
    let commits = stmt
        .query_map([id], |r| {
            Ok(Commit {
                step: r.get::<_, i64>(0)? + 1,
                hash: r.get(1)?,
                branch: r.get(2)?,
                message: r.get::<_, Option<String>>(3)?.map(|x| m(&x)),
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    let mut stmt = conn.prepare("SELECT url FROM prs WHERE session_id = ? ORDER BY seq")?;
    let pull_requests = stmt
        .query_map([id], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    let mut stmt =
        conn.prepare("SELECT id FROM sessions WHERE parent_id = ? ORDER BY started_at")?;
    let subagents = stmt
        .query_map([id], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(Some(EvidenceSession {
        session: row,
        machine_id,
        os_user,
        project_id,
        agent_version,
        steps,
        commits,
        pull_requests,
        subagents,
    }))
}

/// `v` as JSON with object keys sorted and no spaces.
pub fn canonical(v: &Value) -> String {
    let mut out = String::new();
    write_canonical(v, &mut out);
    out
}

fn write_canonical(v: &Value, out: &mut String) {
    match v {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k).unwrap_or_default());
                out.push(':');
                write_canonical(&map[*k], out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (i, x) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(x, out);
            }
            out.push(']');
        }
        other => out.push_str(&serde_json::to_string(other).unwrap_or_default()),
    }
}

/// SHA-256 of a bundle's content: the canonical JSON without `integrity`.
pub fn digest(v: &Value) -> String {
    let mut v = v.clone();
    if let Some(o) = v.as_object_mut() {
        o.remove("integrity");
    }
    let hash = Sha256::digest(canonical(&v).as_bytes());
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

/// What checking a bundle found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Verdict {
    /// The content matches its checksum.
    Intact,
    /// The content changed after the bundle was made.
    Changed { expected: String, found: String },
    /// Not an agentcam evidence bundle, or no checksum in it.
    NotABundle(String),
}

pub fn verify(text: &str) -> Verdict {
    let v: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => return Verdict::NotABundle(format!("not JSON: {e}")),
    };
    if v.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Verdict::NotABundle("no `format: agentcam-evidence` field".into());
    }
    let Some(expected) = v
        .pointer("/integrity/content_sha256")
        .and_then(Value::as_str)
        .map(String::from)
    else {
        return Verdict::NotABundle("no integrity checksum".into());
    };
    let found = digest(&v);
    if found == expected {
        Verdict::Intact
    } else {
        Verdict::Changed { expected, found }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_sorts_keys_at_every_level() {
        let a = json!({"b": 1, "a": {"y": [1, {"d": 2, "c": 3}], "x": "é"}});
        assert_eq!(
            canonical(&a),
            r#"{"a":{"x":"é","y":[1,{"c":3,"d":2}]},"b":1}"#
        );
    }

    #[test]
    fn verify_catches_edits() {
        let mut v = json!({"format": FORMAT, "version": 1, "sessions": [{"title": "Fix login"}]});
        let d = digest(&v);
        v["integrity"] = json!({"algorithm": "sha256", "content_sha256": d});
        let text = serde_json::to_string_pretty(&v).unwrap();
        assert_eq!(verify(&text), Verdict::Intact);
        let edited = text.replace("Fix login", "Fix logout");
        assert!(matches!(verify(&edited), Verdict::Changed { .. }));
        assert!(matches!(verify("{}"), Verdict::NotABundle(_)));
    }
}
