//! Where the estimated cost comes from: by kind of token and by model.
//!
//! Worked out from the token counts in the index with the current price
//! list, so it always agrees with the per-step costs the index stores.

use std::collections::BTreeMap;

use rusqlite::{Connection, params_from_iter};
use serde::Serialize;

use crate::index::{Result, usage_from_row};
use crate::model::Tier;
use crate::prices::{PRICED_ON, Parts, Prices};
use crate::query::{Filter, conditions};

/// One model's share.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct ModelCost {
    /// `None` when the log didn't name the model.
    pub model: Option<String>,
    pub calls: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    /// `None` when the price list has no entry for this model.
    pub usd: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Costs {
    /// Estimated dollars over calls with a price.
    pub usd: f64,
    /// The same, by kind of token.
    pub parts: Parts,
    /// What those calls would have cost with no prompt cache.
    pub without_cache_usd: f64,
    /// Share of prompt tokens read from the cache, 0 to 1.
    pub cache_hit_rate: Option<f64>,
    pub calls: i64,
    /// Calls with tokens but no price: the estimate leaves them out.
    pub unpriced_calls: i64,
    pub unpriced_models: Vec<String>,
    /// Calls served fast (priced higher).
    pub fast_calls: i64,
    /// Most costly first; models with no price last.
    pub models: Vec<ModelCost>,
    /// The day the built-in price list was last checked.
    pub priced_on: &'static str,
    /// Why the user's `prices.yaml` was left out, if it couldn't be read.
    pub prices_error: Option<String>,
}

const COLUMNS: &str = "e.model, e.tok_in, e.tok_out, e.tok_cache_read, e.tok_cache_write,
     e.tok_cache_write_1h, e.tier";
const HAS_TOKENS: &str = "e.tok_in + e.tok_out + e.tok_cache_read + e.tok_cache_write > 0";

/// Cost across everything a filter covers.
pub fn costs(conn: &Connection, prices: &Prices, f: &Filter) -> Result<Costs> {
    let (cond, args) = conditions(f);
    let sql = format!(
        "SELECT {COLUMNS} FROM events e JOIN sessions s ON s.id = e.session_id
         WHERE {cond} AND {HAS_TOKENS}"
    );
    collect(conn, prices, &sql, &args)
}

/// Cost of one session and its subagents.
pub fn session_costs(conn: &Connection, prices: &Prices, id: &str) -> Result<Costs> {
    let sql = format!(
        "SELECT {COLUMNS} FROM events e JOIN sessions s ON s.id = e.session_id
         WHERE (s.id = ?1 OR s.parent_id = ?1) AND {HAS_TOKENS}"
    );
    collect(conn, prices, &sql, &[id.to_string()])
}

fn collect(conn: &Connection, prices: &Prices, sql: &str, args: &[String]) -> Result<Costs> {
    let mut out = Costs {
        priced_on: PRICED_ON,
        ..Default::default()
    };
    let mut models: BTreeMap<Option<String>, ModelCost> = BTreeMap::new();
    let (mut prompt, mut read) = (0u64, 0u64);
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query(params_from_iter(args))?;
    while let Some(r) = rows.next()? {
        let u = usage_from_row(r, 0)?;
        let m = models.entry(u.model.clone()).or_insert_with(|| ModelCost {
            model: u.model.clone(),
            ..Default::default()
        });
        m.calls += 1;
        m.input_tokens += u.input as i64;
        m.output_tokens += u.output as i64;
        m.cache_read_tokens += u.cache_read as i64;
        m.cache_write_tokens += u.cache_write as i64;
        out.calls += 1;
        prompt += u.context();
        read += u.cache_read;
        if u.tier == Tier::Fast {
            out.fast_calls += 1;
        }
        match prices.parts(&u) {
            Some(p) => {
                m.usd = Some(m.usd.unwrap_or(0.0) + p.total());
                out.parts.add(&p);
                out.without_cache_usd += prices.cost_without_cache(&u).unwrap_or(0.0);
            }
            None => out.unpriced_calls += 1,
        }
    }
    out.usd = out.parts.total();
    out.cache_hit_rate = (prompt > 0).then(|| read as f64 / prompt as f64);
    out.models = models.into_values().collect();
    out.models.sort_by(|a, b| {
        b.usd
            .partial_cmp(&a.usd)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.calls.cmp(&a.calls))
    });
    out.unpriced_models = out
        .models
        .iter()
        .filter(|m| m.usd.is_none())
        .map(|m| m.model.clone().unwrap_or_else(|| "not recorded".into()))
        .collect();
    Ok(out)
}
