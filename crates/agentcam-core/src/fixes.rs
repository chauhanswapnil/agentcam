//! Fixes: the few changes most likely to give time or money back.
//!
//! Each one is found in the logs, sized, and comes with something to copy
//! (a setting, or a prompt to hand the agent). Nothing here edits a file,
//! and nothing is shown below a floor: a fix that fires on ordinary work is
//! worse than none.
//!
//! Wording describes the repo and the setup, never the agent or the person.

use std::collections::HashMap;

use rusqlite::{Connection, params_from_iter};
use serde::Serialize;

use crate::approvals::{command_prefix, rule_for};
use crate::cache::Cause;
use crate::index::Result;
use crate::prices::Prices;
use crate::query::{Filter, conditions};
use crate::util::parse_ts_ms;

const WEEK_MS: i64 = 7 * 86_400_000;
/// A time fix must be worth this much a week to be shown.
const FLOOR_MS_PER_WEEK: i64 = 10 * 60_000;
/// Or interrupt this often a week (approvals).
const FLOOR_ASKS_PER_WEEK: f64 = 10.0;
/// A cost fix must be this share of the estimated cost.
const FLOOR_COST_SHARE: f64 = 0.05;
/// A loud command must add this many tokens a week, and a typical run
/// this many, to be shown. Tokens are estimated at four bytes each.
const FLOOR_TOKENS_PER_WEEK: i64 = 200_000;
const LOUD_RUN_TOKENS: i64 = 5_000;
const BYTES_PER_TOKEN: i64 = 4;
/// Quieter forms of common commands, offered as an example in the prompt.
const QUIET: &[(&str, &str)] = &[
    ("cargo test", "cargo test -q"),
    ("cargo build", "cargo build -q"),
    ("cargo check", "cargo check -q"),
    ("cargo clippy", "cargo clippy -q"),
    ("pytest", "pytest -q"),
    ("npm test", "npm test --silent"),
    ("npm install", "npm install --silent"),
    ("pnpm test", "pnpm test --silent"),
    ("pnpm install", "pnpm install --silent"),
    ("go test", "go test (without -v)"),
    ("xcodebuild", "xcodebuild -quiet"),
];
/// A command is "slow" only if a typical run takes this long.
const SLOW_RUN_MS: i64 = 20_000;
const MAX_FIXES: usize = 5;
/// Fixes need about a week of sessions behind them.
const ENOUGH_MS: i64 = 5 * 86_400_000;
/// Commands that wait on something else: a narrower form wouldn't help.
const WAITERS: &[&str] = &[
    "sleep", "wait", "until", "while", "timeout", "watch", "gh run", "tail",
];
const MAX_WEEKS: usize = 12;

