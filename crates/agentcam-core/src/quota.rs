//! An estimate of Claude Code's usage windows, from the token counts in its
//! logs. Claude Code doesn't log how full its windows are, so this groups
//! model calls into 5-hour blocks the way ccusage does: a block starts at
//! the hour of the first call after the previous block ended, and ends five
//! hours later (or after five hours without a call).
//!
//! It is an estimate: it only sees calls logged on this machine, and the
//! real limits are not published. The highest past block is shown as a
//! reference instead of a limit.

use rusqlite::Connection;
use serde::Serialize;

use crate::index::Result;
use crate::util::{format_ts_ms, parse_ts_ms};

pub const BLOCK_MS: i64 = 5 * 3_600_000;
const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 86_400_000;
/// How far back to look for blocks, and so for the highest one.
const LOOKBACK_DAYS: i64 = 90;
/// Blocks returned for the chart.
const CHART_DAYS: i64 = 7;

/// One model call's usage.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Call {
    pub ts_ms: i64,
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    /// API list price; `None` when the model has no price.
    pub cost_usd: Option<f64>,
}

impl Call {
    fn tokens(&self) -> i64 {
        self.input + self.output + self.cache_read + self.cache_write
    }
}

/// Tokens and value over a stretch of calls.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Totals {
    pub calls: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    /// All four token counts added up, as ccusage counts them.
    pub total_tokens: i64,
    /// What these calls would cost at API list prices.
    pub cost_usd: f64,
    /// Calls with no price, so `cost_usd` is low.
    pub unpriced_calls: i64,
}

impl Totals {
    fn add(&mut self, c: &Call) {
        self.calls += 1;
        self.input_tokens += c.input;
        self.output_tokens += c.output;
        self.cache_read_tokens += c.cache_read;
        self.cache_write_tokens += c.cache_write;
        self.total_tokens += c.tokens();
        match c.cost_usd {
            Some(v) => self.cost_usd += v,
            None => self.unpriced_calls += 1,
        }
    }
}

/// One 5-hour block.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Block {
    /// The hour of the block's first call.
    pub start: String,
    /// `start` plus five hours.
    pub end: String,
    pub first_at: String,
    pub last_at: String,
    /// Still open: before `end`, and a call in the last five hours.
    pub active: bool,
    #[serde(flatten)]
    pub totals: Totals,
}

/// The open block's pace and where it is heading.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Pace {
    /// Tokens per minute between the block's first and latest call.
    pub tokens_per_minute: f64,
    pub cost_per_hour: f64,
    /// At this pace, the block's tokens and value when it ends.
    pub projected_tokens: i64,
    pub projected_cost_usd: f64,
    pub remaining_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClaudeQuota {
    /// Always true: these numbers are estimated from the logs.
    pub estimate: bool,
    pub block_minutes: i64,
    /// The block open now, if any.
    pub active: Option<Block>,
    pub pace: Option<Pace>,
    /// The ended block with the most tokens in the last 90 days.
    pub peak: Option<Block>,
    /// Blocks that started in the last 7 days, oldest first.
    pub blocks: Vec<Block>,
    /// Every call in the last 7 days.
    pub week: Totals,
}

fn floor_hour(ms: i64) -> i64 {
    ms.div_euclid(HOUR_MS) * HOUR_MS
}

/// Group calls (sorted by time) into 5-hour blocks.
pub fn blocks(calls: &[Call], now_ms: i64) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    // (start, first, last, totals) of the block being filled.
    let mut cur: Option<(i64, i64, i64, Totals)> = None;
    let close = |(start, first, last, totals): (i64, i64, i64, Totals)| Block {
        start: format_ts_ms(start),
        end: format_ts_ms(start + BLOCK_MS),
        first_at: format_ts_ms(first),
        last_at: format_ts_ms(last),
        active: now_ms < start + BLOCK_MS && now_ms - last < BLOCK_MS,
        totals,
    };
    for c in calls {
        if let Some((start, _, last, _)) = cur
            && (c.ts_ms - start >= BLOCK_MS || c.ts_ms - last >= BLOCK_MS)
        {
            out.push(close(cur.take().expect("block is open")));
        }
        let b =
            cur.get_or_insert_with(|| (floor_hour(c.ts_ms), c.ts_ms, c.ts_ms, Totals::default()));
        b.2 = c.ts_ms;
        b.3.add(c);
    }
    if let Some(b) = cur {
        out.push(close(b));
    }
    out
}

