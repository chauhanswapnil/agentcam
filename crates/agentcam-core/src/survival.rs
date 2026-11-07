//! Did the code last? Lines agents added, checked against the files today.
//!
//! Every edit with a diff gives the lines it added (all lines for a new
//! file). Each added line is then one of:
//!
//! - **kept**: the file still has a line with the same text;
//! - **reworked**: gone, and a later agent edit to the file removed it;
//! - **changed**: gone for another reason (edited by hand, another tool,
//!   a formatter, or the file was moved or deleted).
//!
//! Lines are compared by trimmed text, and lines with fewer than three
//! letters or digits (`}`, `);`, blank) are skipped. Moved lines still
//! count as kept. Only counts are stored, never file contents. A file is
//! re-read only when it or its edits change.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Serialize;

use crate::index::Result;

/// Bump when the way lines are read or sorted changes: every file is
/// checked again on the next run.
const ENGINE: u32 = 2;

/// Files bigger than this aren't compared.
const MAX_FILE: u64 = 4 * 1024 * 1024;

/// One line, as compared: trimmed, or `None` when it's too small to say
/// anything (braces, blank lines, a lone `else`).
pub fn norm(line: &str) -> Option<&str> {
    let t = line.trim();
    let signal = t.chars().filter(|c| c.is_alphanumeric()).take(3).count();
    (signal >= 3 && t.len() <= 500).then_some(t)
}

/// Lines a change added and removed. A new file (`create`) adds all its
/// lines; an edit is read as unified diff hunks. A diff cut short at the
/// index's text limit drops its last, partial line.
pub fn added_removed(diff: &str, action: &str) -> (Vec<String>, Vec<String>) {
    let mut lines: Vec<&str> = diff.lines().collect();
    if diff.ends_with('…') {
        lines.pop();
    }
    let (mut add, mut del) = (Vec::new(), Vec::new());
    // A write over an existing file is stored as hunks, not as content.
    if action == "create" && !diff.starts_with("@@") {
        add.extend(lines.iter().filter_map(|l| norm(l)).map(String::from));
        return (add, del);
    }
    for l in lines {
        if let Some(rest) = l.strip_prefix('+').filter(|_| !l.starts_with("+++")) {
            add.extend(norm(rest).map(String::from));
        } else if let Some(rest) = l.strip_prefix('-').filter(|_| !l.starts_with("---")) {
            del.extend(norm(rest).map(String::from));
        }
    }
    (add, del)
}

/// Where a path lives today. Files edited in an agent worktree
/// (`repo/.claude/worktrees/x/src/a.ts`) end up in the repo once merged
/// and the worktree is removed.
fn resolve(path: &str) -> Option<PathBuf> {
    let p = Path::new(path);
    if p.exists() {
        return Some(p.to_path_buf());
    }
    let i = path.find("/.claude/worktrees/")?;
    let rest = &path[i + "/.claude/worktrees/".len()..];
    let (_, inner) = rest.split_once('/')?;
    let moved = Path::new(&path[..i]).join(inner);
    moved.exists().then_some(moved)
}

/// Counts for one edit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub added: i64,
    pub kept: i64,
    pub reworked: i64,
    pub changed: i64,
}

impl Counts {
    pub fn add(&mut self, o: &Counts) {
        self.added += o.added;
        self.kept += o.kept;
        self.reworked += o.reworked;
        self.changed += o.changed;
    }
}

struct Edit {
    session_id: String,
    seq: i64,
    ts: String,
    action: String,
    added: Vec<String>,
    removed: HashSet<String>,
}

/// Sort each added line of each edit into kept / reworked / changed.
/// `current` is the file's lines today, or `None` when it's gone.
fn classify(edits: &[Edit], current: Option<&HashSet<&str>>) -> Vec<Counts> {
    edits
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let later = &edits[i + 1..];
            let deleted_later = later.iter().any(|l| l.action == "delete");
            let mut c = Counts {
                added: e.added.len() as i64,
                ..Default::default()
            };
            for line in &e.added {
                if current.is_some_and(|cur| cur.contains(line.as_str())) {
                    c.kept += 1;
                } else if deleted_later || later.iter().any(|l| l.removed.contains(line)) {
                    c.reworked += 1;
                } else {
                    c.changed += 1;
                }
            }
            c
        })
        .collect()
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RefreshReport {
    pub files_checked: usize,
    pub files_skipped: usize,
}