/// A step that shows the thing happening.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Example {
    pub session_id: String,
    pub seq: i64,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Fix {
    /// Stable across runs: the kind and what it is about.
    pub id: String,
    /// `approval`, `slow_command`, `loud_command` or `cache`.
    pub kind: &'static str,
    pub title: String,
    /// One or two plain sentences: what the logs show.
    pub detail: String,
    /// How often it happened in the range.
    pub count: i64,
    /// Time it took over the whole range, for time fixes.
    pub total_ms: Option<i64>,
    /// Estimated dollars over the whole range, for cost fixes.
    pub total_usd: Option<f64>,
    /// Estimated tokens printed over the whole range, for loud commands.
    pub total_tokens: Option<i64>,
    pub tokens_per_week: Option<i64>,
    /// Time it takes a week, for time fixes.
    pub ms_per_week: Option<i64>,
    /// Estimated dollars a week at list prices, for cost fixes.
    pub usd_per_week: Option<f64>,
    /// Its share of agent time or of estimated cost, 0 to 1. Fixes are
    /// ranked by this.
    pub share: f64,
    /// Text to copy into a file, when there is one.
    pub fix: Option<String>,
    /// Where that text goes.
    pub fix_where: Option<&'static str>,
    /// A request to paste into the agent, when the agent can make the change.
    pub prompt: Option<String>,
    /// What to do, when the fix is a habit and not a file.
    pub advice: Option<String>,
    pub examples: Vec<Example>,
    /// How often it happened each week, oldest first.
    pub weekly: Vec<i64>,
    /// When the last week of `weekly` ends, in milliseconds since the epoch:
    /// week `i` of `n` covers the seven days before this, `n - 1 - i` weeks back.
    pub weekly_end: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Fixes {
    /// Biggest share first, a handful at most.
    pub fixes: Vec<Fix>,
    /// Weeks the range covers, at least one.
    pub weeks: f64,
    /// False while there is under a week of sessions to go on.
    pub enough_data: bool,
    /// Agent time and sessions in the range, to turn totals into rates.
    pub agent_ms: i64,
    pub sessions: i64,
}

/// "1h 5m", "12 min", "40 s".
fn span(ms: i64) -> String {
    let m = (ms as f64 / 60_000.0).round() as i64;
    if ms < 60_000 {
        format!("{} s", (ms as f64 / 1000.0).round() as i64)
    } else if m < 60 {
        format!("{m} min")
    } else if m % 60 == 0 {
        format!("{}h", m / 60)
    } else {
        format!("{}h {}m", m / 60, m % 60)
    }
}

fn times(n: i64) -> String {
    if n == 1 {
        "once".into()
    } else {
        format!("{n} times")
    }
}

fn median(v: &mut [i64]) -> i64 {
    v.sort_unstable();
    v.get(v.len() / 2).copied().unwrap_or(0)
}

/// The command a step ran, from its stored input (Claude Code keeps an
/// object, Codex the argv or the string).
fn command_of(input: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(input).ok()?;
    match v.get("command").unwrap_or(&v) {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(a) => a.last()?.as_str().map(String::from),
        _ => None,
    }
}

/// What a command line mainly runs, past the setup in front of it:
/// `export PATH=…; cd web && CI=1 npm test -- x` → `npm test`.
fn command_key(cmd: &str) -> Option<String> {
    for words in crate::shell::command_words(cmd) {
        let rest: Vec<&str> = words
            .iter()
            .map(String::as_str)
            .skip_while(|w| !w.starts_with('-') && w.contains('='))
            .collect();
        match rest.first() {
            None | Some(&("cd" | "export" | "source" | "." | "set" | "unset")) => continue,
            Some(_) => {
                return command_prefix(&rest.join(" ")).filter(|key| {
                    !WAITERS
                        .iter()
                        .any(|w| key == w || key.starts_with(&format!("{w} ")))
                });
            }
        }
    }
    None
}

struct Row {
    session: String,
    seq: i64,
    ts_text: Option<String>,
    ts: Option<i64>,
    tool: Option<String>,
    category: Option<String>,
    input: Option<String>,
    duration: Option<i64>,
    approval: Option<String>,
    title: Option<String>,
    out_chars: Option<i64>,
    step_title: String,
}

/// One time a fix's pattern happened.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Occurrence {
    pub example: Example,
    /// The step's own title: the command or the call.
    pub step_title: String,
    pub ts: Option<i64>,
    pub ts_text: Option<String>,
    /// Milliseconds for time fixes, tokens for loud commands.
    pub value: i64,
}

/// Every occurrence of one fix, biggest first.
pub(crate) struct Occurrences {
    pub title: String,
    /// `ms` or `tokens`.
    pub unit: &'static str,
    /// How many times it happened.
    pub total: i64,
    pub items: Vec<Occurrence>,
}

#[derive(Default)]
struct Group {
    /// Each run, oldest first. Kept only when a caller asks for the
    /// occurrences of one fix.
    all: Vec<(Occurrence, Option<i64>)>,
    count: i64,
    ms: i64,
    runs: Vec<i64>,
    /// Ran again with no file change since the last run.
    reruns: i64,
    /// Estimated tokens each run printed.
    outs: Vec<i64>,
    examples: Vec<Example>,
    stamps: Vec<i64>,
}

