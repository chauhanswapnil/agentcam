//! Model prices, for estimating what sessions cost.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::TokenUsage;

const BUILTIN: &str = include_str!("../rules/prices.yaml");

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
    #[serde(default)]
    pub cache_write: Option<f64>,
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
            sources.hash(&mut h);
            format!("{:016x}", h.finish())
        };
        Ok(Prices { list, fingerprint })
    }

    pub fn find(&self, model: &str) -> Option<&Price> {
        self.list.iter().find(|p| model.starts_with(&p.matches))
    }

    /// Dollars for one call, or `None` when the model has no price.
    pub fn cost(&self, u: &TokenUsage) -> Option<f64> {
        let p = self.find(u.model.as_deref()?)?;
        let per = |tokens: u64, price: f64| tokens as f64 * price / 1_000_000.0;
        Some(
            per(u.input, p.input)
                + per(u.output, p.output)
                + per(u.cache_read, p.cache_read.unwrap_or(p.input * 0.10))
                + per(u.cache_write, p.cache_write.unwrap_or(p.input * 1.25)),
        )
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
            cache_write: 0,
        }
    }

    #[test]
    fn longest_match_wins() {
        let p = Prices::builtin();
        assert_eq!(p.find("claude-opus-5-5").unwrap().input, 4.0);
        assert_eq!(p.find("claude-opus-5").unwrap().input, 5.0);
        assert!(p.find("gpt-6.1-sol").is_none());
    }

    #[test]
    fn costs() {
        let p = Prices::builtin();
        // 1M in at $4 + 1M out at $20 + 1M cache reads at $0.20.
        let c = p
            .cost(&usage("claude-opus-5-5", 1_000_000, 1_000_000, 1_000_000))
            .unwrap();
        assert!((c - 24.2).abs() < 1e-9, "{c}");
        // No published cache price: 10% of input.
        let c = p.cost(&usage("claude-opus-5", 0, 0, 1_000_000)).unwrap();
        assert!((c - 0.5).abs() < 1e-9, "{c}");
        assert!(p.cost(&usage("gpt-6.1-sol", 10, 10, 0)).is_none());
    }

    #[test]
    fn user_prices_override() {
        let p = Prices::parse(&[BUILTIN, "- { match: gpt-6, input: 1.0, output: 8.0 }"]).unwrap();
        assert_eq!(p.find("gpt-6.1-sol").unwrap().output, 8.0);
    }
}
