//! The agent's own plan for a session: task lists it kept (Claude Code
//! `TodoWrite`, `TaskCreate` / `TaskUpdate`; Codex `update_plan`) and plans
//! it proposed (`ExitPlanMode`). Read from tool inputs at query time.

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::index::Result;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Task {
    pub text: String,
    /// `pending`, `in_progress` or `completed`.
    pub status: String,
}

/// The task list as it stood after step `seq`.
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub seq: i64,
    pub tasks: Vec<Task>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanDoc {
    pub seq: i64,
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SessionPlan {
    pub snapshots: Vec<Snapshot>,
    pub plans: Vec<PlanDoc>,
}

impl SessionPlan {
    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty() && self.plans.is_empty()
    }
}

fn status(s: &str) -> String {
    match s {
        "completed" | "done" => "completed",
        "in_progress" | "in-progress" | "active" => "in_progress",
        _ => "pending",
    }
    .to_string()
}

pub fn session_plan(conn: &Connection, id: &str) -> Result<SessionPlan> {
    let mut stmt = conn.prepare(
        "SELECT seq, tool_name, input FROM events
         WHERE session_id = ?1 AND kind = 'tool_call'
           AND tool_name IN ('TodoWrite', 'TaskCreate', 'TaskUpdate', 'update_plan', 'ExitPlanMode')
           AND input IS NOT NULL
         ORDER BY seq",
    )?;
    let rows: Vec<(i64, String, String)> = stmt
        .query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(build(&rows))
}

/// Rows of `(seq, tool, input JSON)` in step order.
fn build(rows: &[(i64, String, String)]) -> SessionPlan {
    let mut out = SessionPlan::default();
    // TaskCreate/TaskUpdate keep a running list; ids are given in order of
    // creation ("1", "2", ...), and `deleted` removes one.
    let mut tasks: Vec<(String, Task)> = Vec::new();
    let mut next_id = 1;
    for (seq, tool, input) in rows {
        let Ok(v) = serde_json::from_str::<Value>(input) else {
            continue;
        };
        let list = match tool.as_str() {
            "TodoWrite" => v.get("todos").and_then(Value::as_array).map(|a| {
                a.iter()
                    .filter_map(|t| {
                        Some(Task {
                            text: t.get("content").and_then(Value::as_str)?.to_string(),
                            status: status(t.get("status").and_then(Value::as_str).unwrap_or("")),
                        })
                    })
                    .collect::<Vec<_>>()
            }),
            "update_plan" => v.get("plan").and_then(Value::as_array).map(|a| {
                a.iter()
                    .filter_map(|t| {
                        Some(Task {
                            text: t.get("step").and_then(Value::as_str)?.to_string(),
                            status: status(t.get("status").and_then(Value::as_str).unwrap_or("")),
                        })
                    })
                    .collect::<Vec<_>>()
            }),
            "TaskCreate" => {
                let text = v
                    .get("subject")
                    .or_else(|| v.get("description"))
                    .and_then(Value::as_str)
                    .unwrap_or("Task")
                    .to_string();
                tasks.push((
                    next_id.to_string(),
                    Task {
                        text,
                        status: "pending".into(),
                    },
                ));
                next_id += 1;
                Some(tasks.iter().map(|(_, t)| t.clone()).collect())
            }
            "TaskUpdate" => {
                let id = v
                    .get("taskId")
                    .map(|x| {
                        x.as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| x.to_string())
                    })
                    .unwrap_or_default();
                let st = v.get("status").and_then(Value::as_str);
                if st == Some("deleted") {
                    tasks.retain(|(i, _)| *i != id);
                } else if let Some((_, t)) = tasks.iter_mut().find(|(i, _)| *i == id) {
                    if let Some(s) = st {
                        t.status = status(s);
                    }
                    if let Some(subject) = v.get("subject").and_then(Value::as_str) {
                        t.text = subject.to_string();
                    }
                }
                Some(tasks.iter().map(|(_, t)| t.clone()).collect())
            }
            "ExitPlanMode" => {
                if let Some(p) = v.get("plan").and_then(Value::as_str) {
                    out.plans.push(PlanDoc {
                        seq: *seq,
                        text: crate::model::truncate(p, 16_000),
                    });
                }
                None
            }
            _ => None,
        };
        if let Some(tasks) = list
            && out.snapshots.last().is_none_or(|s| s.tasks != tasks)
        {
            out.snapshots.push(Snapshot { seq: *seq, tasks });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(seq: i64, tool: &str, input: &str) -> (i64, String, String) {
        (seq, tool.into(), input.into())
    }

    #[test]
    fn todo_write_and_codex_plans() {
        let p = build(&[
            row(
                1,
                "TodoWrite",
                r#"{"todos":[{"content":"Read code","status":"in_progress"},{"content":"Fix","status":"pending"}]}"#,
            ),
            row(
                4,
                "TodoWrite",
                r#"{"todos":[{"content":"Read code","status":"completed"},{"content":"Fix","status":"in_progress"}]}"#,
            ),
            row(
                5,
                "TodoWrite",
                r#"{"todos":[{"content":"Read code","status":"completed"},{"content":"Fix","status":"in_progress"}]}"#,
            ),
            row(
                9,
                "update_plan",
                r#"{"plan":[{"step":"Ship","status":"completed"}]}"#,
            ),
        ]);
        assert_eq!(
            p.snapshots.len(),
            3,
            "an unchanged list isn't a new snapshot"
        );
        assert_eq!(p.snapshots[1].tasks[1].status, "in_progress");
        assert_eq!(p.snapshots[2].tasks[0].text, "Ship");
    }

    #[test]
    fn task_create_update_delete() {
        let p = build(&[
            row(2, "TaskCreate", r#"{"subject":"A","description":"x"}"#),
            row(3, "TaskCreate", r#"{"subject":"B"}"#),
            row(6, "TaskUpdate", r#"{"taskId":"1","status":"in_progress"}"#),
            row(8, "TaskUpdate", r#"{"taskId":"2","status":"deleted"}"#),
            row(9, "ExitPlanMode", r##"{"plan":"# Plan\n1. Do it"}"##),
        ]);
        let last = &p.snapshots.last().unwrap().tasks;
        assert_eq!(
            last,
            &vec![Task {
                text: "A".into(),
                status: "in_progress".into()
            }]
        );
        assert_eq!(p.plans[0].seq, 9);
    }
}