/// How fast the open block is going, and where that pace ends up.
pub fn pace(b: &Block, now_ms: i64) -> Option<Pace> {
    let (first, last) = (parse_ts_ms(&b.first_at)?, parse_ts_ms(&b.last_at)?);
    let end = parse_ts_ms(&b.end)?;
    let minutes = (last - first) as f64 / 60_000.0;
    if minutes < 1.0 {
        return None;
    }
    let remaining_ms = (end - now_ms).max(0);
    let left = remaining_ms as f64 / 60_000.0;
    let tpm = b.totals.total_tokens as f64 / minutes;
    let cpm = b.totals.cost_usd / minutes;
    Some(Pace {
        tokens_per_minute: tpm,
        cost_per_hour: cpm * 60.0,
        projected_tokens: b.totals.total_tokens + (tpm * left).round() as i64,
        projected_cost_usd: b.totals.cost_usd + cpm * left,
        remaining_ms,
    })
}

/// Build the estimate from calls sorted by time.
pub fn estimate(calls: &[Call], now_ms: i64) -> ClaudeQuota {
    let all = blocks(calls, now_ms);
    let active = all.iter().find(|b| b.active).cloned();
    let pace = active.as_ref().and_then(|b| pace(b, now_ms));
    let peak = all
        .iter()
        .filter(|b| !b.active)
        .max_by_key(|b| b.totals.total_tokens)
        .cloned();
    let since = format_ts_ms(now_ms - CHART_DAYS * DAY_MS);
    let blocks = all.into_iter().filter(|b| b.start >= since).collect();
    let mut week = Totals::default();
    for c in calls
        .iter()
        .filter(|c| c.ts_ms >= now_ms - CHART_DAYS * DAY_MS)
    {
        week.add(c);
    }
    ClaudeQuota {
        estimate: true,
        block_minutes: BLOCK_MS / 60_000,
        active,
        pace,
        peak,
        blocks,
        week,
    }
}