/// Re-check files whose content or edits changed since the last run.
pub fn refresh(conn: &mut Connection) -> Result<RefreshReport> {
    let mut report = RefreshReport::default();
    // Edits with a diff, plus deletes (they make later lines "reworked").
    let mut by_path: HashMap<String, Vec<Edit>> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT t.path, t.session_id, t.seq, e.ts, t.action, t.diff
             FROM file_touches t JOIN events e ON e.session_id = t.session_id AND e.seq = t.seq
             WHERE e.ts IS NOT NULL AND (t.diff IS NOT NULL OR t.action = 'delete')
               AND t.action != 'read'
             ORDER BY e.ts, t.seq",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let path: String = r.get(0)?;
            let action: String = r.get(4)?;
            let diff: Option<String> = r.get(5)?;
            let (added, removed) = diff
                .as_deref()
                .map(|d| added_removed(d, &action))
                .unwrap_or_default();
            by_path.entry(path).or_default().push(Edit {
                session_id: r.get(1)?,
                seq: r.get(2)?,
                ts: r.get(3)?,
                action,
                added,
                removed: removed.into_iter().collect(),
            });
        }
    }

    let tx = conn.transaction()?;
    for (path, edits) in &by_path {
        if edits.iter().all(|e| e.added.is_empty()) {
            continue;
        }
        let file = resolve(path);
        let meta = file.as_ref().and_then(|f| std::fs::metadata(f).ok());
        // Unchanged file and unchanged edits: nothing to do.
        let last = edits.last().map_or("", |e| e.ts.as_str());
        let sig = match &meta {
            Some(m) => format!(
                "{ENGINE}:{}:{}:{}:{last}",
                m.len(),
                m.modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_millis()),
                edits.len()
            ),
            None => format!("{ENGINE}:gone:{}:{last}", edits.len()),
        };
        let stored: Option<String> = tx
            .query_row(
                "SELECT sig FROM survival_files WHERE path = ?",
                [path],
                |r| r.get(0),
            )
            .optional()?;
        if stored.as_deref() == Some(sig.as_str()) {
            continue;
        }
        tx.execute("DELETE FROM survival WHERE path = ?", [path])?;
        tx.execute(
            "INSERT OR REPLACE INTO survival_files (path, sig) VALUES (?, ?)",
            params![path, sig],
        )?;

        let text = match (&file, &meta) {
            (Some(f), Some(m)) if m.is_file() && m.len() <= MAX_FILE => std::fs::read(f)
                .ok()
                .map(|b| String::from_utf8_lossy(&b).into_owned()),
            (Some(_), _) => {
                // Too big, a folder, or unreadable: say nothing.
                report.files_skipped += 1;
                continue;
            }
            (None, _) => None,
        };
        // A file that's gone counts only if its project is still here;
        // otherwise the repo moved or was deleted and we can't tell.
        if text.is_none() {
            let root = crate::project_root(path);
            let parent = Path::new(path).parent();
            let project_here = Path::new(&root).is_dir() && root.len() > 1;
            if !project_here || parent.is_none() {
                report.files_skipped += 1;
                continue;
            }
        }
        let current: Option<HashSet<&str>> = text
            .as_deref()
            .map(|t| t.lines().filter_map(norm).collect());
        let counts = classify(edits, current.as_ref());
        let mut ins = tx.prepare_cached(
            "INSERT INTO survival (session_id, seq, path, ts, added, kept, reworked, changed)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )?;
        for (e, c) in edits.iter().zip(&counts) {
            if c.added > 0 {
                ins.execute(params![
                    e.session_id,
                    e.seq,
                    path,
                    e.ts,
                    c.added,
                    c.kept,
                    c.reworked,
                    c.changed
                ])?;
            }
        }
        report.files_checked += 1;
    }
    tx.commit()?;
    Ok(report)
}

