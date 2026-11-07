//! agentcam for teams: a self-hosted server that collects session metadata
//! from laptops, and the laptop side that joins and syncs.
//!
//! What a laptop sends is fixed by `agentcam_core::share` (session
//! metadata) and `agentcam_core::fleet` (setup names). Never prompts,
//! replies, commands, output, diffs or file contents. Laptops push; the
//! server never reaches out to them.

pub mod baseline;
pub mod client;
pub mod server;
pub mod siem;
pub mod store;
pub mod views;

use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Index(#[from] agentcam_core::index::IndexError),
    #[error("{0}")]
    Http(String),
    #[error("{0}")]
    Bad(String),
    #[error("{0}")]
    Denied(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// A random token with a readable prefix: `ac_dev_…`, `ac_view_…`, `ac_inv_…`.
pub fn new_token(kind: &str) -> String {
    let mut b = [0u8; 24];
    getrandom::fill(&mut b).expect("the OS random source works");
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("ac_{kind}_{hex}")
}

/// Tokens are stored as SHA-256 hashes, never as given.
pub fn hash_token(token: &str) -> String {
    Sha256::digest(token.trim().as_bytes())
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

pub fn now() -> String {
    agentcam_core::util::format_ts_ms(agentcam_core::util::now_ms())
}