/// Claude Code model calls (sessions and subagents) in the last 90 days.
pub fn claude_quota(conn: &Connection, now_ms: i64) -> Result<Option<ClaudeQuota>> {
    let since = format_ts_ms(now_ms - LOOKBACK_DAYS * DAY_MS);
    let mut stmt = conn.prepare(
        "SELECT e.ts, e.tok_in, e.tok_out, e.tok_cache_read, e.tok_cache_write, e.cost_usd
         FROM events e JOIN sessions s ON s.id = e.session_id
         WHERE s.agent = 'claude-code' AND e.ts >= ?1 AND e.ts <= ?2
           AND e.tok_in + e.tok_out + e.tok_cache_read + e.tok_cache_write > 0
         ORDER BY e.ts",
    )?;
    let calls: Vec<Call> = stmt
        .query_map([&since, &format_ts_ms(now_ms)], |r| {
            let ts: String = r.get(0)?;
            let c = Call {
                ts_ms: 0,
                input: r.get(1)?,
                output: r.get(2)?,
                cache_read: r.get(3)?,
                cache_write: r.get(4)?,
                cost_usd: r.get(5)?,
            };
            Ok(parse_ts_ms(&ts).map(|ts_ms| Call { ts_ms, ..c }))
        })?
        .filter_map(|c| c.transpose())
        .collect::<std::result::Result<_, _>>()?;
    if calls.is_empty() {
        return Ok(None);
    }
    Ok(Some(estimate(&calls, now_ms)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_762_400_000_000; // 2025-11-06T03:33:20Z

    fn call(min: i64, tokens: i64, cost: Option<f64>) -> Call {
        Call {
            ts_ms: T0 + min * 60_000,
            input: tokens / 2,
            output: tokens - tokens / 2,
            cost_usd: cost,
            ..Default::default()
        }
    }

    #[test]
    fn blocks_start_on_the_hour_and_last_five_hours() {
        // 03:33, 05:33, then 08:13 is past 03:00 + 5h, so a new block.
        let calls = [
            call(0, 100, Some(1.0)),
            call(120, 50, Some(0.5)),
            call(280, 10, None),
        ];
        let b = blocks(&calls, T0 + 300 * 60_000);
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].start, "2025-11-06T03:00:00.000Z");
        assert_eq!(b[0].end, "2025-11-06T08:00:00.000Z");
        assert_eq!(b[0].totals.total_tokens, 150);
        assert_eq!(b[0].totals.cost_usd, 1.5);
        assert!(!b[0].active);
        assert_eq!(b[1].start, "2025-11-06T08:00:00.000Z");
        assert_eq!(b[1].totals.unpriced_calls, 1);
        assert!(b[1].active);
    }

    #[test]
    fn a_five_hour_gap_starts_a_new_block() {
        // Both calls fall in a 5h span from the hour, but nothing ran for 5h.
        let calls = [
            Call {
                ts_ms: 0,
                input: 1,
                ..Default::default()
            },
            Call {
                ts_ms: BLOCK_MS,
                input: 1,
                ..Default::default()
            },
        ];
        assert_eq!(blocks(&calls, BLOCK_MS + 1).len(), 2);
        // A call 4h59m in stays in the first block.
        let near = [
            calls[0],
            Call {
                ts_ms: BLOCK_MS - 60_000,
                ..calls[1]
            },
        ];
        assert_eq!(blocks(&near, BLOCK_MS).len(), 1);
    }

    #[test]
    fn pace_projects_to_the_block_end() {
        // 600 tokens over 60 minutes = 10/min; block 03:00-08:00, now 04:33
        // leaves 207 minutes.
        let calls = [call(0, 300, Some(0.3)), call(60, 300, Some(0.3))];
        let now = T0 + 60 * 60_000;
        let q = estimate(&calls, now);
        let a = q.active.as_ref().expect("open block");
        assert_eq!(a.totals.total_tokens, 600);
        let p = q.pace.expect("pace");
        assert!((p.tokens_per_minute - 10.0).abs() < 1e-9);
        assert_eq!(p.remaining_ms, 207 * 60_000 - 20_000);
        assert!(
            (p.projected_tokens - (600 + 2067)).abs() <= 1,
            "{}",
            p.projected_tokens
        );
        assert!((p.cost_per_hour - 0.6).abs() < 1e-9);
        assert!(q.peak.is_none());
    }

    #[test]
    fn peak_is_the_largest_ended_block_and_week_is_rolling() {
        let day = 24 * 60;
        let calls = [
            call(-10 * day, 9_000, Some(9.0)), // too old for the week
            call(-3 * day, 5_000, Some(5.0)),
            call(-day, 2_000, Some(2.0)),
            call(0, 100_000, Some(50.0)), // open: never the peak
        ];
        let q = estimate(&calls, T0 + 10 * 60_000);
        assert_eq!(q.peak.unwrap().totals.total_tokens, 9_000);
        assert_eq!(q.week.total_tokens, 107_000);
        assert_eq!(q.week.calls, 3);
        // Only blocks from the last 7 days go to the chart.
        assert_eq!(q.blocks.len(), 3);
        assert!(q.estimate);
    }

    #[test]
    fn no_open_block_when_quiet_for_five_hours() {
        let calls = [call(0, 100, Some(1.0))];
        let q = estimate(&calls, T0 + 6 * 60 * 60_000);
        assert!(q.active.is_none() && q.pace.is_none());
        assert_eq!(q.peak.unwrap().totals.total_tokens, 100);
    }
}
