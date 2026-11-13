//! Model prices, for estimating what sessions cost.
//!
//! A call's cost has four parts: fresh input, output, cache reads and cache
//! writes. Cache writes kept for an hour cost more than the 5-minute kind,
//! a long prompt can move a call to a higher price, and a fast or flex call
//! is a multiple of the standard one.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::{Tier, TokenUsage};

const BUILTIN: &str = include_str!("../rules/prices.yaml");

/// The day the built-in list was last checked against the price pages.
pub const PRICED_ON: &str = "2026-11-11";

/// Bump when the cost maths changes, so every session is priced again.
const ENGINE: &str = "prices-2";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Price {
    /// A model id or the start of one.
    #[serde(rename = "match")]
    pub matches: String,
    /// Dollars per million tokens.
    pub input: f64,
    pub output: f64,
    #[serde(default)]
    pub cache_read: Option<f64>,
    /// Cache writes kept 5 minutes.
    #[serde(default)]
    pub cache_write: Option<f64>,
    /// Cache writes kept 1 hour.
    #[serde(default)]
    pub cache_write_1h: Option<f64>,
    /// Prices for a call whose prompt is over a size.
    #[serde(default)]
    pub long: Option<Long>,
    /// What a fast call costs, as a multiple of standard.
    #[serde(default)]
    pub fast: Option<f64>,
    /// What a flex call costs, as a multiple of standard.
    #[serde(default)]
    pub flex: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Long {
    /// Prompt tokens (fresh input, cache reads and cache writes) above
    /// which these prices apply to the whole call.
    pub over: u64,
    pub input: f64,
    pub output: f64,
    #[serde(default)]
    pub cache_read: Option<f64>,
}

/// The dollars one call (or many, added up) cost, by kind of token.
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq)]
pub struct Parts {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

impl Parts {
    pub fn total(&self) -> f64 {
        self.input + self.output + self.cache_read + self.cache_write
    }

    pub fn add(&mut self, o: &Parts) {
        self.input += o.input;
        self.output += o.output;
        self.cache_read += o.cache_read;
        self.cache_write += o.cache_write;
    }
}

/// The per-million rates that apply to one call.
struct Rates {
    input: f64,
    output: f64,
    cache_read: f64,
    cache_write: f64,
    cache_write_1h: f64,
}

impl Price {
    fn rates(&self, u: &TokenUsage) -> Rates {
        let long = self.long.as_ref().filter(|l| u.context() > l.over);
        let (input, output) = long.map_or((self.input, self.output), |l| (l.input, l.output));
        // Cache prices follow input when a long prompt raises it.
        let scale = if self.input > 0.0 {
            input / self.input
        } else {
            1.0
        };
        let cache_read = long
            .and_then(|l| l.cache_read)
            .unwrap_or_else(|| self.cache_read.unwrap_or(self.input * 0.10) * scale);
        let m = match u.tier {
            Tier::Standard => 1.0,
            Tier::Fast => self.fast.unwrap_or(2.0),
            Tier::Flex => self.flex.unwrap_or(0.5),
        };
        Rates {
            input: input * m,
            output: output * m,
            cache_read: cache_read * m,
            cache_write: self.cache_write.unwrap_or(self.input * 1.25) * scale * m,
            cache_write_1h: self.cache_write_1h.unwrap_or(self.input * 2.0) * scale * m,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Prices {
    list: Vec<Price>,
    pub fingerprint: String,
}

impl Prices {
    pub fn builtin() -> Self {
        Self::parse(&[BUILTIN]).expect("built-in prices are valid")
    }

    /// Built-in prices plus the user's `prices.yaml`, if present. A user
    /// entry with the same `match` replaces the built-in one.
    pub fn with_user_file(path: &Path) -> Result<Self, serde_yaml_ng::Error> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&[BUILTIN, &text]),
            Err(_) => Ok(Self::builtin()),
        }
    }

    pub fn parse(sources: &[&str]) -> Result<Self, serde_yaml_ng::Error> {
        let mut list: Vec<Price> = Vec::new();
        for src in sources {
            for p in serde_yaml_ng::from_str::<Option<Vec<Price>>>(src)?.unwrap_or_default() {
                list.retain(|x| x.matches != p.matches);
                list.push(p);
            }
        }
        // Longest match first, so `claude-opus-5-5` beats `claude-opus-5`.
        list.sort_by_key(|p| std::cmp::Reverse(p.matches.len()));
        let fingerprint = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            ENGINE.hash(&mut h);
            sources.hash(&mut h);
            format!("{:016x}", h.finish())
        };
        Ok(Prices { list, fingerprint })
    }

    pub fn find(&self, model: &str) -> Option<&Price> {
        self.list.iter().find(|p| model.starts_with(&p.matches))
    }

    /// Dollars for one call by kind of token, or `None` when the model has
    /// no price.
    pub fn parts(&self, u: &TokenUsage) -> Option<Parts> {
        let r = self.find(u.model.as_deref()?)?.rates(u);
        let per = |tokens: u64, price: f64| tokens as f64 * price / 1_000_000.0;
        let hour = u.cache_write_1h.min(u.cache_write);
        Some(Parts {
            input: per(u.input, r.input),
            output: per(u.output, r.output),
            cache_read: per(u.cache_read, r.cache_read),
            cache_write: per(u.cache_write - hour, r.cache_write) + per(hour, r.cache_write_1h),
        })
    }

