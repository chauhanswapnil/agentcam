//! Who and where a session came from, so indexes from many laptops can
//! later merge on a team server: a stable machine id, the OS user, and a
//! project id taken from the git remote (paths differ per laptop; remotes
//! don't).

use std::hash::{Hash, Hasher};
use std::path::Path;

/// A stable id for this machine, kept in `machine-id` next to the index.
/// Created once; random, not derived from hardware.
pub fn machine_id(dir: &Path) -> String {
    let path = dir.join("machine-id");
    if let Ok(id) = std::fs::read_to_string(&path) {
        let id = id.trim();
        if id.len() >= 16 {
            return id.to_string();
        }
    }
    let id = random_id();
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(&path, format!("{id}\n"));
    id
}

/// 32 hex characters from the clock, process id and stack address. Good
/// enough for an id; not for secrets.
pub fn random_id() -> String {
    let mut out = String::new();
    for salt in 0u8..2 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        std::time::SystemTime::now().hash(&mut h);
        std::process::id().hash(&mut h);
        (&out as *const String as usize).hash(&mut h);
        salt.hash(&mut h);
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

/// The OS account name, if the environment says.
pub fn os_user() -> Option<String> {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .ok()
        .filter(|u| !u.is_empty())
}

/// The `origin` remote of the repo at `root` as `host/owner/repo`, read
/// from `.git/config` (no git command runs). Credentials in the URL are
/// dropped.
pub fn git_remote(root: &Path) -> Option<String> {
    let git = root.join(".git");
    let config = if git.is_dir() {
        git.join("config")
    } else {
        // A worktree or submodule: `.git` is a file pointing at the real dir.
        let pointer = std::fs::read_to_string(&git).ok()?;
        let dir = pointer.trim().strip_prefix("gitdir:")?.trim();
        let dir = root.join(dir);
        // Worktrees keep config in the common dir two levels up.
        let common = dir.join("commondir");
        match std::fs::read_to_string(&common) {
            Ok(c) => dir.join(c.trim()).join("config"),
            Err(_) => dir.join("config"),
        }
    };
    let text = std::fs::read_to_string(config).ok()?;
    let mut in_origin = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_origin = l == "[remote \"origin\"]";
        } else if in_origin
            && let Some(url) = l
                .strip_prefix("url")
                .map(str::trim_start)
                .and_then(|r| r.strip_prefix('='))
        {
            return normalize_remote(url.trim());
        }
    }
    None
}

/// `git@github.com:o/r.git`, `https://u:t@github.com/o/r`, `ssh://git@host:22/o/r`
/// all become `github.com/o/r` or `host/o/r`.
pub fn normalize_remote(url: &str) -> Option<String> {
    let url = url.trim().trim_end_matches('/');
    let rest = if let Some(i) = url.find("://") {
        let after = &url[i + 3..];
        let after = after.rsplit_once('@').map_or(after, |(_, h)| h);
        let (host, path) = after.split_once('/')?;
        let host = host.split(':').next()?;
        format!("{host}/{path}")
    } else if let Some((left, path)) = url.split_once(':') {
        // scp-like `user@host:path`. A Windows drive (`C:\`) or a local
        // path isn't a remote.
        if left.len() <= 1 || left.contains('/') {
            return None;
        }
        let host = left.rsplit_once('@').map_or(left, |(_, h)| h);
        format!("{host}/{}", path.trim_start_matches('/'))
    } else {
        return None;
    };
    let rest = rest.trim_end_matches(".git").to_lowercase();
    (rest.contains('/') && !rest.contains("..")).then_some(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_remotes() {
        let n = |u| normalize_remote(u);
        assert_eq!(
            n("git@github.com:Chauhan/Agentcam.git").as_deref(),
            Some("github.com/chauhan/agentcam")
        );
        assert_eq!(
            n("https://user:tok3n@github.com/o/r.git").as_deref(),
            Some("github.com/o/r")
        );
        assert_eq!(
            n("ssh://git@gitlab.example.com:2222/team/app").as_deref(),
            Some("gitlab.example.com/team/app")
        );
        assert_eq!(
            n("https://github.com/o/r/").as_deref(),
            Some("github.com/o/r")
        );
        assert_eq!(n("/srv/git/repo.git"), None);
        assert_eq!(n("C:\\repos\\x"), None);
    }

    #[test]
    fn reads_origin_and_keeps_machine_id() {
        let dir = std::env::temp_dir().join(format!("agentcam-id-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(
            dir.join(".git/config"),
            "[core]\n\tbare = false\n[remote \"upstream\"]\n\turl = git@github.com:x/y.git\n[remote \"origin\"]\n\turl = https://t@github.com/me/app.git\n",
        )
        .unwrap();
        assert_eq!(git_remote(&dir).as_deref(), Some("github.com/me/app"));
        let a = machine_id(&dir);
        assert_eq!(a, machine_id(&dir), "stable across calls");
        assert_eq!(a.len(), 32);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
