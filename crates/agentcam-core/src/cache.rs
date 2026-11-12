//! The prompt cache: how much of each prompt was read from it, and the
//! calls where it was lost and written again ("rebuilds").
//!
//! A cache read costs a fraction of fresh input and a write costs more than
//! it, so losing the cache on a big session means paying to write the whole
//! prompt again. The logs don't hold the prompt, so the cause of a rebuild
//! is read from what they do show just before it, and is left unnamed when
//! they show nothing.

use rusqlite::{Connection, params_from_iter};
use serde::Serialize;

use crate::index::{Result, usage_from_row};
use crate::model::TokenUsage;
use crate::prices::Prices;
use crate::query::{Filter, conditions};
use crate::util::parse_ts_ms;

/// Smaller writes than this are ordinary turn-by-turn growth.
const MIN_REBUILD_TOKENS: u64 = 10_000;
const FIVE_MINUTES_MS: i64 = 5 * 60_000;
const ONE_HOUR_MS: i64 = 60 * 60_000;
const LATEST: usize = 20;

/// Why the cache was written again, as far as the log shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Cause {
    /// The call used another model than the one before; caches are per model.
    ModelSwitch,
    /// The turn ran at another effort than the one before, which changes
    /// the prompt the cache is keyed on.
    EffortChange,
    /// More time passed since the last call than the cache is kept for.
    Break,
    /// Nothing in the log explains it.
    Unknown,
}

/// One call that wrote the prompt again.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Rebuild {
    pub session_id: String,
    pub seq: i64,
    pub ts: Option<String>,
    pub cause: Cause,
    /// Tokens written to the cache by this call.
    pub tokens: i64,
    /// What writing them cost over reading them, when the model has a price.
    pub usd: Option<f64>,
    /// Time since the call before, uncapped.
    pub gap_ms: Option<i64>,
    pub model: Option<String>,
    /// The model of the call before, for a model switch.
    pub from_model: Option<String>,
    /// The efforts before and after, for an effort change.
    pub from_effort: Option<String>,
    pub effort: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct CauseTotal {
    pub cause: Option<Cause>,
    pub count: i64,
    pub tokens: i64,
    pub usd: f64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Cache {
    /// Share of prompt tokens read from the cache, 0 to 1.
    pub hit_rate: Option<f64>,
    pub calls: i64,
    pub rebuilds: i64,
    pub rebuild_tokens: i64,
    pub rebuild_usd: f64,
    /// Most tokens first.
    pub by_cause: Vec<CauseTotal>,
    /// Compactions rewrite the prompt on purpose; counted apart.
    pub compactions: i64,
    /// Newest first, a short list (every one, for a single session).
    pub latest: Vec<Rebuild>,
}

const COLUMNS: &str = "e.session_id, e.seq, e.ts, e.title, s.title,
     e.model, e.tok_in, e.tok_out, e.tok_cache_read, e.tok_cache_write, e.tok_cache_write_1h, e.tier";
const WANTED: &str = "(e.tok_in + e.tok_out + e.tok_cache_read + e.tok_cache_write > 0
     OR (e.kind = 'system' AND e.title = 'Context compacted'))";

/// The cache across everything a filter covers.
pub fn cache(conn: &Connection, prices: &Prices, f: &Filter) -> Result<Cache> {
    let (cond, args) = conditions(f);
    let sql = format!(
        "SELECT {COLUMNS} FROM events e JOIN sessions s ON s.id = e.session_id
         WHERE {cond} AND {WANTED} ORDER BY e.session_id, e.seq"
    );
    collect(conn, prices, &sql, &args, LATEST)
}

/// The cache in one session and its subagents, with every rebuild listed.
pub fn session_cache(conn: &Connection, prices: &Prices, id: &str) -> Result<Cache> {
    let sql = format!(
        "SELECT {COLUMNS} FROM events e JOIN sessions s ON s.id = e.session_id
         WHERE (s.id = ?1 OR s.parent_id = ?1) AND {WANTED} ORDER BY e.session_id, e.seq"
    );
    collect(conn, prices, &sql, &[id.to_string()], usize::MAX)
}

/// The last model call seen in a session.
struct Last {
    ts: Option<i64>,
    model: Option<String>,
    /// The effort of the turn it ran in.
    effort: Option<String>,
    /// It read at least half its prompt from the cache: there was a cache
    /// to lose.
    warm: bool,
    /// How long the session's cache writes are kept.
    lifetime_ms: i64,
    compacted_since: bool,
}