    /// Dollars for one call, or `None` when the model has no price.
    pub fn cost(&self, u: &TokenUsage) -> Option<f64> {
        self.parts(u).map(|p| p.total())
    }

    /// What the call would have cost with no prompt cache: every prompt
    /// token at the fresh-input price.
    pub fn cost_without_cache(&self, u: &TokenUsage) -> Option<f64> {
        let r = self.find(u.model.as_deref()?)?.rates(u);
        let per = |tokens: u64, price: f64| tokens as f64 * price / 1_000_000.0;
        Some(per(u.context(), r.input) + per(u.output, r.output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(model: &str, input: u64, output: u64, cache_read: u64) -> TokenUsage {
        TokenUsage {
            model: Some(model.into()),
            input,
            output,
            cache_read,
            ..Default::default()
        }
    }

    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    const M: u64 = 1_000_000;

    #[test]
    fn longest_match_wins() {
        let p = Prices::builtin();
        assert_eq!(p.find("claude-opus-5-5").unwrap().input, 4.0);
        assert_eq!(p.find("claude-opus-5").unwrap().input, 5.0);
        // Fable 5.2 has its own row; it used to fall through to Fable 5.
        assert_eq!(p.find("claude-fable-5-2").unwrap().input, 8.0);
        assert_eq!(p.find("claude-fable-5").unwrap().input, 10.0);
        assert_eq!(p.find("gpt-5.5-pro").unwrap().input, 30.0);
        assert!(p.find("some-new-model").is_none());
    }

    #[test]
    fn costs() {
        let p = Prices::builtin();
        // 1M in at $4 + 1M out at $20 + 1M cache reads at $0.20.
        near(p.cost(&usage("claude-opus-5-5", M, M, M)).unwrap(), 24.2);
        // No published cache price: 10% of input.
        near(p.cost(&usage("claude-opus-5", 0, 0, M)).unwrap(), 0.5);
        assert!(p.cost(&usage("some-new-model", 10, 10, 0)).is_none());
    }

    #[test]
    fn hour_long_cache_writes_cost_more() {
        let p = Prices::builtin();
        let mut u = usage("claude-opus-5-5", 0, 0, 0);
        u.cache_write = M;
        // All 5-minute: 125% of $4.
        near(p.cost(&u).unwrap(), 5.0);
        // All 1-hour: 200% of $4.
        u.cache_write_1h = M;
        near(p.cost(&u).unwrap(), 8.0);
        // Half and half.
        u.cache_write_1h = M / 2;
        near(p.cost(&u).unwrap(), 6.5);
        // A bad log can't push the 1-hour share past the whole.
        u.cache_write_1h = 5 * M;
        near(p.cost(&u).unwrap(), 8.0);
    }

    #[test]
    fn a_long_prompt_moves_to_the_higher_price() {
        let p = Prices::builtin();
        // At the line: the base price.
        let mut u = usage("claude-haiku-5-5", 100_000, M, 0);
        near(p.cost(&u).unwrap(), 0.01 + 0.5);
        // One token over, counting cache reads: every part is 5x.
        u.input = 50_001;
        u.cache_read = 50_000;
        let parts = p.parts(&u).unwrap();
        near(parts.output, 2.5);
        near(parts.input, 50_001.0 * 0.5 / 1e6);
        near(parts.cache_read, 50_000.0 * 0.05 / 1e6);
        // A near miss: a model with no long price is flat at any size.
        near(
            p.cost(&usage("claude-opus-5-5", 900_000, 0, 0)).unwrap(),
            3.6,
        );
        // OpenAI names its own long cache price.
        near(p.cost(&usage("gpt-6.1-sol", 0, 0, M)).unwrap(), 0.20);
    }

    #[test]
    fn fast_and_flex_are_multiples() {
        let p = Prices::builtin();
        let mut u = usage("claude-opus-5-5", M, M, 0);
        near(p.cost(&u).unwrap(), 24.0);
        u.tier = Tier::Fast;
        near(p.cost(&u).unwrap(), 48.0);
        let mut g = usage("gpt-5.5", 100_000, 0, 0);
        g.tier = Tier::Fast;
        near(p.cost(&g).unwrap(), 0.5 * 2.5);
        g.tier = Tier::Flex;
        near(p.cost(&g).unwrap(), 0.25);
    }

    #[test]
    fn openai_cache_writes_cost_as_input() {
        let p = Prices::builtin();
        let mut u = usage("gpt-6.1-sol", 0, 0, 0);
        u.cache_write = 100_000;
        near(p.cost(&u).unwrap(), 0.2);
    }

    #[test]
    fn without_cache_prices_every_prompt_token_as_fresh() {
        let p = Prices::builtin();
        let u = usage("claude-opus-5-5", 0, 0, M);
        near(p.cost_without_cache(&u).unwrap(), 4.0);
        near(p.cost(&u).unwrap(), 0.2);
    }

    #[test]
    fn user_prices_override() {
        let p =
            Prices::parse(&[BUILTIN, "- { match: my-model, input: 1.0, output: 8.0 }"]).unwrap();
        assert_eq!(p.find("my-model-v2").unwrap().output, 8.0);
        // The same `match` replaces a built-in row.
        let p = Prices::parse(&[
            BUILTIN,
            "- { match: claude-opus-5, input: 1.0, output: 2.0 }",
        ])
        .unwrap();
        assert_eq!(p.find("claude-opus-5").unwrap().input, 1.0);
        assert_eq!(p.find("claude-opus-5-5").unwrap().input, 4.0);
    }
}
