//! The laptop side: join a team server with an invite, then push session
//! metadata and the fleet inventory. Nothing is sent until `join`.
//!
//! `team.json` next to the index holds the link (server, device token,
//! team). `team-sync.json` holds which session versions the server already
//! has, plus when the last sync ran and how it went.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use agentcam_core::share;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::server::{JoinReq, SyncReq, SyncResp};
use crate::{Error, Result, now};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub server: String,
    pub device_token: String,
    pub team: String,
    pub machine_id: String,
    pub joined_at: String,
    #[serde(default)]
    pub paused: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncState {
    /// Session id → version the server has.
    #[serde(default)]
    pub sent: HashMap<String, String>,
    pub last_sync: Option<String>,
    pub last_error: Option<String>,
    #[serde(default)]
    pub last_sent: usize,
}

pub fn link_path(db: &Path) -> PathBuf {
    db.with_file_name("team.json")
}

pub fn state_path(db: &Path) -> PathBuf {
    db.with_file_name("team-sync.json")
}

pub fn load_link(db: &Path) -> Option<Link> {
    serde_json::from_str(&std::fs::read_to_string(link_path(db)).ok()?).ok()
}

pub fn save_link(db: &Path, link: &Link) -> Result<()> {
    write_private(
        &link_path(db),
        &serde_json::to_string_pretty(link).unwrap_or_default(),
    )
}

pub fn load_state(db: &Path) -> SyncState {
    std::fs::read_to_string(state_path(db))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_state(db: &Path, s: &SyncState) -> Result<()> {
    write_private(
        &state_path(db),
        &serde_json::to_string(s).unwrap_or_default(),
    )
}

/// The link file holds a token; keep it readable by this user only.
fn write_private(path: &Path, text: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, text)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// `https://` always; `http://` only to this machine, unless allowed.
pub fn check_url(server: &str, allow_http: bool) -> Result<String> {
    let s = server.trim().trim_end_matches('/').to_string();
    let host = s
        .split("://")
        .nth(1)
        .unwrap_or("")
        .split(['/', ':'])
        .next()
        .unwrap_or("");
    if s.starts_with("https://") {
        return Ok(s);
    }
    if s.starts_with("http://") {
        if allow_http || matches!(host, "127.0.0.1" | "localhost" | "[::1]") {
            return Ok(s);
        }
        return Err(Error::Bad(
            "use https:// for a team server on another machine (or pass --allow-http)".into(),
        ));
    }
    Err(Error::Bad("the server address starts with https://".into()))
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(60))
        .build()
}

fn post<T: Serialize, R: for<'de> Deserialize<'de>>(
    url: &str,
    token: Option<&str>,
    body: &T,
) -> Result<R> {
    let mut req = agent().post(url).set("Content-Type", "application/json");
    if let Some(t) = token {
        req = req.set("Authorization", &format!("Bearer {t}"));
    }
    let res = req.send_string(&serde_json::to_string(body).unwrap_or_default());
    match res {
        Ok(r) => {
            let text = r.into_string().map_err(|e| Error::Http(e.to_string()))?;
            serde_json::from_str(&text)
                .map_err(|e| Error::Http(format!("bad answer from the server: {e}")))
        }
        Err(ureq::Error::Status(code, r)) => {
            let msg = r
                .into_string()
                .ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                .and_then(|j| j["error"].as_str().map(str::to_string))
                .unwrap_or_default();
            Err(if code == 401 {
                Error::Denied(msg)
            } else {
                Error::Http(format!("server said {code}: {msg}"))
            })
        }
        Err(e) => Err(Error::Http(format!("can't reach {url}: {e}"))),
    }
}

/// Trade an invite for a device token and save the link.
pub fn join(
    db: &Path,
    server: &str,
    invite: &str,
    machine_id: &str,
    os_user: Option<String>,
    allow_http: bool,
) -> Result<Link> {
    let server = check_url(server, allow_http)?;
    let j: crate::store::Joined = post(
        &format!("{server}/api/team/join"),
        None,
        &JoinReq {
            invite: invite.trim().to_string(),
            machine_id: machine_id.to_string(),
            os_user,
        },
    )?;
    let link = Link {
        server,
        device_token: j.device_token,
        team: j.team,
        machine_id: machine_id.to_string(),
        joined_at: now(),
        paused: false,
    };
    save_link(db, &link)?;
    // A new link starts from scratch: the server has nothing yet.
    save_state(db, &SyncState::default())?;
    Ok(link)
}