impl Group {
    fn add(&mut self, r: &Row, ms: i64, tokens: Option<i64>, keep: bool) {
        self.count += 1;
        self.ms += ms;
        self.runs.push(ms);
        if keep {
            self.all.push((
                Occurrence {
                    example: Example {
                        session_id: r.session.clone(),
                        seq: r.seq,
                        title: r.title.clone(),
                    },
                    step_title: r.step_title.clone(),
                    ts: r.ts,
                    ts_text: r.ts_text.clone(),
                    value: ms,
                },
                tokens,
            ));
        }
        if self.examples.len() < 3 {
            self.examples.push(Example {
                session_id: r.session.clone(),
                seq: r.seq,
                title: r.title.clone(),
            });
        }
        if let Some(t) = r.ts {
            self.stamps.push(t);
        }
    }
}

/// Counts per week, oldest first, over the latest weeks the range covers
/// (the week ending at `last` comes last).
fn weekly(stamps: &[i64], last: i64, weeks: usize) -> Vec<i64> {
    let mut out = vec![0; weeks.clamp(1, MAX_WEEKS)];
    let n = out.len() as i64;
    for t in stamps {
        let back = (last - t) / WEEK_MS;
        if (0..n).contains(&back) {
            out[(n - 1 - back) as usize] += 1;
        }
    }
    out
}

/// The fixes worth showing for what a filter covers.
pub fn fixes(conn: &Connection, prices: &Prices, f: &Filter) -> Result<Fixes> {
    let mut out = find(conn, prices, f, true)?;
    out.fixes.truncate(MAX_FIXES);
    Ok(out)
}

/// Every pattern a fix could come from, biggest share first. With `floors`
/// off nothing is left out for being small: the ledger uses that to measure
/// a fix before and after, however little is left of it.
pub(crate) fn find(conn: &Connection, prices: &Prices, f: &Filter, floors: bool) -> Result<Fixes> {
    Ok(find_inner(conn, prices, f, floors, None)?.0)
}

/// Every occurrence of the fix with this id, with no floors applied. `None`
/// when the id isn't a fix with occurrences of its own (cache rebuilds are
/// listed by `cache`).
pub(crate) fn occurrences(
    conn: &Connection,
    prices: &Prices,
    f: &Filter,
    id: &str,
) -> Result<Option<Occurrences>> {
    let found = find_inner(conn, prices, f, false, Some(id))?.1;
    if found.is_some() {
        return Ok(found);
    }
    // A fix with no run in this stretch of time has no occurrences, not an
    // unknown id.
    let empty = |title: String, unit: &'static str| Occurrences {
        title,
        unit,
        total: 0,
        items: Vec::new(),
    };
    Ok(match id.split_once(':') {
        Some(("slow", key)) => Some(empty(format!("`{key}` runs"), "ms")),
        Some(("approval", rule)) => Some(empty(format!("`{rule}` allowed by hand"), "ms")),
        Some(("loud", key)) => Some(empty(format!("`{key}` output"), "tokens")),
        _ => None,
    })
}

