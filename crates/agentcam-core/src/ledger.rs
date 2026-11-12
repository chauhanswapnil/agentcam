//! What became of each fix: the ones marked done, with the weeks before
//! and the time since side by side, and the ones set aside.
//!
//! Marks live in `profile.json` next to the index, not in it, so they
//! survive a rebuild. When a fix is marked done, the four weeks before are
//! measured once and kept; the time since is measured on demand. Both are
//! given as a rate per 10 hours of agent time, so a busy week and a quiet
//! one compare fairly. Nothing is claimed until a full week has passed.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::index::Result;
use crate::prices::Prices;
use crate::query::Filter;
use crate::util::{format_ts_ms, parse_ts_ms};

const DAY_MS: i64 = 86_400_000;
/// How far back the "before" goes.
const BEFORE_DAYS: i64 = 28;
/// Before this much has passed since the fix, the change isn't stated.
const ENOUGH_DAYS: f64 = 7.0;
const ENOUGH_SESSIONS: i64 = 5;
const TEN_HOURS_MS: f64 = 36_000_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// The person made the change.
    Applied,
    /// Not for them: don't show it again.
    Dismissed,
}

/// What a fix measured over a stretch of time.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Window {
    pub days: f64,
    pub count: i64,
    pub ms: i64,
    pub usd: f64,
    /// Estimated tokens printed, for loud commands.
    #[serde(default)]
    pub tokens: i64,
    pub agent_ms: i64,
    pub sessions: i64,
}

impl Window {
    /// The fix's measure (time, dollars for a cost fix, tokens for a loud
    /// command) per 10 hours of agent time.
    fn rate(&self, kind: &str) -> Option<f64> {
        let amount = match kind {
            "cache" => self.usd,
            "loud_command" => self.tokens as f64,
            _ => self.ms as f64,
        };
        (self.agent_ms > 0).then(|| amount * TEN_HOURS_MS / self.agent_ms as f64)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mark {
    pub id: String,
    pub kind: String,
    /// The card's title when it was marked.
    pub title: String,
    pub state: State,
    pub at: String,
    /// The four weeks before, kept for fixes marked done.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<Window>,
}

/// The contents of `profile.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub fixes: Vec<Mark>,
}

pub fn path(db: &Path) -> PathBuf {
    db.with_file_name("profile.json")
}

/// A missing or unreadable file is an empty store.
pub fn load(path: &Path) -> Store {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, s: &Store) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(s).unwrap_or_default())?;
    std::fs::rename(tmp, path)
}

impl Store {
    pub fn state(&self, id: &str) -> Option<State> {
        self.fixes.iter().find(|m| m.id == id).map(|m| m.state)
    }
}

fn kind_of(id: &str) -> &str {
    match id.split(':').next() {
        Some("approval") => "approval",
        Some("slow") => "slow_command",
        Some("loud") => "loud_command",
        Some("cache") => "cache",
        _ => "other",
    }
}

/// What the fix `id` measured between two times.
fn measure(
    conn: &Connection,
    prices: &Prices,
    id: &str,
    since: i64,
    until: i64,
) -> Result<(Window, Option<String>)> {
    let f = Filter {
        since: Some(format_ts_ms(since)),
        until: Some(format_ts_ms(until)),
        ..Default::default()
    };
    let found = crate::fixes::find(conn, prices, &f, false)?;
    let fix = found.fixes.iter().find(|x| x.id == id);
    Ok((
        Window {
            days: (until - since) as f64 / DAY_MS as f64,
            count: fix.map_or(0, |x| x.count),
            ms: fix.and_then(|x| x.total_ms).unwrap_or(0),
            usd: fix.and_then(|x| x.total_usd).unwrap_or(0.0),
            tokens: fix.and_then(|x| x.total_tokens).unwrap_or(0),
            agent_ms: found.agent_ms,
            sessions: found.sessions,
        },
        fix.map(|x| x.title.clone()),
    ))
}