/// Remove survival rows for sessions being rewritten, and let their files
/// be checked again.
pub(crate) fn forget_session(tx: &Transaction, id: &str) -> Result<()> {
    tx.execute(
        "DELETE FROM survival_files WHERE path IN (SELECT path FROM survival WHERE session_id = ?1)",
        [id],
    )?;
    tx.execute("DELETE FROM survival WHERE session_id = ?1", [id])?;
    Ok(())
}

/// Totals for one session and its subagents, or `None` if it added
/// nothing that could be checked.
pub fn session_counts(conn: &Connection, id: &str) -> Result<Option<Counts>> {
    let c = conn.query_row(
        "SELECT coalesce(sum(v.added), 0), coalesce(sum(v.kept), 0), coalesce(sum(v.reworked), 0),
                coalesce(sum(v.changed), 0)
         FROM survival v JOIN sessions s ON s.id = v.session_id
         WHERE s.id = ?1 OR s.parent_id = ?1",
        [id],
        counts_row,
    )?;
    Ok((c.added > 0).then_some(c))
}

fn counts_row(r: &rusqlite::Row) -> rusqlite::Result<Counts> {
    Ok(Counts {
        added: r.get(0)?,
        kept: r.get(1)?,
        reworked: r.get(2)?,
        changed: r.get(3)?,
    })
}

const SUMS: &str = "coalesce(sum(v.added), 0), coalesce(sum(v.kept), 0),
    coalesce(sum(v.reworked), 0), coalesce(sum(v.changed), 0)";

/// Lines grouped by how long ago they were written.
#[derive(Debug, Clone, Serialize)]
pub struct AgeBucket {
    pub label: &'static str,
    pub counts: Counts,
}

