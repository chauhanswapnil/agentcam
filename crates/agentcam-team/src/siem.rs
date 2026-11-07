//! Findings from every machine, for the security team's SIEM.
//!
//! Pull: `GET /api/team/findings/export` with a viewer token. Push: an
//! admin sets a webhook (generic NDJSON, OCSF, or Splunk HEC) and each sync
//! sends the findings it brought. Records hold the same metadata laptops
//! share: rule, severity, step, masked excerpt; OS user names only when an
//! admin has turned names on.

use std::sync::LazyLock;
use std::time::Duration;

use agentcam_core::siem::FindingRecord;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::store::Store;
use crate::{Error, Result};

/// Rule titles by id, from the built-in rules (laptops send ids only).
static TITLES: LazyLock<std::collections::HashMap<String, String>> = LazyLock::new(|| {
    agentcam_core::audit::Rules::builtin()
        .defs()
        .into_iter()
        .map(|d| (d.id.clone(), d.title.clone()))
        .collect()
});

/// Findings on sessions received at or after `since`, optionally only
/// `sessions` from one machine.
pub fn records(
    store: &Store,
    since: Option<&str>,
    team: Option<i64>,
    only: Option<(&str, &[String])>,
) -> Result<Vec<FindingRecord>> {
    let people = crate::views::show_people(store);
    let mut stmt = store.conn().prepare(
        "SELECT coalesce(s.ended_at, s.started_at), f.rule_id, f.severity, f.excerpt, f.session_id, f.step,
                s.agent, s.agent_version, s.model, s.project, f.machine_id, d.os_user
         FROM findings f
         JOIN sessions s ON s.machine_id = f.machine_id AND s.session_id = f.session_id
         JOIN devices d ON d.machine_id = f.machine_id
         WHERE (?1 IS NULL OR s.received_at >= ?1) AND (?2 IS NULL OR s.team_id = ?2)
         ORDER BY s.received_at, f.session_id, f.step",
    )?;
    let rows = stmt.query_map(params![since, team], |r| {
        let rule_id: String = r.get(1)?;
        Ok(FindingRecord {
            ts: r.get(0)?,
            title: TITLES
                .get(&rule_id)
                .cloned()
                .unwrap_or_else(|| rule_id.clone()),
            rule_id,
            severity: r.get(2)?,
            excerpt: r.get(3)?,
            session_id: r.get(4)?,
            step: r.get(5)?,
            related_step: None,
            tool: None,
            agent: r.get(6)?,
            agent_version: r.get(7)?,
            model: r.get(8)?,
            project: None,
            project_id: r.get(9)?,
            machine_id: r.get(10)?,
            os_user: r.get::<_, Option<String>>(11)?.filter(|_| people),
        })
    })?;
    let mut out: Vec<FindingRecord> = rows.collect::<std::result::Result<_, _>>()?;
    if let Some((machine, ids)) = only {
        out.retain(|r| r.machine_id == machine && ids.contains(&r.session_id));
    }
    Ok(out)
}

/// Where and how to push findings. The token is stored on the server and
/// never returned by the API.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Webhook {
    pub url: String,
    /// `ndjson`, `ocsf` or `splunk_hec`.
    pub format: String,
    #[serde(default, skip_serializing)]
    pub token: Option<String>,
}

/// The last push, for the Settings page.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LastPush {
    pub ts: String,
    pub ok: bool,
    pub sent: usize,
    pub message: String,
}

pub fn webhook(store: &Store) -> Option<Webhook> {
    let url = store.setting("siem_url").filter(|u| !u.is_empty())?;
    Some(Webhook {
        url,
        format: store
            .setting("siem_format")
            .unwrap_or_else(|| "ndjson".into()),
        token: store.setting("siem_token").filter(|t| !t.is_empty()),
    })
}