/// Set a fix's state, or clear it with `None`. Marking one done measures
/// the four weeks before, once.
pub fn mark(
    conn: &Connection,
    prices: &Prices,
    store: &mut Store,
    id: &str,
    state: Option<State>,
    now: i64,
) -> Result<()> {
    let old = store
        .fixes
        .iter()
        .position(|m| m.id == id)
        .map(|i| store.fixes.remove(i));
    let Some(state) = state else { return Ok(()) };
    let (before, title) = measure(conn, prices, id, now - BEFORE_DAYS * DAY_MS, now)?;
    store.fixes.push(Mark {
        id: id.to_string(),
        kind: kind_of(id).to_string(),
        title: title
            .or(old.map(|m| m.title))
            .unwrap_or_else(|| id.to_string()),
        state,
        at: format_ts_ms(now),
        before: (state == State::Applied).then_some(before),
    });
    Ok(())
}

/// A fix marked done, measured.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Measured {
    #[serde(flatten)]
    pub mark: Mark,
    pub after: Window,
    /// Per 10 hours of agent time: milliseconds, dollars for a cost fix, or
    /// tokens for a loud command.
    pub before_rate: Option<f64>,
    pub after_rate: Option<f64>,
    /// `after_rate / before_rate - 1`, once there is enough to go on.
    pub change: Option<f64>,
    /// A full week and a handful of sessions have passed since.
    pub enough: bool,
    /// Days still to go before the change is stated.
    pub days_left: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Ledger {
    /// Newest first.
    pub applied: Vec<Measured>,
    pub dismissed: Vec<Mark>,
}

pub fn ledger(conn: &Connection, prices: &Prices, store: &Store, now: i64) -> Result<Ledger> {
    let mut out = Ledger::default();
    for m in &store.fixes {
        if m.state == State::Dismissed {
            out.dismissed.push(m.clone());
            continue;
        }
        let at = parse_ts_ms(&m.at).unwrap_or(now).min(now);
        let (after, _) = measure(conn, prices, &m.id, at, now)?;
        let before_rate = m.before.as_ref().and_then(|w| w.rate(&m.kind));
        let after_rate = after.rate(&m.kind);
        let enough = after.days >= ENOUGH_DAYS && after.sessions >= ENOUGH_SESSIONS;
        let change = match (before_rate, after_rate) {
            (Some(b), Some(a)) if enough && b > 0.0 => Some(a / b - 1.0),
            _ => None,
        };
        out.applied.push(Measured {
            mark: m.clone(),
            days_left: (ENOUGH_DAYS - after.days).max(0.0),
            after,
            before_rate,
            after_rate,
            change,
            enough,
        });
    }
    out.applied.sort_by(|a, b| b.mark.at.cmp(&a.mark.at));
    Ok(out)
}

/// One week of a fix's measure.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Week {
    /// The week's first moment.
    pub start: String,
    /// How many times the fix's pattern happened.
    pub count: i64,
    /// Milliseconds, tokens or (for a cache fix) dollars, by the fix's kind.
    pub value: f64,
    /// Agent time in the week, so the UI can show a rate.
    pub agent_ms: i64,
}

const WEEKS: usize = 12;
const WEEK_MS: i64 = 7 * DAY_MS;