#[derive(Debug, Clone, Serialize)]
pub struct WeekRow {
    /// Local date of the Monday the week starts on.
    pub week: String,
    pub counts: Counts,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentModelRow {
    pub agent: String,
    pub model: Option<String>,
    pub sessions: i64,
    pub counts: Counts,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectRow {
    pub project: Option<String>,
    pub sessions: i64,
    pub counts: Counts,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileRow {
    pub path: String,
    pub project: Option<String>,
    pub edits: i64,
    pub counts: Counts,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// Edits at least a day old: settled enough to judge.
    pub settled: Counts,
    /// Edits from the last day; often still being worked on.
    pub recent: Counts,
    pub ages: Vec<AgeBucket>,
    pub weeks: Vec<WeekRow>,
    pub by_agent: Vec<AgentModelRow>,
    pub by_project: Vec<ProjectRow>,
    /// Files whose agent lines were rewritten or changed the most.
    pub files: Vec<FileRow>,
}

/// What lasted, for edits that match `f`.
pub fn report(conn: &Connection, f: &crate::query::Filter, now_ms: i64) -> Result<Report> {
    let (cond, args) = crate::query::conditions(f);
    let from = format!(
        "FROM survival v JOIN events e ON e.session_id = v.session_id AND e.seq = v.seq
         JOIN sessions s ON s.id = v.session_id WHERE {cond}"
    );
    let day = crate::util::format_ts_ms(now_ms - 86_400_000);
    let week = crate::util::format_ts_ms(now_ms - 7 * 86_400_000);
    let month = crate::util::format_ts_ms(now_ms - 30 * 86_400_000);
    let with = |extra: &str, more: &[&str]| {
        let mut a: Vec<String> = args.clone();
        a.extend(more.iter().map(|s| s.to_string()));
        (format!("{from} {extra}"), a)
    };
    let sum = |extra: &str, more: &[&str]| -> Result<Counts> {
        let (sql, a) = with(extra, more);
        Ok(conn.query_row(
            &format!("SELECT {SUMS} {sql}"),
            rusqlite::params_from_iter(&a),
            counts_row,
        )?)
    };
    let settled = sum("AND v.ts < ?", &[&day])?;
    let recent = sum("AND v.ts >= ?", &[&day])?;
    let ages = vec![
        AgeBucket {
            label: "1–7 days ago",
            counts: sum("AND v.ts < ? AND v.ts >= ?", &[&day, &week])?,
        },
        AgeBucket {
            label: "1–4 weeks ago",
            counts: sum("AND v.ts < ? AND v.ts >= ?", &[&week, &month])?,
        },
        AgeBucket {
            label: "Over a month ago",
            counts: sum("AND v.ts < ?", &[&month])?,
        },
    ];

    let (sql, a) = with("GROUP BY 1 ORDER BY 1 DESC LIMIT 12", &[]);
    let mut stmt = conn.prepare(&format!(
        "SELECT date(v.ts, 'localtime', '-6 days', 'weekday 1'), {SUMS} {sql}"
    ))?;
    let mut weeks: Vec<WeekRow> = stmt
        .query_map(rusqlite::params_from_iter(&a), |r| {
            Ok(WeekRow {
                week: r.get(0)?,
                counts: Counts {
                    added: r.get(1)?,
                    kept: r.get(2)?,
                    reworked: r.get(3)?,
                    changed: r.get(4)?,
                },
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    weeks.reverse();

    let (sql, a) = with(
        "AND v.ts < ? GROUP BY 1, 2 ORDER BY sum(v.added) DESC LIMIT 20",
        &[&day],
    );
    let mut stmt = conn.prepare(&format!(
        "SELECT s.agent, s.model, count(DISTINCT coalesce(s.parent_id, s.id)), {SUMS} {sql}"
    ))?;
    let by_agent = stmt
        .query_map(rusqlite::params_from_iter(&a), |r| {
            Ok(AgentModelRow {
                agent: r.get(0)?,
                model: r.get(1)?,
                sessions: r.get(2)?,
                counts: Counts {
                    added: r.get(3)?,
                    kept: r.get(4)?,
                    reworked: r.get(5)?,
                    changed: r.get(6)?,
                },
            })
        })?
        .collect::<std::result::Result<_, _>>()?;

    let (sql, a) = with(
        "AND v.ts < ? GROUP BY 1 ORDER BY sum(v.added) DESC LIMIT 20",
        &[&day],
    );
    let mut stmt = conn.prepare(&format!(
        "SELECT s.project_root, count(DISTINCT coalesce(s.parent_id, s.id)), {SUMS} {sql}"
    ))?;
    let by_project = stmt
        .query_map(rusqlite::params_from_iter(&a), |r| {
            Ok(ProjectRow {
                project: r.get(0)?,
                sessions: r.get(1)?,
                counts: Counts {
                    added: r.get(2)?,
                    kept: r.get(3)?,
                    reworked: r.get(4)?,
                    changed: r.get(5)?,
                },
            })
        })?
        .collect::<std::result::Result<_, _>>()?;

    let (sql, a) = with(
        "AND v.ts < ? GROUP BY v.path HAVING sum(v.added) >= 5
         ORDER BY sum(v.reworked) + sum(v.changed) DESC, sum(v.added) DESC LIMIT 15",
        &[&day],
    );
    let mut stmt = conn.prepare(&format!(
        "SELECT v.path, max(s.project_root), count(*), {SUMS} {sql}"
    ))?;
    let files = stmt
        .query_map(rusqlite::params_from_iter(&a), |r| {
            Ok(FileRow {
                path: r.get(0)?,
                project: r.get(1)?,
                edits: r.get(2)?,
                counts: Counts {
                    added: r.get(3)?,
                    kept: r.get(4)?,
                    reworked: r.get(5)?,
                    changed: r.get(6)?,
                },
            })
        })?
        .collect::<std::result::Result<_, _>>()?;

    Ok(Report {
        settled,
        recent,
        ages,
        weeks,
        by_agent,
        by_project,
        files,
    })
}

/// The path a worktree file has in its main repo, by name only:
/// `repo/.claude/worktrees/x/src/a.ts` is `repo/src/a.ts`.
fn main_path(path: &str) -> String {
    match path.find("/.claude/worktrees/") {
        Some(i) => {
            let rest = &path[i + "/.claude/worktrees/".len()..];
            match rest.split_once('/') {
                Some((_, inner)) => format!("{}/{inner}", &path[..i]),
                None => path.to_string(),
            }
        }
        None => path.to_string(),
    }
}

/// One agent edit that wrote lines still in the file.
#[derive(Debug, Clone, Serialize)]
pub struct BlameEdit {
    pub session_id: String,
    pub seq: i64,
    pub ts: String,
    pub agent: String,
    pub model: Option<String>,
    pub session_title: Option<String>,
    /// The prompt of the turn the edit was made in (first 300 characters).
    pub prompt: Option<String>,
    /// Lines of today's file it wrote.
    pub lines: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlameLine {
    pub n: usize,
    pub text: String,
    /// Index into `edits`; `None` when no agent edit wrote this line.
    pub edit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Blame {
    pub path: String,
    /// The file could be read today.
    pub found: bool,
    pub too_big: bool,
    pub lines: Vec<BlameLine>,
    pub edits: Vec<BlameEdit>,
    pub agent_lines: usize,
}

/// Which agent edit wrote each line of `path` as it is on disk today.
/// A line belongs to the latest edit that added the same trimmed text.
/// Read on demand; nothing is stored.
pub fn blame(conn: &Connection, path: &str) -> Result<Blame> {
    let root = crate::project_root(path);
    let rel = path
        .strip_prefix(&root)
        .map(|r| r.trim_start_matches('/'))
        .filter(|r| !r.is_empty())
        .unwrap_or(path);
    let mut stmt = conn.prepare(
        "SELECT t.path, t.session_id, t.seq, e.ts, t.action, t.diff, s.agent, coalesce(e.model, s.model), s.title
         FROM file_touches t
         JOIN events e ON e.session_id = t.session_id AND e.seq = t.seq
         JOIN sessions s ON s.id = t.session_id
         WHERE e.ts IS NOT NULL AND t.diff IS NOT NULL AND t.action != 'read'
           AND (t.path = ?1 OR (t.path LIKE '%/.claude/worktrees/%' AND t.path LIKE '%/' || ?2))
         ORDER BY e.ts, t.seq",
    )?;
    struct Row {
        edit: BlameEdit,
        added: Vec<String>,
    }
    let mut rows: Vec<Row> = Vec::new();
    let mut q = stmt.query(params![path, rel])?;
    while let Some(r) = q.next()? {
        let p: String = r.get(0)?;
        if main_path(&p) != path {
            continue;
        }
        let action: String = r.get(4)?;
        let diff: String = r.get(5)?;
        let (added, _) = added_removed(&diff, &action);
        rows.push(Row {
            edit: BlameEdit {
                session_id: r.get(1)?,
                seq: r.get(2)?,
                ts: r.get(3)?,
                agent: r.get(6)?,
                model: r.get(7)?,
                session_title: r.get(8)?,
                prompt: None,
                lines: 0,
            },
            added,
        });
    }
    drop(q);
    drop(stmt);

    let file = resolve(path);
    let meta = file.as_ref().and_then(|f| std::fs::metadata(f).ok());
    let mut out = Blame {
        path: path.to_string(),
        found: meta.as_ref().is_some_and(|m| m.is_file()),
        too_big: meta.as_ref().is_some_and(|m| m.len() > MAX_FILE),
        lines: Vec::new(),
        edits: Vec::new(),
        agent_lines: 0,
    };
    // Only files an agent edited are read: the API never serves files
    // that no session touched.
    if !out.found || out.too_big || rows.is_empty() {
        return Ok(out);
    }
    let text = file
        .and_then(|f| std::fs::read(f).ok())
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();

    // Latest edit wins for each trimmed line.
    let mut owner: HashMap<&str, usize> = HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        for l in &r.added {
            owner.insert(l.as_str(), i);
        }
    }
    let mut lines: Vec<(String, Option<usize>)> = text
        .lines()
        .map(|l| (l.to_string(), norm(l).and_then(|n| owner.get(n).copied())))
        .collect();
    // Braces and blank lines between two lines of the same edit belong to it.
    let mut i = 0;
    while i < lines.len() {
        if lines[i].1.is_none() && norm(&lines[i].0).is_none() {
            let start = i;
            while i < lines.len() && lines[i].1.is_none() && norm(&lines[i].0).is_none() {
                i += 1;
            }
            let before = start.checked_sub(1).and_then(|b| lines[b].1);
            let after = lines.get(i).and_then(|l| l.1);
            if before.is_some() && before == after {
                for l in &mut lines[start..i] {
                    l.1 = before;
                }
            }
        } else {
            i += 1;
        }
    }

    // Keep only edits that still own lines, in time order.
    let mut used: Vec<usize> = lines.iter().filter_map(|l| l.1).collect();
    used.sort_unstable();
    used.dedup();
    let index: HashMap<usize, usize> = used.iter().enumerate().map(|(n, &o)| (o, n)).collect();
    let mut prompt = conn.prepare(
        "SELECT substr(p.body, 1, 300) FROM turns t
         JOIN events p ON p.session_id = t.session_id AND p.seq = t.seq
         WHERE t.session_id = ?1 AND t.seq <= ?2 AND t.end_seq >= ?2 LIMIT 1",
    )?;
    for &o in &used {
        let mut e = rows[o].edit.clone();
        e.prompt = prompt
            .query_row(params![e.session_id, e.seq], |r| r.get(0))
            .optional()?;
        out.edits.push(e);
    }
    for (n, (text, o)) in lines.into_iter().enumerate() {
        let edit = o.map(|o| index[&o]);
        if let Some(e) = edit {
            out.edits[e].lines += 1;
            out.agent_lines += 1;
        }
        out.lines.push(BlameLine {
            n: n + 1,
            text,
            edit,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(action: &str, diff: &str) -> Edit {
        let (added, removed) = added_removed(diff, action);
        Edit {
            session_id: "s".into(),
            seq: 0,
            ts: String::new(),
            action: action.into(),
            added,
            removed: removed.into_iter().collect(),
        }
    }

    #[test]
    fn reads_added_and_removed_lines() {
        let (a, d) = added_removed(
            "@@ -1,3 +1,3 @@\n fn main() {\n-    old_call();\n+    new_call(1);\n+}\n }",
            "edit",
        );
        assert_eq!(a, vec!["new_call(1);"]);
        assert_eq!(d, vec!["old_call();"]);
        let (a, _) = added_removed("let x = 1;\n\n}\nreturn x;", "create");
        assert_eq!(a, vec!["let x = 1;", "return x;"]);
        let (a, d) = added_removed("@@ -1 +1 @@\n-old line here\n+new line here", "create");
        assert_eq!(
            (a, d),
            (vec!["new line here".into()], vec!["old line here".into()])
        );
        // A diff cut at the size limit loses its partial last line.
        let (a, _) = added_removed("+let one = 1;\n+let tw…", "edit");
        assert_eq!(a, vec!["let one = 1;"]);
    }

    #[test]
    fn sorts_lines_into_kept_reworked_changed() {
        let edits = vec![
            edit(
                "create",
                "fn alpha() {}\nfn beta() {}\nfn gamma() {}\nfn delta() {}",
            ),
            edit("edit", "@@ -2 +2 @@\n-fn beta() {}\n+fn beta2() {}"),
        ];
        let now: HashSet<&str> = ["fn alpha() {}", "fn beta2() {}", "fn gamma() {}  "]
            .iter()
            .filter_map(|l| norm(l))
            .collect();
        let c = classify(&edits, Some(&now));
        assert_eq!(
            c[0],
            Counts {
                added: 4,
                kept: 2,
                reworked: 1,
                changed: 1
            },
            "alpha and gamma kept, beta reworked by the later edit, delta changed by someone else"
        );
        assert_eq!(c[1].kept, 1);

        // The agent deleted the file later: everything it had added was reworked.
        let gone = vec![edit("create", "fn alpha() {}"), edit("delete", "")];
        assert_eq!(classify(&gone, None)[0].reworked, 1);
        // Gone without an agent deleting it: changed.
        assert_eq!(classify(&gone[..1], None)[0].changed, 1);
    }

    #[test]
    fn skips_lines_with_no_signal() {
        assert_eq!(norm("  }  "), None);
        assert_eq!(norm("});"), None);
        assert_eq!(norm("x = 1"), None);
        assert_eq!(norm("  return ok;"), Some("return ok;"));
    }
}
