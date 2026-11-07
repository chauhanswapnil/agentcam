//! Export sessions as OpenTelemetry traces (OTLP/JSON), so they can be
//! viewed in Jaeger, Grafana Tempo, Honeycomb or any OTLP backend.
//!
//! Each session is a trace: a root span for the session, one child span per
//! step (tool calls last as long as they ran), and subagent sessions nested
//! under the tool call that started them.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use rusqlite::{Connection, params_from_iter};
use serde_json::{Value, json};

use crate::index::Result;
use crate::query::{Filter, conditions};
use crate::util::parse_ts_ms;

struct Sess {
    id: String,
    agent: String,
    parent: Option<String>,
    title: String,
    project: Option<String>,
    model: Option<String>,
    branch: Option<String>,
    started: Option<String>,
    ended: Option<String>,
    cost: Option<f64>,
}

fn hex_id(parts: &[&str], bytes: usize) -> String {
    let mut out = String::new();
    let mut salt = 0u64;
    while out.len() < bytes * 2 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        salt.hash(&mut h);
        parts.hash(&mut h);
        out.push_str(&format!("{:016x}", h.finish()));
        salt += 1;
    }
    out.truncate(bytes * 2);
    out
}

fn nanos(ts: Option<&str>) -> Option<String> {
    Some((parse_ts_ms(ts?)? as i128 * 1_000_000).to_string())
}

fn attr(key: &str, v: Value) -> Value {
    let value = match v {
        Value::Bool(b) => json!({ "boolValue": b }),
        Value::Number(n) if n.is_i64() => json!({ "intValue": n.to_string() }),
        Value::Number(n) => json!({ "doubleValue": n.as_f64() }),
        other => {
            json!({ "stringValue": other.as_str().map(String::from).unwrap_or_else(|| other.to_string()) })
        }
    };
    json!({ "key": key, "value": value })
}