fn find_inner(
    conn: &Connection,
    prices: &Prices,
    f: &Filter,
    floors: bool,
    want: Option<&str>,
) -> Result<(Fixes, Option<Occurrences>)> {
    let mut wanted: Option<Occurrences> = None;
    let (cond, args) = conditions(f);
    let mut stmt = conn.prepare(&format!(
        "SELECT e.session_id, e.seq, e.ts, e.tool_name, e.category,
                CASE WHEN e.category = 'command' OR e.approval IN ('you', 'rejected') THEN e.input END,
                e.duration_ms, e.approval, s.title, e.out_chars, e.title
         FROM events e JOIN sessions s ON s.id = e.session_id
         WHERE e.kind = 'tool_call' AND {cond} ORDER BY e.session_id, e.seq"
    ))?;
    let rows: Vec<Row> = stmt
        .query_map(params_from_iter(&args), |r| {
            let ts_text: Option<String> = r.get(2)?;
            Ok(Row {
                session: r.get(0)?,
                seq: r.get(1)?,
                ts: ts_text.as_deref().and_then(parse_ts_ms),
                ts_text,
                tool: r.get(3)?,
                category: r.get(4)?,
                input: r.get(5)?,
                duration: r.get(6)?,
                approval: r.get(7)?,
                title: r.get(8)?,
                out_chars: r.get(9)?,
                step_title: r.get(10)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    let stamps = || rows.iter().filter_map(|r| r.ts);
    let (first, last) = (stamps().min().unwrap_or(0), stamps().max().unwrap_or(0));
    let weeks = ((last - first) as f64 / WEEK_MS as f64).max(1.0);
    let whole_weeks = weeks.ceil() as usize;
    let mut out = Fixes {
        weeks,
        enough_data: last - first >= ENOUGH_MS,
        ..Default::default()
    };
    let bar = crate::profile::time_bar(conn, f)?;
    out.agent_ms = bar.total_ms;
    out.sessions = bar.sessions;
    let agent_ms = bar.total_ms.max(1);

    // How long each tool usually runs when nobody has to allow it.
    let by_hand = |r: &Row| r.approval.as_deref() == Some("you");
    let mut usual: HashMap<&str, Vec<i64>> = HashMap::new();
    for r in &rows {
        if let (Some(t), Some(d), false) = (r.tool.as_deref(), r.duration, by_hand(r)) {
            usual.entry(t).or_default().push(d);
        }
    }
    let usual: HashMap<&str, i64> = usual
        .into_iter()
        .map(|(t, mut d)| (t, median(&mut d)))
        .collect();

    // Calls allowed by hand, by the allow rule that would cover them; and
    // commands by what they run.
    let mut asks: HashMap<String, Group> = HashMap::new();
    let mut commands: HashMap<String, Group> = HashMap::new();
    // Per session: the commands run since the last file change.
    let mut session = "";
    let mut since_change: Vec<String> = Vec::new();
    for r in &rows {
        if r.session != session {
            session = &r.session;
            since_change.clear();
        }
        let category = r.category.as_deref();
        if matches!(category, Some("edit" | "write")) {
            since_change.clear();
        }
        if by_hand(r)
            && let Some(rule) = r
                .tool
                .as_deref()
                .and_then(|t| rule_for(t, r.input.as_deref()))
        {
            let run = r
                .tool
                .as_deref()
                .and_then(|t| usual.get(t))
                .copied()
                .unwrap_or(0);
            let wait = (r.duration.unwrap_or(0) - run).max(0);
            asks.entry(rule)
                .or_default()
                .add(r, wait, None, want.is_some());
        }
        if category == Some("command")
            && !by_hand(r)
            && r.approval.as_deref() != Some("rejected")
            && let Some(key) = r
                .input
                .as_deref()
                .and_then(command_of)
                .as_deref()
                .and_then(command_key)
            && let Some(d) = r.duration
        {
            let g = commands.entry(key.clone()).or_default();
            g.add(
                r,
                d,
                r.out_chars.map(|n| n / BYTES_PER_TOKEN),
                want.is_some(),
            );
            if let Some(n) = r.out_chars {
                g.outs.push(n / BYTES_PER_TOKEN);
            }
            if since_change.contains(&key) {
                g.reruns += 1;
            } else {
                since_change.push(key);
            }
        }
    }

    for (rule, mut g) in asks {
        let per_week = (g.ms as f64 / weeks) as i64;
        if floors && per_week < FLOOR_MS_PER_WEEK && (g.count as f64 / weeks) < FLOOR_ASKS_PER_WEEK
        {
            continue;
        }
        let typical = median(&mut g.runs);
        let settings = serde_json::json!({ "permissions": { "allow": [rule] } });
        let title = format!("You allowed `{rule}` by hand {}", times(g.count));
        if want == Some(format!("approval:{rule}").as_str()) {
            wanted = Some(Occurrences::new(&title, "ms", g.count, &g.all, |o, _| {
                o.value
            }));
        }
        out.fixes.push(Fix {
            id: format!("approval:{rule}"),
            kind: "approval",
            title,
            detail: format!(
                "Each one held the agent until you answered, about {} at a time and {} in all (an estimate). You allowed every one.",
                span(typical),
                span(g.ms)
            ),
            count: g.count,
            total_ms: Some(g.ms),
            ms_per_week: Some(per_week),
            share: g.ms as f64 / agent_ms as f64,
            fix: Some(serde_json::to_string_pretty(&settings).unwrap_or_default()),
            fix_where: Some("Claude Code settings, under permissions.allow"),
            prompt: Some(format!(
                "Add `{rule}` to permissions.allow in this project's .claude/settings.json, keeping what is already there."
            )),
            weekly: weekly(&g.stamps, last, whole_weeks),
            examples: g.examples,
            ..Default::default()
        });
    }

    // Tokens that entered a prompt fresh in the range: what a loud
    // command's output is a share of.
    let costs = crate::cost::costs(conn, prices, f)?;
    let new_tokens: i64 = costs
        .models
        .iter()
        .map(|m| m.input_tokens + m.cache_write_tokens)
        .sum::<i64>()
        .max(1);

    for (key, mut g) in commands {
        // Loud: what it prints is read by the model, run after run.
        let tokens: i64 = g.outs.iter().sum();
        let typical_out = median(&mut g.outs);
        let tokens_week = (tokens as f64 / weeks) as i64;
        if tokens > 0
            && (!floors || (tokens_week >= FLOOR_TOKENS_PER_WEEK && typical_out >= LOUD_RUN_TOKENS))
        {
            let k = |n: i64| {
                if n >= 1_000_000 {
                    format!("{:.1}M", n as f64 / 1e6)
                } else {
                    format!("{}K", (n as f64 / 1000.0).round() as i64)
                }
            };
            let hint = QUIET
                .iter()
                .find(|(c, _)| *c == key)
                .map(|(_, q)| format!(" For this one, `{q}` is the usual quiet form."))
                .unwrap_or_default();
            let title = format!(
                "`{key}` printed about {} tokens over {} runs",
                k(tokens),
                g.count
            );
            if want == Some(format!("loud:{key}").as_str()) {
                let n = g.all.iter().filter(|(_, t)| t.is_some()).count() as i64;
                wanted = Some(Occurrences::new(&title, "tokens", n, &g.all, |_, t| {
                    t.unwrap_or(0)
                }));
            }
            out.fixes.push(Fix {
                id: format!("loud:{key}"),
                kind: "loud_command",
                title,
                detail: format!(
                    "A typical run prints about {} tokens, and the model reads all of it and carries it for the rest of the session (an estimate, at four bytes a token). A quiet flag, or only the last lines, keeps what matters.",
                    k(typical_out)
                ),
                count: g.count,
                total_tokens: Some(tokens),
                tokens_per_week: Some(tokens_week),
                share: tokens as f64 / new_tokens as f64,
                prompt: Some(format!(
                    "In this repo `{key}` prints about {} tokens on a typical run, which fills the context. Find a quieter form for everyday runs (a quiet flag, a short reporter, or piping through `tail`) that still shows errors in full, and add it to AGENTS.md under the commands section.{hint}",
                    k(typical_out)
                )),
                fix_where: Some("AGENTS.md"),
                weekly: weekly(&g.stamps, last, whole_weeks),
                examples: g.examples.clone(),
                ..Default::default()
            });
        }

        let per_week = (g.ms as f64 / weeks) as i64;
        let typical = median(&mut g.runs);
        if floors && (per_week < FLOOR_MS_PER_WEEK || typical < SLOW_RUN_MS) {
            continue;
        }
        let again = if g.reruns > 0 {
            format!(
                " It ran again with no file changed since the last run {}.",
                times(g.reruns)
            )
        } else {
            String::new()
        };
        let title = format!("`{key}` ran {} and took {}", times(g.count), span(g.ms));
        if want == Some(format!("slow:{key}").as_str()) {
            wanted = Some(Occurrences::new(&title, "ms", g.count, &g.all, |o, _| {
                o.value
            }));
        }
        out.fixes.push(Fix {
            id: format!("slow:{key}"),
            kind: "slow_command",
            title,
            detail: format!("A typical run takes {}.{again} A narrower or quieter form for everyday edits would give most of that back.", span(typical)),
            count: g.count,
            total_ms: Some(g.ms),
            ms_per_week: Some(per_week),
            share: g.ms as f64 / agent_ms as f64,
            prompt: Some(format!(
                "In this repo `{key}` ran {} recently and a typical run takes {}. Find a faster check for everyday edits (a narrower target, only the tests near the change, or a quiet flag) and add it to AGENTS.md under the commands section. Keep the full run as the check before finishing.",
                times(g.count),
                span(typical)
            )),
            fix_where: Some("AGENTS.md"),
            weekly: weekly(&g.stamps, last, whole_weeks),
            examples: g.examples,
            ..Default::default()
        });
    }

    // Cache rebuilds, by cause.
    let cache = crate::cache::cache_in_full(conn, prices, f)?;
    let cost = costs.usd;
    for c in &cache.by_cause {
        let Some(cause) = c.cause else { continue };
        if cost <= 0.0 || (floors && c.usd / cost < FLOOR_COST_SHARE) {
            continue;
        }
        let tokens = format!("{:.1}M", c.tokens as f64 / 1e6);
        let (what, title, advice) = match cause {
            Cause::Break => (
                "break",
                format!(
                    "The prompt cache was written again after a break {}",
                    times(c.count)
                ),
                "At the end of a task, start a fresh session rather than returning to a big one. Compacting before a long break also makes the return cheaper.",
            ),
            Cause::ModelSwitch => (
                "model_switch",
                format!(
                    "A model switch wrote the prompt cache again {}",
                    times(c.count)
                ),
                "Each model keeps its own cache. Pick the model at the start of a session, or hand the other model's work to a subagent.",
            ),
            Cause::EffortChange => (
                "effort_change",
                format!(
                    "An effort change wrote the prompt cache again {}",
                    times(c.count)
                ),
                "Effort is part of what the cache is keyed on. Set it once per session where you can.",
            ),
            Cause::Unknown => continue,
        };
        let seen: Vec<_> = cache.latest.iter().filter(|b| b.cause == cause).collect();
        out.fixes.push(Fix {
            id: format!("cache:{what}"),
            kind: "cache",
            title,
            detail: format!(
                "{tokens} tokens were written to the cache a second time, about ${:.2} at API list prices. This is how the cache works, not a fault; the cost is yours to weigh.",
                c.usd
            ),
            count: c.count,
            total_usd: Some(c.usd),
            usd_per_week: Some(c.usd / weeks),
            share: c.usd / cost,
            advice: Some(advice.into()),
            examples: seen
                .iter()
                .take(3)
                .map(|b| Example {
                    session_id: b.session_id.clone(),
                    seq: b.seq,
                    title: b.title.clone(),
                })
                .collect(),
            weekly: weekly(
                &seen.iter().filter_map(|b| b.ts.as_deref().and_then(parse_ts_ms)).collect::<Vec<_>>(),
                last,
                whole_weeks,
            ),
            ..Default::default()
        });
    }

    for x in &mut out.fixes {
        x.weekly_end = Some(last);
    }
    out.fixes.sort_by(|a, b| {
        b.share
            .partial_cmp(&a.share)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok((out, wanted))
}

impl Occurrences {
    /// The kept runs, biggest first. `value` picks the number each is
    /// ranked by; runs that print nothing (value 0) are left out.
    fn new(
        title: &str,
        unit: &'static str,
        total: i64,
        all: &[(Occurrence, Option<i64>)],
        value: impl Fn(&Occurrence, Option<i64>) -> i64,
    ) -> Occurrences {
        let mut items: Vec<Occurrence> = all
            .iter()
            .map(|(o, t)| Occurrence {
                value: value(o, *t),
                ..o.clone()
            })
            .filter(|o| o.value > 0)
            .collect();
        items.sort_by(|a, b| {
            b.value.cmp(&a.value).then_with(|| {
                (&a.example.session_id, a.example.seq).cmp(&(&b.example.session_id, b.example.seq))
            })
        });
        Occurrences {
            title: title.to_string(),
            unit,
            total,
            items,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Index;

    struct Step {
        min: i64,
        tool: &'static str,
        category: &'static str,
        command: &'static str,
        secs: i64,
        approval: Option<&'static str>,
        /// Bytes of output.
        out: i64,
    }

    fn cmd(min: i64, command: &'static str, secs: i64) -> Step {
        Step {
            min,
            tool: "Bash",
            category: "command",
            command,
            secs,
            approval: Some("rule"),
            out: 400,
        }
    }

    fn edit(min: i64) -> Step {
        Step {
            min,
            tool: "Edit",
            category: "edit",
            command: "",
            secs: 1,
            approval: None,
            out: 2,
        }
    }

    fn index(steps: &[Step]) -> Index {
        let idx = Index::open_in_memory().unwrap();
        idx.conn()
            .execute_batch(
                "INSERT INTO sessions (id, agent, source_path, title) VALUES ('s', 'claude-code', '/Users/dev/x.jsonl', 'Fix login');",
            )
            .unwrap();
        for (seq, s) in steps.iter().enumerate() {
            // Spread over a little more than a week.
            let day = 1 + s.min / 1440;
            let ts = format!(
                "2026-11-{day:02}T{:02}:{:02}:00.000Z",
                (s.min % 1440) / 60,
                s.min % 60
            );
            let input = serde_json::json!({ "command": s.command }).to_string();
            idx.conn()
                .execute(
                    "INSERT INTO events (session_id, seq, ts, kind, category, tool_name, title, body,
                        source_offset, gap_ms, duration_ms, approval, input, out_chars)
                     VALUES ('s', ?1, ?2, 'tool_call', ?3, ?4, '', '', 0, 60000, ?5, ?6, ?7, ?8)",
                    rusqlite::params![seq as i64, ts, s.category, s.tool, s.secs * 1000, s.approval, input, s.out],
                )
                .unwrap();
        }
        idx
    }

    fn run(steps: &[Step]) -> Fixes {
        let idx = index(steps);
        fixes(idx.conn(), idx.prices(), &Filter::default()).unwrap()
    }

    const DAY: i64 = 1440;

    #[test]
    fn a_slow_command_run_often_is_a_fix() {
        let mut steps = vec![edit(0)];
        // Ten 90-second test runs over eight days, three of them with no
        // file change since the run before.
        for i in 0..7 {
            steps.push(cmd(i * DAY + 10, "cargo test --all", 90));
            steps.push(edit(i * DAY + 20));
        }
        for i in 0..3 {
            steps.push(cmd(7 * DAY + 30 + i * 5, "cd web && cargo test", 90));
        }
        let f = run(&steps);
        assert!(f.enough_data);
        assert_eq!(f.fixes.len(), 1, "{:#?}", f.fixes);
        let x = &f.fixes[0];
        assert_eq!(
            (x.id.as_str(), x.kind, x.count),
            ("slow:cargo test", "slow_command", 10)
        );
        assert_eq!(x.title, "`cargo test` ran 10 times and took 15 min");
        assert!(
            x.detail
                .contains("no file changed since the last run 2 times"),
            "{}",
            x.detail
        );
        assert!(x.prompt.as_deref().unwrap().contains("AGENTS.md"));
        assert_eq!(x.fix, None, "agentcam doesn't invent commands");
        assert_eq!(x.examples.len(), 3);
        // Weeks end at the latest run: one run falls in the week before.
        assert_eq!(x.weekly, vec![1, 9]);
        assert!(x.weekly_end.is_some());
    }

    #[test]
    fn every_run_of_a_fix_can_be_listed() {
        let mut steps = vec![edit(0)];
        for i in 0..7 {
            steps.push(cmd(i * DAY + 10, "cargo test --all", 90));
            steps.push(edit(i * DAY + 20));
        }
        for i in 0..3 {
            steps.push(cmd(7 * DAY + 30 + i * 5, "cd web && cargo test", 90));
        }
        let idx = index(&steps);
        let b = crate::behind::behind(
            idx.conn(),
            idx.prices(),
            &Filter::default(),
            None,
            "fix:slow:cargo test",
        )
        .unwrap()
        .unwrap();
        // All ten runs, not the three examples a card keeps.
        assert_eq!((b.items.len(), b.total, b.unit), (10, 10, "ms"));
        assert!(b.items.iter().all(|x| x.value == 90_000.0));
        // A near miss: a command that never ran has an empty list, and an
        // id that isn't a kind of fix is unknown.
        let none = crate::behind::behind(
            idx.conn(),
            idx.prices(),
            &Filter::default(),
            None,
            "fix:slow:make",
        )
        .unwrap()
        .unwrap();
        assert_eq!((none.items.len(), none.total), (0, 0));
        assert!(
            crate::behind::behind(
                idx.conn(),
                idx.prices(),
                &Filter::default(),
                None,
                "fix:whatever:x"
            )
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn setup_in_front_of_a_command_is_looked_past() {
        assert_eq!(
            command_key("export PATH=/opt/bin:$PATH; cd web && CI=1 npm test -- login").as_deref(),
            Some("npm test")
        );
        assert_eq!(
            command_key("DEVELOPER_DIR=/Applications/Xcode.app xcodebuild -scheme App build")
                .as_deref(),
            Some("xcodebuild")
        );
        assert_eq!(
            command_key("cargo test -p core").as_deref(),
            Some("cargo test")
        );
        // Waiting on CI or a timer isn't a slow command.
        assert_eq!(command_key("gh run watch 123"), None);
        assert_eq!(command_key("sleep 30"), None);
        // Nothing but setup: no key.
        assert_eq!(command_key("export FOO=1"), None);
        assert_eq!(command_key("cd web"), None);
    }

    #[test]
    fn many_quick_commands_and_a_rare_slow_one_are_not() {
        let mut steps = vec![];
        // Quick ones, however many: not slow.
        for i in 0..200 {
            steps.push(cmd(i * 50, "git status", 4));
        }
        // One long build in eight days: under the weekly floor.
        steps.push(cmd(3 * DAY, "npm run build", 300));
        steps.push(edit(8 * DAY));
        assert_eq!(run(&steps).fixes, vec![]);
    }

    #[test]
    fn calls_allowed_by_hand_again_and_again_suggest_a_rule() {
        let mut steps = vec![cmd(0, "npm test", 5), edit(8 * DAY)];
        // Twelve a week, each held about a minute.
        for i in 0..12 {
            steps.push(Step {
                approval: Some("you"),
                ..cmd(DAY + i * 30, "npm test", 65)
            });
        }
        let f = run(&steps);
        let x = f
            .fixes
            .iter()
            .find(|x| x.kind == "approval")
            .expect("an approval fix");
        assert_eq!(x.id, "approval:Bash(npm test:*)");
        assert_eq!(x.count, 12);
        assert!(x.fix.as_deref().unwrap().contains("\"Bash(npm test:*)\""));
        // 60 s over the usual 5 s run, twelve times.
        assert!(x.detail.contains("12 min in all"), "{}", x.detail);
        // A broad command is never suggested, however often it was allowed.
        let mut steps = vec![edit(8 * DAY)];
        for i in 0..40 {
            steps.push(Step {
                approval: Some("you"),
                ..cmd(i * 30, "rm -rf build", 65)
            });
        }
        assert!(run(&steps).fixes.iter().all(|x| x.kind != "approval"));
    }

    #[test]
    fn a_command_that_prints_a_lot_is_a_fix() {
        let mut steps = vec![edit(0)];
        // Twelve quick runs over eight days, 120 KB of output each: about
        // 30K tokens a run, 360K in all.
        for i in 0..12 {
            steps.push(Step {
                out: 120_000,
                ..cmd(i * 960 + 10, "cargo test --all", 5)
            });
            steps.push(edit(i * 960 + 20));
        }
        // A near miss: as many runs of something that prints little.
        for i in 0..12 {
            steps.push(cmd(i * 960 + 30, "cargo fmt --check", 5));
        }
        let f = run(&steps);
        assert_eq!(f.fixes.len(), 1, "{:#?}", f.fixes);
        let x = &f.fixes[0];
        assert_eq!((x.id.as_str(), x.kind), ("loud:cargo test", "loud_command"));
        assert_eq!(
            x.title,
            "`cargo test` printed about 360K tokens over 12 runs"
        );
        assert_eq!(x.total_tokens, Some(360_000));
        assert!(x.prompt.as_deref().unwrap().contains("`cargo test -q`"));
    }

    #[test]
    fn under_a_week_of_data_says_so() {
        let f = run(&[cmd(0, "cargo test", 90), cmd(60, "cargo test", 90)]);
        assert!(!f.enough_data);
    }
}