/// What a call's cache writes cost over reading the same tokens.
fn premium(prices: &Prices, u: &TokenUsage) -> Option<f64> {
    let written = prices.parts(u)?.cache_write;
    let as_read = prices
        .parts(&TokenUsage {
            cache_read: u.cache_write,
            cache_write: 0,
            cache_write_1h: 0,
            input: u.context() - u.cache_write,
            ..u.clone()
        })?
        .cache_read;
    Some((written - as_read).max(0.0))
}

fn collect(
    conn: &Connection,
    prices: &Prices,
    sql: &str,
    args: &[String],
    keep: usize,
) -> Result<Cache> {
    let mut out = Cache::default();
    let (mut prompt, mut read) = (0u64, 0u64);
    let mut session = String::new();
    let mut last: Option<Last> = None;
    let mut found: Vec<Rebuild> = Vec::new();

    // Each turn's effort, to tell when it changed: session -> (first seq, effort).
    let mut efforts: std::collections::HashMap<String, Vec<(i64, Option<String>)>> =
        Default::default();
    {
        let mut stmt =
            conn.prepare("SELECT session_id, seq, effort FROM turns ORDER BY session_id, seq")?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            efforts
                .entry(r.get(0)?)
                .or_default()
                .push((r.get(1)?, r.get(2)?));
        }
    }
    let effort_at = |session: &str, seq: i64| -> Option<String> {
        let turns = efforts.get(session)?;
        let i = turns.partition_point(|(start, _)| *start <= seq);
        turns.get(i.checked_sub(1)?)?.1.clone()
    };
    // A log copied on resume or fork repeats its calls: count each once.
    let mut seen: std::collections::HashSet<(Option<String>, u64)> = Default::default();

    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query(params_from_iter(args))?;
    while let Some(r) = rows.next()? {
        let id: String = r.get(0)?;
        if id != session {
            session = id.clone();
            last = None;
        }
        let u = usage_from_row(r, 5)?;
        if u.context() + u.output == 0 {
            // The compaction step itself.
            out.compactions += 1;
            if let Some(l) = last.as_mut() {
                l.compacted_since = true;
            }
            continue;
        }
        let ts_text: Option<String> = r.get(2)?;
        let ts = ts_text.as_deref().and_then(parse_ts_ms);
        out.calls += 1;
        prompt += u.context();
        read += u.cache_read;

        let seq: i64 = r.get(1)?;
        let effort = effort_at(&id, seq);
        let gap = last.as_ref().and_then(|l| Some(ts? - l.ts?));
        let rewritten = u.cache_write >= MIN_REBUILD_TOKENS && u.cache_write * 2 >= u.context();
        if let Some(l) = last
            .as_ref()
            .filter(|l| l.warm && !l.compacted_since && rewritten)
            .filter(|_| seen.insert((ts_text.clone(), u.cache_write)))
        {
            let cause = if l.model != u.model {
                Cause::ModelSwitch
            } else if gap.is_some_and(|g| g > l.lifetime_ms) {
                Cause::Break
            } else if l.effort.is_some() && effort.is_some() && l.effort != effort {
                Cause::EffortChange
            } else {
                Cause::Unknown
            };
            found.push(Rebuild {
                session_id: id,
                seq,
                ts: ts_text,
                cause,
                tokens: u.cache_write as i64,
                usd: premium(prices, &u),
                gap_ms: gap,
                model: u.model.clone(),
                from_model: (cause == Cause::ModelSwitch)
                    .then(|| l.model.clone())
                    .flatten(),
                from_effort: (cause == Cause::EffortChange)
                    .then(|| l.effort.clone())
                    .flatten(),
                effort: (cause == Cause::EffortChange)
                    .then(|| effort.clone())
                    .flatten(),
                title: r.get(4)?,
            });
        }

        // A cache is there to lose once a call has read most of its prompt
        // from it; a rebuild leaves one behind too.
        let warm = u.cache_read * 2 >= u.context() || rewritten;
        let lifetime_ms = if u.cache_write > 0 {
            if u.cache_write_1h * 2 >= u.cache_write {
                ONE_HOUR_MS
            } else {
                FIVE_MINUTES_MS
            }
        } else {
            last.as_ref().map_or(FIVE_MINUTES_MS, |l| l.lifetime_ms)
        };
        last = Some(Last {
            ts: ts.or(last.as_ref().and_then(|l| l.ts)),
            model: u.model,
            effort,
            warm: warm || last.as_ref().is_some_and(|l| l.warm && !rewritten),
            lifetime_ms,
            compacted_since: false,
        });
    }

    out.hit_rate = (prompt > 0).then(|| read as f64 / prompt as f64);
    out.rebuilds = found.len() as i64;
    let mut causes: std::collections::BTreeMap<Cause, CauseTotal> = Default::default();
    for b in &found {
        out.rebuild_tokens += b.tokens;
        out.rebuild_usd += b.usd.unwrap_or(0.0);
        let c = causes.entry(b.cause).or_insert_with(|| CauseTotal {
            cause: Some(b.cause),
            ..Default::default()
        });
        c.count += 1;
        c.tokens += b.tokens;
        c.usd += b.usd.unwrap_or(0.0);
    }
    out.by_cause = causes.into_values().collect();
    out.by_cause.sort_by_key(|c| std::cmp::Reverse(c.tokens));
    found.sort_by(|a, b| b.ts.cmp(&a.ts));
    found.truncate(keep);
    out.latest = found;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Index;

    /// (minute, model, fresh, read, written, written for an hour)
    type Call = (i64, &'static str, i64, i64, i64, i64);

    fn index(session: &str, calls: &[Call], compact_before: Option<usize>) -> Index {
        let idx = Index::open_in_memory().unwrap();
        idx.conn()
            .execute(
                "INSERT INTO sessions (id, agent, source_path, title) VALUES (?1, 'claude-code', '/Users/dev/x.jsonl', 'Fix login')",
                [session],
            )
            .unwrap();
        let mut seq = 0;
        for (i, (min, model, fresh, read, write, hour)) in calls.iter().enumerate() {
            let ts = format!("2026-11-03T{:02}:{:02}:00.000Z", 10 + min / 60, min % 60);
            if compact_before == Some(i) {
                idx.conn()
                    .execute(
                        "INSERT INTO events (session_id, seq, ts, kind, title, body, source_offset)
                         VALUES (?1, ?2, ?3, 'system', 'Context compacted', '', 0)",
                        rusqlite::params![session, seq, ts],
                    )
                    .unwrap();
                seq += 1;
            }
            idx.conn()
                .execute(
                    "INSERT INTO events (session_id, seq, ts, kind, title, body, source_offset, model,
                        tok_in, tok_out, tok_cache_read, tok_cache_write, tok_cache_write_1h)
                     VALUES (?1, ?2, ?3, 'response', '', '', 0, ?4, ?5, 100, ?6, ?7, ?8)",
                    rusqlite::params![session, seq, ts, model, fresh, read, write, hour],
                )
                .unwrap();
            seq += 1;
        }
        idx
    }

    const OPUS: &str = "claude-opus-5-5";

    fn run(calls: &[Call], compact_before: Option<usize>) -> Cache {
        let idx = index("s", calls, compact_before);
        cache(idx.conn(), idx.prices(), &Filter::default()).unwrap()
    }

    #[test]
    fn a_break_longer_than_the_cache_is_kept_rebuilds_it() {
        let c = run(
            &[
                // The first call writes the cache: nothing was lost.
                (0, OPUS, 1_000, 0, 100_000, 100_000),
                (1, OPUS, 1_000, 100_000, 2_000, 2_000),
                // 90 minutes later the hour-long cache is gone.
                (91, OPUS, 1_000, 0, 104_000, 104_000),
                (92, OPUS, 1_000, 104_000, 1_000, 1_000),
            ],
            None,
        );
        assert_eq!((c.calls, c.rebuilds, c.rebuild_tokens), (4, 1, 104_000));
        let b = &c.latest[0];
        assert_eq!(
            (b.cause, b.seq, b.gap_ms),
            (Cause::Break, 2, Some(90 * 60_000))
        );
        assert_eq!(b.title.as_deref(), Some("Fix login"));
        // 104k written for an hour at $8, against $0.20 to read them.
        let usd = 104_000.0 * (8.0 - 0.2) / 1e6;
        assert!((b.usd.unwrap() - usd).abs() < 1e-9);
        assert!((c.rebuild_usd - usd).abs() < 1e-9);
        assert_eq!(c.by_cause[0].cause, Some(Cause::Break));
        let hit = c.hit_rate.unwrap();
        assert!((hit - 204_000.0 / 415_000.0).abs() < 1e-9, "{hit}");
    }

    #[test]
    fn a_short_break_inside_an_hour_long_cache_is_not_a_break() {
        // 20 minutes away with an hour-long cache: still warm, no rebuild.
        let c = run(
            &[
                (0, OPUS, 1_000, 0, 100_000, 100_000),
                (1, OPUS, 1_000, 100_000, 2_000, 2_000),
                (21, OPUS, 1_000, 102_000, 2_000, 2_000),
            ],
            None,
        );
        assert_eq!(c.rebuilds, 0);
        // The same 20 minutes with a 5-minute cache does lose it.
        let c = run(
            &[
                (0, OPUS, 1_000, 0, 100_000, 0),
                (1, OPUS, 1_000, 100_000, 2_000, 0),
                (21, OPUS, 1_000, 0, 104_000, 0),
            ],
            None,
        );
        assert_eq!((c.rebuilds, c.latest[0].cause), (1, Cause::Break));
    }

    #[test]
    fn a_model_switch_is_named_with_both_models() {
        let c = run(
            &[
                (0, OPUS, 1_000, 0, 100_000, 100_000),
                (1, OPUS, 1_000, 100_000, 2_000, 2_000),
                (2, "claude-sonnet-5-5", 1_000, 0, 103_000, 103_000),
            ],
            None,
        );
        let b = &c.latest[0];
        assert_eq!(b.cause, Cause::ModelSwitch);
        assert_eq!(b.from_model.as_deref(), Some(OPUS));
        assert_eq!(b.model.as_deref(), Some("claude-sonnet-5-5"));
    }

    #[test]
    fn compaction_and_ordinary_growth_are_not_rebuilds() {
        // A compaction rewrites the prompt on purpose.
        let c = run(
            &[
                (0, OPUS, 1_000, 0, 100_000, 100_000),
                (1, OPUS, 1_000, 100_000, 2_000, 2_000),
                (2, OPUS, 1_000, 0, 30_000, 30_000),
            ],
            Some(2),
        );
        assert_eq!((c.rebuilds, c.compactions), (0, 1));
        // A big tool result adds 40k to a 300k prompt: growth, not a rebuild.
        let c = run(
            &[
                (0, OPUS, 1_000, 0, 300_000, 300_000),
                (1, OPUS, 1_000, 300_000, 40_000, 40_000),
            ],
            None,
        );
        assert_eq!(c.rebuilds, 0);
    }

    #[test]
    fn an_effort_change_between_turns_is_named() {
        let idx = index(
            "s",
            &[
                (0, OPUS, 1_000, 0, 100_000, 100_000),
                (1, OPUS, 1_000, 100_000, 2_000, 2_000),
                (2, OPUS, 1_000, 20_000, 84_000, 84_000),
                // The same effort on the next turn: growth only.
                (3, OPUS, 1_000, 104_000, 1_000, 1_000),
            ],
            None,
        );
        idx.conn()
            .execute_batch(
                "INSERT INTO turns (session_id, n, seq, end_seq, effort) VALUES
                    ('s', 0, 0, 1, 'high'), ('s', 1, 2, 2, 'low'), ('s', 2, 3, 3, 'low');",
            )
            .unwrap();
        let c = cache(idx.conn(), idx.prices(), &Filter::default()).unwrap();
        assert_eq!(c.rebuilds, 1);
        let b = &c.latest[0];
        assert_eq!(b.cause, Cause::EffortChange);
        assert_eq!(
            (b.from_effort.as_deref(), b.effort.as_deref()),
            (Some("high"), Some("low"))
        );
    }

    #[test]
    fn a_copied_log_counts_each_rebuild_once() {
        let idx = index(
            "s",
            &[
                (0, OPUS, 1_000, 0, 100_000, 100_000),
                (1, OPUS, 1_000, 100_000, 2_000, 2_000),
                (91, OPUS, 1_000, 0, 104_000, 104_000),
            ],
            None,
        );
        // A resumed session's log starts with a copy of the first one.
        idx.conn()
            .execute_batch(
                "INSERT INTO sessions (id, agent, source_path) VALUES ('t', 'claude-code', '/Users/dev/y.jsonl');
                 INSERT INTO events (session_id, seq, ts, kind, title, body, source_offset, model,
                    tok_in, tok_out, tok_cache_read, tok_cache_write, tok_cache_write_1h)
                 SELECT 't', seq, ts, kind, title, body, source_offset, model,
                    tok_in, tok_out, tok_cache_read, tok_cache_write, tok_cache_write_1h
                 FROM events WHERE session_id = 's';",
            )
            .unwrap();
        let c = cache(idx.conn(), idx.prices(), &Filter::default()).unwrap();
        assert_eq!(c.rebuilds, 1);
    }

    #[test]
    fn a_rebuild_nothing_explains_is_left_unnamed() {
        let idx = index(
            "s",
            &[
                (0, OPUS, 1_000, 0, 100_000, 100_000),
                (1, OPUS, 1_000, 100_000, 2_000, 2_000),
                (2, OPUS, 1_000, 0, 104_000, 104_000),
            ],
            None,
        );
        let c = session_cache(idx.conn(), idx.prices(), "s").unwrap();
        assert_eq!((c.rebuilds, c.latest[0].cause), (1, Cause::Unknown));
    }
}