/// The fix's measure for each of the last 12 weeks, oldest first, with agent
/// time per week so the UI can show a rate. The last week ends at `now`.
pub fn weeks(conn: &Connection, prices: &Prices, id: &str, now: i64) -> Result<Vec<Week>> {
    let start = now - WEEKS as i64 * WEEK_MS;
    let f = Filter {
        since: Some(format_ts_ms(start)),
        until: Some(format_ts_ms(now)),
        ..Default::default()
    };
    let mut out: Vec<Week> = (0..WEEKS as i64)
        .map(|i| Week {
            start: format_ts_ms(start + i * WEEK_MS),
            count: 0,
            value: 0.0,
            agent_ms: 0,
        })
        .collect();
    let mut put = |ts: Option<i64>, value: f64| {
        if let Some(i) = ts
            .map(|t| (t - start) / WEEK_MS)
            .filter(|i| (0..WEEKS as i64).contains(i))
        {
            out[i as usize].count += 1;
            out[i as usize].value += value;
        }
    };
    // One pass over the fix's occurrences, not one fix search per week.
    if let Some(cause) = id.strip_prefix("cache:") {
        for b in crate::cache::cache_in_full(conn, prices, &f)?.latest {
            let name = serde_json::to_value(b.cause).ok();
            if name.as_ref().and_then(|v| v.as_str()) == Some(cause) {
                put(b.ts.as_deref().and_then(parse_ts_ms), b.usd.unwrap_or(0.0));
            }
        }
    } else if let Some(o) = crate::fixes::occurrences(conn, prices, &f, id)? {
        for x in o.items {
            put(x.ts, x.value as f64);
        }
    }
    // Agent time per week: the capped gaps, as the Overview sums them.
    let mut stmt = conn.prepare(
        "SELECT CAST((CAST(strftime('%s', e.ts) AS INTEGER) * 1000 - ?1) / ?2 AS INTEGER) AS w, sum(e.gap_ms)
         FROM events e JOIN sessions s ON s.id = e.session_id
         WHERE e.ts >= ?3 AND e.ts < ?4 GROUP BY w",
    )?;
    let rows = stmt.query_map(
        rusqlite::params![start, WEEK_MS, format_ts_ms(start), format_ts_ms(now)],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
    )?;
    for row in rows {
        let (w, ms) = row?;
        if (0..WEEKS as i64).contains(&w) {
            out[w as usize].agent_ms = ms;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Index;

    /// Sessions of one prompt and `runs` 90-second test runs, on the days
    /// given (day 0 is 2026-10-01).
    fn index(days: &[(i64, i64)]) -> Index {
        let idx = Index::open_in_memory().unwrap();
        for (n, (day, runs)) in days.iter().enumerate() {
            let sid = format!("s{n}");
            idx.conn()
                .execute(
                    "INSERT INTO sessions (id, agent, source_path) VALUES (?1, 'claude-code', '/Users/dev/x.jsonl')",
                    [&sid],
                )
                .unwrap();
            let at = |min: i64| format_ts_ms(day_ms(*day) + min * 60_000);
            idx.conn()
                .execute(
                    "INSERT INTO events (session_id, seq, ts, kind, title, body, source_offset, gap_ms)
                     VALUES (?1, 0, ?2, 'prompt', '', '', 0, 0)",
                    rusqlite::params![sid, at(0)],
                )
                .unwrap();
            // An hour of agent time a session, whatever it ran.
            for i in 0..12 {
                let run = i < *runs;
                idx.conn()
                    .execute(
                        "INSERT INTO events (session_id, seq, ts, kind, category, tool_name, title, body,
                            source_offset, gap_ms, duration_ms, approval, input)
                         VALUES (?1, ?2, ?3, 'tool_call', ?4, ?5, '', '', 0, 300000, ?6, 'rule', ?7)",
                        rusqlite::params![
                            sid,
                            i + 1,
                            at(5 * (i + 1)),
                            if run { "command" } else { "read" },
                            if run { "Bash" } else { "Read" },
                            if run { 90_000 } else { 1_000 },
                            if run { r#"{"command":"cargo test"}"# } else { "{}" },
                        ],
                    )
                    .unwrap();
            }
        }
        idx
    }

    fn day_ms(day: i64) -> i64 {
        parse_ts_ms("2026-10-01T09:00:00.000Z").unwrap() + day * DAY_MS
    }

    #[test]
    fn a_fix_marked_done_is_measured_before_and_after() {
        // Four test runs a session for three weeks, then one a session.
        let mut days: Vec<(i64, i64)> = (0..20).map(|d| (d, 4)).collect();
        days.extend((20..30).map(|d| (d, 1)));
        let idx = index(&days);
        let mut store = Store::default();
        let applied_at = day_ms(20) - 3_600_000;
        mark(
            idx.conn(),
            idx.prices(),
            &mut store,
            "slow:cargo test",
            Some(State::Applied),
            applied_at,
        )
        .unwrap();
        let m = &store.fixes[0];
        assert_eq!((m.kind.as_str(), m.state), ("slow_command", State::Applied));
        assert!(
            m.title.starts_with("`cargo test` ran 80 times"),
            "{}",
            m.title
        );
        let before = m.before.as_ref().unwrap();
        assert_eq!(
            (before.count, before.ms, before.sessions),
            (80, 80 * 90_000, 20)
        );

        // Three days on: too soon to say.
        let l = ledger(idx.conn(), idx.prices(), &store, day_ms(23)).unwrap();
        let x = &l.applied[0];
        assert!(!x.enough && x.change.is_none());
        assert!(x.days_left > 3.0 && x.days_left < 4.5, "{}", x.days_left);

        // Ten days on: a quarter of the runs for the same agent time.
        let l = ledger(idx.conn(), idx.prices(), &store, day_ms(30)).unwrap();
        let x = &l.applied[0];
        assert!(x.enough);
        assert_eq!((x.after.count, x.after.sessions), (10, 10));
        // 6 min per agent-hour before, 1.5 after: an hour and 15 minutes per ten.
        assert!((x.before_rate.unwrap() - 3_600_000.0).abs() < 1.0);
        assert!((x.after_rate.unwrap() - 900_000.0).abs() < 1.0);
        assert!((x.change.unwrap() + 0.75).abs() < 1e-9);
    }

    #[test]
    fn a_fix_that_changed_nothing_says_so_and_marks_can_be_cleared() {
        let idx = index(&(0..30).map(|d| (d, 4)).collect::<Vec<_>>());
        let mut store = Store::default();
        mark(
            idx.conn(),
            idx.prices(),
            &mut store,
            "slow:cargo test",
            Some(State::Applied),
            day_ms(20) - 1,
        )
        .unwrap();
        let l = ledger(idx.conn(), idx.prices(), &store, day_ms(30)).unwrap();
        assert!(
            l.applied[0].change.unwrap().abs() < 1e-9,
            "no change, stated as none"
        );

        // Set aside: kept with its title, no measuring.
        mark(
            idx.conn(),
            idx.prices(),
            &mut store,
            "slow:cargo test",
            Some(State::Dismissed),
            day_ms(30),
        )
        .unwrap();
        assert_eq!(store.state("slow:cargo test"), Some(State::Dismissed));
        assert_eq!(store.fixes.len(), 1);
        assert!(store.fixes[0].before.is_none());
        let l = ledger(idx.conn(), idx.prices(), &store, day_ms(30)).unwrap();
        assert_eq!((l.applied.len(), l.dismissed.len()), (0, 1));
        // Cleared: back to nothing.
        mark(
            idx.conn(),
            idx.prices(),
            &mut store,
            "slow:cargo test",
            None,
            day_ms(30),
        )
        .unwrap();
        assert_eq!(store, Store::default());
    }

    #[test]
    fn weeks_count_a_fix_per_week_with_the_agent_time_beside_it() {
        // Test runs in weeks 1 to 3 of the sessions and none in week 4.
        let idx = index(&[(0, 4), (8, 2), (15, 1), (22, 0)]);
        // The twelve weeks end 4 weeks after day 0, so day 0 starts week 8.
        let now = day_ms(0) + 28 * DAY_MS;
        let w = weeks(idx.conn(), idx.prices(), "slow:cargo test", now).unwrap();
        assert_eq!(w.len(), 12);
        let counts: Vec<i64> = w.iter().map(|x| x.count).collect();
        assert_eq!(counts[..8], [0; 8]);
        assert_eq!(counts[8..], [4, 2, 1, 0]);
        assert_eq!(w[8].value, 4.0 * 90_000.0);
        // An hour of agent time per session, in the week it ran.
        assert!(w[8..].iter().all(|x| x.agent_ms > 0), "{w:?}");
        assert!(w[..8].iter().all(|x| x.agent_ms == 0));
        // A near miss: a command that isn't a fix has no occurrences.
        let none = weeks(idx.conn(), idx.prices(), "slow:make", now).unwrap();
        assert!(none.iter().all(|x| x.count == 0));
    }

    #[test]
    fn saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("agentcam-ledger-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("profile.json");
        assert_eq!(load(&p), Store::default(), "missing file: no marks");
        let idx = index(&[(0, 4)]);
        let mut store = Store::default();
        mark(
            idx.conn(),
            idx.prices(),
            &mut store,
            "cache:break",
            Some(State::Dismissed),
            day_ms(1),
        )
        .unwrap();
        save(&p, &store).unwrap();
        assert_eq!(load(&p), store);
        // A fix no longer found keeps its id as the title.
        assert_eq!(store.fixes[0].title, "cache:break");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