pub fn leave(db: &Path) -> Result<bool> {
    let had = link_path(db).exists();
    let _ = std::fs::remove_file(link_path(db));
    let _ = std::fs::remove_file(state_path(db));
    Ok(had)
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SyncReport {
    pub sent: usize,
    pub written: usize,
    pub unchanged: usize,
}

/// Sessions per request.
const BATCH: usize = 200;

/// Push sessions the server doesn't have at their current version, plus
/// the inventory. `home` is where agent config lives.
pub fn sync(
    conn: &Connection,
    db: &Path,
    home: &Path,
    agentcam_version: &str,
) -> Result<SyncReport> {
    let link = load_link(db)
        .ok_or_else(|| Error::Bad("not joined to a team; run `agentcam team join`".into()))?;
    if link.paused {
        return Ok(SyncReport::default());
    }
    let mut state = load_state(db);
    let result = push(conn, &link, &mut state, home, agentcam_version);
    state.last_sync = Some(now());
    state.last_error = result.as_ref().err().map(|e| e.to_string());
    if let Ok(r) = &result {
        state.last_sent = r.sent;
    }
    save_state(db, &state)?;
    result
}

fn push(
    conn: &Connection,
    link: &Link,
    state: &mut SyncState,
    home: &Path,
    version: &str,
) -> Result<SyncReport> {
    let all = share::versions(conn)?;
    let changed: Vec<String> = all
        .iter()
        .filter(|(id, v)| state.sent.get(id) != Some(v))
        .map(|(id, _)| id.clone())
        .collect();
    let mut report = SyncReport {
        unchanged: all.len() - changed.len(),
        ..Default::default()
    };
    let url = format!("{}/api/team/sync", link.server);
    let inventory = agentcam_core::fleet::inventory(conn, home, version)?;
    let mut chunks: Vec<&[String]> = changed.chunks(BATCH).collect();
    if chunks.is_empty() {
        // Still send the inventory, so the fleet view stays current.
        chunks.push(&[]);
    }
    for (i, ids) in chunks.into_iter().enumerate() {
        let metas = share::metas(conn, ids)?;
        let body = SyncReq {
            schema_version: share::SCHEMA_VERSION,
            agentcam_version: Some(version.to_string()),
            inventory: (i == 0).then(|| inventory.clone()),
            sessions: metas.clone(),
        };
        let r: SyncResp = post(&url, Some(&link.device_token), &body)?;
        for m in metas {
            state.sent.insert(m.session_id, m.version);
        }
        report.sent += r.received;
        report.written += r.written;
    }
    Ok(report)
}

/// What the local UI shows about the link. No token.
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub joined: bool,
    pub server: Option<String>,
    pub team: Option<String>,
    pub joined_at: Option<String>,
    pub paused: bool,
    pub last_sync: Option<String>,
    pub last_error: Option<String>,
    pub sessions_on_server: usize,
}

pub fn status(db: &Path) -> Status {
    let link = load_link(db);
    let st = load_state(db);
    Status {
        joined: link.is_some(),
        server: link.as_ref().map(|l| l.server.clone()),
        team: link.as_ref().map(|l| l.team.clone()),
        joined_at: link.as_ref().map(|l| l.joined_at.clone()),
        paused: link.as_ref().is_some_and(|l| l.paused),
        last_sync: st.last_sync,
        last_error: st.last_error,
        sessions_on_server: st.sent.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_need_https_off_this_machine() {
        assert!(check_url("https://team.example.com/", false).is_ok());
        assert!(check_url("http://127.0.0.1:4850", false).is_ok());
        assert!(check_url("http://localhost:4850", false).is_ok());
        assert!(check_url("http://team.example.com", false).is_err());
        assert!(check_url("http://team.example.com", true).is_ok());
        assert!(check_url("team.example.com", true).is_err());
    }
}