/// OTLP/JSON for the top-level sessions with activity under `f`, plus their
/// subagents. Returns the request body and how many sessions it holds.
pub fn export(conn: &Connection, f: &Filter) -> Result<(Value, usize)> {
    let (cond, args) = conditions(f);
    let roots_sql = format!(
        "SELECT DISTINCT coalesce(s.parent_id, s.id) FROM events e JOIN sessions s ON s.id = e.session_id WHERE {cond}"
    );
    let mut stmt = conn.prepare(&roots_sql)?;
    let roots: Vec<String> = stmt
        .query_map(params_from_iter(&args), |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;

    let mut sess_stmt = conn.prepare(
        "SELECT id, agent, parent_id, coalesce(title, first_prompt, id), project_root, model, git_branch,
                started_at, ended_at, coalesce(est_cost_usd, cost_usd)
         FROM sessions WHERE id = ?1 OR parent_id = ?1 ORDER BY parent_id IS NOT NULL, started_at",
    )?;
    let mut ev_stmt = conn.prepare(
        "SELECT seq, ts, kind, tool_name, category, title, is_error, duration_ms, tok_in, tok_out,
                tok_cache_read, cost_usd, model
         FROM events WHERE session_id = ? ORDER BY seq",
    )?;

    let mut spans = Vec::new();
    for root in &roots {
        let group: Vec<Sess> = sess_stmt
            .query_map([root], |r| {
                Ok(Sess {
                    id: r.get(0)?,
                    agent: r.get(1)?,
                    parent: r.get(2)?,
                    title: r.get(3)?,
                    project: r.get(4)?,
                    model: r.get(5)?,
                    branch: r.get(6)?,
                    started: r.get(7)?,
                    ended: r.get(8)?,
                    cost: r.get(9)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        let trace = hex_id(&[root], 16);
        // Subagent tool calls in the parent, in order, pair with subagent
        // sessions in start order (the same rule the UI uses).
        let mut agent_calls: Vec<String> = Vec::new();
        let mut parent_span: HashMap<String, String> = HashMap::new();

        for s in &group {
            let root_span = hex_id(&[&s.id, "session"], 8);
            let parent = match &s.parent {
                None => None,
                Some(p) => {
                    let call = if agent_calls.is_empty() {
                        None
                    } else {
                        Some(agent_calls.remove(0))
                    };
                    Some(call.unwrap_or_else(|| hex_id(&[p, "session"], 8)))
                }
            };
            parent_span.insert(s.id.clone(), root_span.clone());
            let start = nanos(s.started.as_deref()).unwrap_or_else(|| "0".into());
            let end = nanos(s.ended.as_deref()).unwrap_or_else(|| start.clone());
            let mut attrs = vec![
                attr("agentcam.session_id", json!(s.id)),
                attr("agentcam.agent", json!(s.agent)),
            ];
            for (k, v) in [
                ("agentcam.project", &s.project),
                ("gen_ai.request.model", &s.model),
                ("vcs.ref.head.name", &s.branch),
            ] {
                if let Some(v) = v {
                    attrs.push(attr(k, json!(v)));
                }
            }
            if let Some(c) = s.cost {
                attrs.push(attr("agentcam.cost_usd", json!(c)));
            }
            let mut span = json!({
                "traceId": trace, "spanId": root_span, "name": crate::model::first_line(&s.title, 120),
                "kind": 1, "startTimeUnixNano": start, "endTimeUnixNano": end, "attributes": attrs,
            });
            if let Some(p) = parent {
                span["parentSpanId"] = json!(p);
            }
            spans.push(span);

            let mut rows = ev_stmt.query([&s.id])?;
            while let Some(r) = rows.next()? {
                let seq: i64 = r.get(0)?;
                let ts: Option<String> = r.get(1)?;
                let Some(st) = ts.as_deref().and_then(parse_ts_ms) else {
                    continue;
                };
                let kind: String = r.get(2)?;
                let tool: Option<String> = r.get(3)?;
                let category: Option<String> = r.get(4)?;
                let title: String = r.get(5)?;
                let is_error: bool = r.get(6)?;
                let dur: i64 = r.get::<_, Option<i64>>(7)?.unwrap_or(0).max(1);
                let span_id = hex_id(&[&s.id, &seq.to_string()], 8);
                if s.parent.is_none() && category.as_deref() == Some("subagent") {
                    agent_calls.push(span_id.clone());
                }
                let mut attrs = vec![
                    attr("agentcam.step", json!(seq)),
                    attr("agentcam.kind", json!(kind)),
                    attr("agentcam.title", json!(crate::model::truncate(&title, 500))),
                ];
                if let Some(c) = &category {
                    attrs.push(attr("agentcam.category", json!(c)));
                }
                let (tin, tout, tcr): (i64, i64, i64) = (r.get(8)?, r.get(9)?, r.get(10)?);
                if tin + tout + tcr > 0 {
                    attrs.push(attr("gen_ai.usage.input_tokens", json!(tin + tcr)));
                    attrs.push(attr("gen_ai.usage.output_tokens", json!(tout)));
                }
                if let Some(c) = r.get::<_, Option<f64>>(11)? {
                    attrs.push(attr("agentcam.cost_usd", json!(c)));
                }
                if let Some(m) = r.get::<_, Option<String>>(12)? {
                    attrs.push(attr("gen_ai.response.model", json!(m)));
                }
                let name = match (kind.as_str(), &tool) {
                    ("tool_call", Some(t)) => t.clone(),
                    (k, _) => k.to_string(),
                };
                let mut span = json!({
                    "traceId": trace, "spanId": span_id, "parentSpanId": parent_span[&s.id],
                    "name": name, "kind": 1,
                    "startTimeUnixNano": (st as i128 * 1_000_000).to_string(),
                    "endTimeUnixNano": ((st + dur) as i128 * 1_000_000).to_string(),
                    "attributes": attrs,
                });
                if is_error {
                    span["status"] = json!({ "code": 2, "message": "step errored" });
                }
                spans.push(span);
            }
        }
    }
    let body = json!({
        "resourceSpans": [{
            "resource": { "attributes": [attr("service.name", json!("agentcam"))] },
            "scopeSpans": [{ "scope": { "name": "agentcam", "version": env!("CARGO_PKG_VERSION") }, "spans": spans }],
        }]
    });
    Ok((body, roots.len()))
}