/// Check a webhook an admin wants to save: https (or loopback), a known
/// format.
pub fn validate(w: &Webhook) -> Result<()> {
    let loopback = ["http://localhost", "http://127.0.0.1", "http://[::1]"]
        .iter()
        .any(|l| w.url.starts_with(l));
    if !w.url.starts_with("https://") && !loopback {
        return Err(Error::Bad(
            "the webhook needs https:// (or a loopback address)".into(),
        ));
    }
    if !matches!(w.format.as_str(), "ndjson" | "ocsf" | "splunk_hec") {
        return Err(Error::Bad("format is ndjson, ocsf or splunk_hec".into()));
    }
    Ok(())
}

/// The request body for a batch of records.
pub fn body(records: &[FindingRecord], format: &str) -> String {
    match format {
        "splunk_hec" => records
            .iter()
            .map(|r| {
                let time =
                    r.ts.as_deref()
                        .and_then(agentcam_core::util::parse_ts_ms)
                        .map_or(0.0, |ms| ms as f64 / 1000.0);
                json!({
                    "time": time,
                    "source": "agentcam",
                    "sourcetype": "agentcam:finding",
                    "event": agentcam_core::siem::ocsf(r),
                })
                .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n"),
        f => agentcam_core::siem::lines(records, f),
    }
}

/// Send one batch. Blocking; call it off the request path.
pub fn push(w: &Webhook, records: &[FindingRecord]) -> std::result::Result<(), String> {
    if records.is_empty() {
        return Ok(());
    }
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(20))
        .build();
    let mut req = agent.post(&w.url).set(
        "Content-Type",
        if w.format == "splunk_hec" {
            "application/json"
        } else {
            "application/x-ndjson"
        },
    );
    if let Some(t) = &w.token {
        let auth = if w.format == "splunk_hec" {
            format!("Splunk {t}")
        } else {
            format!("Bearer {t}")
        };
        req = req.set("Authorization", &auth);
    }
    match req.send_string(&body(records, &w.format)) {
        Ok(_) => Ok(()),
        Err(ureq::Error::Status(code, _)) => Err(format!("the webhook answered {code}")),
        Err(e) => Err(format!("couldn't reach the webhook: {}", e.kind())),
    }
}

pub fn last_push(store: &Store) -> Option<LastPush> {
    store
        .setting("siem_last")
        .and_then(|t| serde_json::from_str(&t).ok())
}

pub fn save_last_push(store: &Store, p: &LastPush) -> Result<()> {
    store.set_setting("siem_last", &serde_json::to_string(p).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec() -> FindingRecord {
        FindingRecord {
            ts: Some("2026-11-01T10:00:00.000Z".into()),
            rule_id: "git-force-push".into(),
            title: "Force push".into(),
            severity: "high".into(),
            excerpt: "git push --force".into(),
            session_id: "s1".into(),
            step: 4,
            related_step: None,
            tool: None,
            agent: "codex".into(),
            agent_version: None,
            model: None,
            project: None,
            project_id: Some("github.com/acme/app".into()),
            machine_id: "m1".into(),
            os_user: None,
        }
    }

    #[test]
    fn splunk_events_wrap_ocsf() {
        let b = body(&[rec()], "splunk_hec");
        let v: serde_json::Value = serde_json::from_str(&b).unwrap();
        assert_eq!(v["sourcetype"], "agentcam:finding");
        assert_eq!(v["time"], 1_793_527_200.0);
        assert_eq!(v["event"]["class_uid"], 2004);
    }

    #[test]
    fn webhooks_need_https() {
        let w = |url: &str, format: &str| Webhook {
            url: url.into(),
            format: format.into(),
            token: None,
        };
        assert!(validate(&w("https://siem.example/hook", "ocsf")).is_ok());
        assert!(validate(&w("http://127.0.0.1:8088/services/collector", "splunk_hec")).is_ok());
        assert!(validate(&w("http://siem.example/hook", "ocsf")).is_err());
        assert!(validate(&w("https://siem.example/hook", "xml")).is_err());
    }
}
