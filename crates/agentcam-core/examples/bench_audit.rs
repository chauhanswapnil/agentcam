//! Times parsing and the risk audit over the real logs on this machine.
//! `cargo run --release -p agentcam-core --example bench_audit`
use std::time::Instant;

fn main() {
    let rules = match std::env::var("RULES") {
        Ok(f) => {
            agentcam_core::audit::Rules::parse(&[&std::fs::read_to_string(f).unwrap()]).unwrap()
        }
        Err(_) => agentcam_core::audit::Rules::builtin(),
    };
    let mut parsed = Vec::new();
    let t = Instant::now();
    for src in agentcam_core::default_log_sources() {
        for entry in walk(&src.path) {
            let p = match src.agent {
                agentcam_core::Agent::ClaudeCode => agentcam_core::claude::parse(&entry),
                agentcam_core::Agent::Codex => agentcam_core::codex::parse(&entry),
            };
            if let Ok(p) = p {
                parsed.push(p);
            }
        }
    }
    println!("parse: {:?} ({} sessions)", t.elapsed(), parsed.len());
    let t = Instant::now();
    let mut n = 0;
    let rounds: usize = std::env::var("ROUNDS")
        .ok()
        .and_then(|r| r.parse().ok())
        .unwrap_or(1);
    for _ in 0..rounds {
        for p in &parsed {
            n += rules.check(p).len();
        }
    }
    if rounds > 1 {
        return;
    }
    println!("audit: {:?} ({n} findings)", t.elapsed());

    // Each rule alone, slowest first.
    let yaml = include_str!("../rules/default.yaml");
    let blocks: Vec<String> = yaml
        .split("\n- id: ")
        .skip(1)
        .map(|b| format!("- id: {b}"))
        .collect();
    let mut times = Vec::new();
    for b in &blocks {
        let Ok(r) = agentcam_core::audit::Rules::parse(&[b]) else {
            continue;
        };
        let t = Instant::now();
        for p in &parsed {
            r.check(p);
        }
        times.push((t.elapsed(), b.lines().next().unwrap_or("").to_string()));
    }
    let empty = agentcam_core::audit::Rules::parse(&["[]"]).unwrap();
    let t = Instant::now();
    for p in &parsed {
        empty.check(p);
    }
    println!("no rules: {:?}", t.elapsed());
    times.sort_by_key(|t| std::cmp::Reverse(t.0));
    for (d, id) in times.iter().take(12) {
        println!("{d:>12?}  {id}");
    }
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else if p.extension().is_some_and(|x| x == "jsonl") && !p.ends_with("journal.jsonl") {
            out.push(p);
        }
    }
    out
}
