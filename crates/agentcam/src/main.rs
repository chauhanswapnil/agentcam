use std::path::PathBuf;

use agentcam_core::Index;
use anyhow::Context;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

const DEFAULT_PORT: u16 = 4747;

/// A dashcam for your coding agents.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Index database file. Defaults to the user data folder.
    #[arg(long, global = true, env = "AGENTCAM_DB")]
    db: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Start the local web UI (the default when no command is given).
    Serve {
        /// Port to listen on. Falls back to a free port if taken.
        #[arg(long, short, default_value_t = DEFAULT_PORT)]
        port: u16,
        /// Don't open the browser.
        #[arg(long)]
        no_open: bool,
    },
    /// Read new and changed agent logs into the index, then print a summary.
    Index {
        /// Re-read every log, not just the changed ones.
        #[arg(long)]
        full: bool,
    },
    /// Index, then print what the risk rules found.
    Scan {
        /// Only show findings at or above this severity: low, medium, high, critical.
        #[arg(long, default_value = "medium")]
        min: String,
        /// Time range: 7d, 30d, 90d, 365d or all.
        #[arg(long, default_value = "all")]
        range: String,
        /// Print every finding instead of a summary per rule.
        #[arg(long)]
        all: bool,
    },
    /// Claude Code PreToolUse hook: reads the tool call from stdin and
    /// exits 2 (block) if a guard rule matches. Set up from the Audit page.
    Guard,
    /// Export sessions as OpenTelemetry traces (OTLP/JSON).
    Export {
        #[command(subcommand)]
        format: ExportFormat,
    },
    /// Write an evidence bundle (one session or a date range) as JSON, with a
    /// SHA-256 checksum. `agentcam evidence verify <file>` checks one.
    Evidence {
        #[command(subcommand)]
        check: Option<EvidenceCommand>,
        /// One session by id.
        #[arg(long)]
        session: Option<String>,
        /// Time range when no session is given: 7d, 30d, 90d or all.
        #[arg(long, default_value = "7d")]
        range: String,
        /// Only this project root.
        #[arg(long)]
        project: Option<String>,
        /// File to write (default agentcam-evidence-<date>.json).
        #[arg(long, short)]
        out: Option<PathBuf>,
    },
    /// Team server and joining one (metadata only; see the Outbox page).
    Team {
        #[command(subcommand)]
        command: TeamCommand,
    },
    /// Which agent session wrote each line of a file, as it is today.
    Blame { file: PathBuf },
    /// Read-only MCP server over stdio, so agents can look up past sessions.
    /// Add it with: claude mcp add agentcam -- agentcam mcp
    Mcp,
    /// Full-text search across all indexed sessions.
    Search {
        /// Words to look for (SQLite FTS5 syntax).
        query: Vec<String>,
        #[arg(long, short, default_value_t = 20)]
        limit: usize,
    },
}

#[derive(Subcommand)]
enum TeamCommand {
    /// Run a team server. Laptops push session metadata to it.
    Serve {
        /// Folder for the team database. Defaults to `agentcam-team` in the data folder.
        #[arg(long)]
        data: Option<PathBuf>,
        /// Address to listen on. Use 0.0.0.0:4850 to reach it from other machines (put TLS in front).
        #[arg(long, default_value = "127.0.0.1:4850")]
        bind: String,
    },
    /// Make a token for the team UI (on the server machine).
    Token {
        #[arg(long)]
        data: Option<PathBuf>,
        /// Who it's for, shown in the audit log.
        #[arg(long, default_value = "Admin")]
        name: String,
        /// admin or viewer.
        #[arg(long, default_value = "admin")]
        role: String,
    },
    /// Make an invite code for a team (on the server machine).
    Invite {
        /// Team name; created if new.
        team: String,
        #[arg(long)]
        data: Option<PathBuf>,
        /// How many machines may use it.
        #[arg(long, default_value_t = 50)]
        uses: i64,
        /// Days until it expires.
        #[arg(long, default_value_t = 7)]
        days: i64,
    },
    /// List joined machines (on the server machine).
    Machines {
        #[arg(long)]
        data: Option<PathBuf>,
    },
    /// Remove a machine and everything it sent; its token stops working (on the server machine).
    Remove {
        /// The start of the machine id, as the UI shows it (`Machine abc123`).
        machine: String,
        #[arg(long)]
        data: Option<PathBuf>,
    },
    /// List viewer tokens by id, name and role (on the server machine).
    Viewers {
        #[arg(long)]
        data: Option<PathBuf>,
    },
    /// Revoke a viewer token by id (on the server machine).
    Revoke {
        id: i64,
        #[arg(long)]
        data: Option<PathBuf>,
    },
    /// Join a team server with an invite code.
    Join {
        /// The server, like https://agentcam.example.com
        server: String,
        invite: String,
        /// Allow http:// to a server on another machine (tokens travel unencrypted).
        #[arg(long)]
        allow_http: bool,
    },
    /// Send changed session metadata now.
    Sync,
    /// Show the link and the last sync.
    Status,
    /// Stop sending until `resume`.
    Pause,
    /// Start sending again.
    Resume,
    /// Forget the team link (the server keeps what it has).
    Leave,
}

#[derive(Subcommand)]
enum EvidenceCommand {
    /// Check that a bundle wasn't changed after it was made.
    Verify { file: PathBuf },
}

#[derive(Subcommand)]
enum ExportFormat {
    /// Risk findings, one JSON object per line, for a SIEM or log pipeline.
    Findings {
        /// `ndjson` (agentcam's records) or `ocsf` (OCSF 1.3 Detection Finding).
        #[arg(long, default_value = "ndjson")]
        format: String,
        /// Time range: 7d, 30d, 90d or all. Ignored with --since.
        #[arg(long, default_value = "30d")]
        range: String,
        /// Only findings after this time (RFC 3339), say the last `ts` you shipped.
        #[arg(long)]
        since: Option<String>,
        /// Lowest severity: low, medium, high or critical.
        #[arg(long, default_value = "low")]
        min: String,
        /// File to write (default: stdout).
        #[arg(long, short)]
        out: Option<PathBuf>,
    },
    /// OTLP/JSON traces: write a file, or send to a collector with --endpoint.
    Otlp {
        /// Time range: 7d, 30d, 90d or all.
        #[arg(long, default_value = "7d")]
        range: String,
        /// Only this project root.
        #[arg(long)]
        project: Option<String>,
        /// File to write (default agentcam-traces.json). Ignored with --endpoint.
        #[arg(long, short)]
        out: Option<PathBuf>,
        /// POST to an OTLP/HTTP traces endpoint, e.g. http://localhost:4318/v1/traces.
        /// This sends session data off the machine only if the endpoint is remote.
        #[arg(long)]
        endpoint: Option<String>,
        /// Extra request header for --endpoint, as `name=value` (repeatable).
        #[arg(long = "header")]
        headers: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        // Exit code 2 means "block" to an agent's hook. If the guard hook
        // was set up with arguments this version doesn't know, let the
        // call run rather than block every tool call.
        Err(e) if std::env::args().any(|a| a == "guard") && e.use_stderr() => std::process::exit(0),
        Err(e) => e.exit(),
    };
    // The MCP server speaks JSON-RPC on stdout; nothing else may print there.
    if matches!(cli.command, Some(Command::Mcp)) {
        let db = cli
            .db
            .or_else(agentcam_core::default_db_path)
            .context("no data folder; pass --db")?;
        return mcp(&db);
    }
    // The guard hook must print nothing but its verdict, and be fast.
    if matches!(cli.command, Some(Command::Guard)) {
        let db = cli.db.or_else(agentcam_core::default_db_path);
        // Fail open: a panic would exit 101, which agents report as a hook
        // error; any surprise lets the call run instead.
        std::panic::set_hook(Box::new(|_| {}));
        let verdict = std::panic::catch_unwind(|| guard(db.as_deref())).unwrap_or(0);
        std::process::exit(verdict);
    }
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let db = cli
        .db
        .or_else(agentcam_core::default_db_path)
        .context("can't find a data folder for the index; pass --db")?;
    let command = cli.command.unwrap_or(Command::Serve {
        port: DEFAULT_PORT,
        no_open: false,
    });

    match command {
        Command::Serve { port, no_open } => serve(&db, port, no_open).await,
        Command::Index { full } => index(&db, full),
        Command::Scan { min, range, all } => scan(&db, &min, &range, all),
        Command::Guard => unreachable!("handled before startup"),
        Command::Export { format } => match format {
            ExportFormat::Otlp {
                range,
                project,
                out,
                endpoint,
                headers,
            } => export_otlp(&db, &range, project, out, endpoint, &headers),
            ExportFormat::Findings {
                format,
                range,
                since,
                min,
                out,
            } => export_findings(&db, &format, &range, since, &min, out),
        },
        Command::Evidence {
            check: Some(EvidenceCommand::Verify { file }),
            ..
        } => verify_evidence(&file),
        Command::Evidence {
            check: None,
            session,
            range,
            project,
            out,
        } => evidence(&db, session, &range, project, out),
        Command::Search { query, limit } => search(&db, &query.join(" "), limit),
        Command::Team { command } => team(&db, command).await,
        Command::Blame { file } => blame(&db, &file),
        Command::Mcp => unreachable!("handled before startup"),
    }
}

fn since_days(range: &str) -> Option<String> {
    let days: i64 = range.strip_suffix('d')?.parse().ok()?;
    Some(agentcam_core::util::format_ts_ms(
        agentcam_core::util::now_ms() - days * 86_400_000,
    ))
}

fn export_findings(
    db: &std::path::Path,
    format: &str,
    range: &str,
    since: Option<String>,
    min: &str,
    out: Option<PathBuf>,
) -> anyhow::Result<()> {
    if format != "ndjson" && format != "ocsf" {
        anyhow::bail!("--format is ndjson or ocsf");
    }
    let mut idx = open_index(db)?;
    idx.update(&agentcam_core::default_log_sources(), false)?;
    let f = agentcam_core::query::Filter {
        since: since.or_else(|| since_days(range)),
        ..Default::default()
    };
    let floor = agentcam_core::query::severity_num(min);
    let records: Vec<_> = agentcam_core::siem::findings(idx.conn(), &f, 1_000_000)?
        .into_iter()
        .filter(|r| agentcam_core::query::severity_num(&r.severity) >= floor)
        .collect();
    let text = agentcam_core::siem::lines(&records, format);
    match out {
        Some(path) => {
            std::fs::write(&path, text)?;
            eprintln!("Wrote {} findings to {}", records.len(), path.display());
        }
        None => print!("{text}"),
    }
    Ok(())
}

fn evidence(
    db: &std::path::Path,
    session: Option<String>,
    range: &str,
    project: Option<String>,
    out: Option<PathBuf>,
) -> anyhow::Result<()> {
    let mut idx = open_index(db)?;
    idx.update(&agentcam_core::default_log_sources(), false)?;
    let scope = agentcam_core::evidence::Scope {
        since: if session.is_some() {
            None
        } else {
            since_days(range)
        },
        session_id: session,
        until: None,
        project: project.map(|p| agentcam_core::project_root(&p)),
        agent: None,
    };
    let ev = agentcam_core::evidence::build(idx.conn(), scope, idx.rules())?;
    if ev.sessions.is_empty() {
        anyhow::bail!("no sessions in that range");
    }
    let path = out.unwrap_or_else(|| {
        PathBuf::from(format!(
            "agentcam-evidence-{}.json",
            &ev.generated_at[..ev.generated_at.len().min(10)]
        ))
    });
    std::fs::write(&path, serde_json::to_string_pretty(&ev)?)?;
    println!(
        "Wrote {} ({} sessions, {} steps, sha256 {})",
        path.display(),
        ev.summary.sessions,
        ev.summary.steps,
        ev.integrity
            .as_ref()
            .map_or("", |i| &i.content_sha256[..16])
    );
    if ev.summary.truncated {
        println!(
            "The range held more than {} sessions; narrow it with --range or --project for the rest.",
            agentcam_core::evidence::MAX_SESSIONS
        );
    }
    Ok(())
}

fn verify_evidence(file: &std::path::Path) -> anyhow::Result<()> {
    let text =
        std::fs::read_to_string(file).with_context(|| format!("can't read {}", file.display()))?;
    match agentcam_core::evidence::verify(&text) {
        agentcam_core::evidence::Verdict::Intact => {
            println!("{}: intact (content matches its sha256)", file.display());
            Ok(())
        }
        agentcam_core::evidence::Verdict::Changed { expected, found } => {
            anyhow::bail!(
                "{}: changed after it was made (expected sha256 {expected}, found {found})",
                file.display()
            )
        }
        agentcam_core::evidence::Verdict::NotABundle(why) => {
            anyhow::bail!("{}: not an evidence bundle: {why}", file.display())
        }
    }
}

fn blame(db: &std::path::Path, file: &std::path::Path) -> anyhow::Result<()> {
    let abs =
        std::fs::canonicalize(file).with_context(|| format!("can't find {}", file.display()))?;
    let mut idx = open_index(db)?;
    idx.update(&agentcam_core::default_log_sources(), false)?;
    let b = agentcam_core::survival::blame(idx.conn(), &abs.to_string_lossy())?;
    if b.too_big {
        anyhow::bail!("{} is too big to check", file.display());
    }
    for (i, e) in b.edits.iter().enumerate() {
        println!(
            "[{i}] {} #{:<4} {} {:<11} {:<18} {}",
            &e.session_id[..e.session_id.len().min(8)],
            e.seq,
            &e.ts[..e.ts.len().min(10)],
            e.agent,
            e.model.as_deref().unwrap_or("?"),
            e.prompt
                .as_deref()
                .unwrap_or("")
                .replace('\n', " ")
                .chars()
                .take(60)
                .collect::<String>()
        );
    }
    if !b.edits.is_empty() {
        println!();
    }
    for l in &b.lines {
        let tag = l.edit.map_or("    ".to_string(), |e| format!("[{e}] "));
        println!("{:>5} {tag}{}", l.n, l.text);
    }
    println!(
        "\n{} of {} lines written by agent sessions. Open a step: agentcam, then /sessions/<id>?step=<n>",
        b.agent_lines,
        b.lines.len()
    );
    Ok(())
}

/// Serve MCP over stdio until stdin closes. Picks up new logs once at the
/// start (a few milliseconds when little changed); answers read the index.
fn mcp(db: &std::path::Path) -> anyhow::Result<()> {
    use std::io::{BufRead, Write};
    let mut idx = open_index(db)?;
    let sources = agentcam_core::default_log_sources();
    idx.update(&sources, false)?;
    // The server lives as long as the agent session, so pick up new logs
    // before a tool call, at most every 30 seconds. Only changed files are read.
    let mut indexed = std::time::Instant::now();
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(msg) => {
                if msg.get("method").and_then(|m| m.as_str()) == Some("tools/call")
                    && indexed.elapsed() > std::time::Duration::from_secs(30)
                {
                    // A failed refresh still answers from what's indexed.
                    let _ = idx.update(&sources, false);
                    indexed = std::time::Instant::now();
                }
                agentcam_core::mcp::handle(idx.conn(), idx.rules(), &msg)
            }
            Err(e) => Some(serde_json::json!({
                "jsonrpc": "2.0", "id": null,
                "error": {"code": -32700, "message": format!("parse error: {e}")}
            })),
        };
        if let Some(r) = reply {
            writeln!(out, "{r}")?;
            out.flush()?;
        }
    }
    Ok(())
}

fn team_data(data: Option<PathBuf>, db: &std::path::Path) -> PathBuf {
    data.unwrap_or_else(|| db.with_file_name("agentcam-team"))
}

fn home() -> anyhow::Result<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .context("can't find the home folder")
}

async fn team(db: &std::path::Path, cmd: TeamCommand) -> anyhow::Result<()> {
    use agentcam_team::{client, server, store::Store};
    match cmd {
        TeamCommand::Serve { data, bind } => {
            let dir = team_data(data, db);
            let store = Store::open(&dir.join("team.db"))?;
            if store.viewer_count()? == 0 {
                let t = store.create_viewer("Admin", "admin")?;
                store.log("server", "created first admin token", None)?;
                println!("First start. Your admin token (shown once):\n\n  {t}\n");
                println!("Open http://{bind}/team/login#{t} to sign in.\n");
            }
            let addr: std::net::SocketAddr =
                bind.parse().context("--bind looks like 127.0.0.1:4850")?;
            if !addr.ip().is_loopback() {
                println!(
                    "Listening on {addr}. Put TLS in front (a reverse proxy) before laptops join over a network."
                );
            }
            println!(
                "agentcam team server at http://{addr}  (data in {})",
                dir.display()
            );
            server::serve(addr, server::TeamState::new(store)).await?;
        }
        TeamCommand::Token { data, name, role } => {
            let store = Store::open(&team_data(data, db).join("team.db"))?;
            let t = store.create_viewer(&name, &role)?;
            store.log("cli", "added viewer", Some(&format!("{name} ({role})")))?;
            println!("{t}");
        }
        TeamCommand::Invite {
            team,
            data,
            uses,
            days,
        } => {
            let store = Store::open(&team_data(data, db).join("team.db"))?;
            let id = store.team(&team)?;
            let code = store.create_invite(id, uses, days)?;
            store.log("cli", "created invite", Some(&team))?;
            println!(
                "Invite for {team} ({uses} machines, {days} days):\n\n  agentcam team join <server> {code}"
            );
        }
        TeamCommand::Machines { data } => {
            let store = Store::open(&team_data(data, db).join("team.db"))?;
            let ms = store.machines()?;
            if ms.is_empty() {
                println!("No machines have joined yet.");
            }
            for m in ms {
                println!(
                    "{:<14} {:<16} {:<12} joined {}  last sync {}",
                    agentcam_team::store::short(&m.machine_id),
                    m.team,
                    m.os_user.as_deref().unwrap_or("-"),
                    m.joined_at.get(..10).unwrap_or(&m.joined_at),
                    m.last_sync
                        .as_deref()
                        .map_or("never", |t| t.get(..16).unwrap_or(t)),
                );
            }
        }
        TeamCommand::Remove { machine, data } => {
            let mut store = Store::open(&team_data(data, db).join("team.db"))?;
            let id = store.find_machine(&machine)?;
            store.remove_device(&id)?;
            let name = agentcam_team::store::short(&id);
            store.log("cli", "removed machine", Some(&name))?;
            println!("Removed {name} and everything it sent. It can join again with a new invite.");
        }
        TeamCommand::Viewers { data } => {
            let store = Store::open(&team_data(data, db).join("team.db"))?;
            for v in store.viewers()? {
                println!(
                    "{:>4}  {:<20} {:<7} created {}  last seen {}",
                    v.id,
                    v.name,
                    v.role,
                    v.created_at.get(..10).unwrap_or(&v.created_at),
                    v.last_seen
                        .as_deref()
                        .map_or("never", |t| t.get(..10).unwrap_or(t)),
                );
            }
        }
        TeamCommand::Revoke { id, data } => {
            let store = Store::open(&team_data(data, db).join("team.db"))?;
            let v = store.revoke_viewer(id)?;
            store.log(
                "cli",
                "revoked viewer",
                Some(&format!("{} ({})", v.name, v.role)),
            )?;
            println!("Revoked {} ({}).", v.name, v.role);
        }
        TeamCommand::Join {
            server,
            invite,
            allow_http,
        } => {
            let idx = open_index(db)?;
            let link = client::join(
                db,
                &server,
                &invite,
                idx.machine_id(),
                agentcam_core::identity::os_user(),
                allow_http,
            )?;
            println!("Joined {} on {}.", link.team, link.server);
            println!(
                "agentcam sends session metadata only: counts, times, tokens, findings, file paths in the project. Never prompts, output or diffs."
            );
            println!("See exactly what's sent on the Outbox page. Sending now…");
            team_sync(db)?;
        }
        TeamCommand::Sync => team_sync(db)?,
        TeamCommand::Status => {
            let s = client::status(db);
            if !s.joined {
                println!("Not joined to a team.");
                return Ok(());
            }
            println!("Team:      {}", s.team.unwrap_or_default());
            println!("Server:    {}", s.server.unwrap_or_default());
            println!("Paused:    {}", if s.paused { "yes" } else { "no" });
            println!(
                "Last sync: {}",
                s.last_sync.unwrap_or_else(|| "never".into())
            );
            if let Some(e) = s.last_error {
                println!("Problem:   {e}");
            }
            println!("Sessions on the server: {}", s.sessions_on_server);
        }
        TeamCommand::Pause | TeamCommand::Resume => {
            let paused = matches!(cmd, TeamCommand::Pause);
            let mut link = client::load_link(db).context("not joined to a team")?;
            link.paused = paused;
            client::save_link(db, &link)?;
            println!(
                "{}",
                if paused {
                    "Paused. Nothing is sent until `agentcam team resume`."
                } else {
                    "Sending again."
                }
            );
        }
        TeamCommand::Leave => {
            if client::leave(db)? {
                println!(
                    "Left the team. The server keeps what it already has; ask an admin to remove this machine."
                );
            } else {
                println!("Not joined to a team.");
            }
        }
    }
    Ok(())
}

/// Index, then push what changed.
fn team_sync(db: &std::path::Path) -> anyhow::Result<()> {
    let mut idx = open_index(db)?;
    idx.update(&agentcam_core::default_log_sources(), false)?;
    let r = agentcam_team::client::sync(idx.conn(), db, &home()?, env!("CARGO_PKG_VERSION"))?;
    println!(
        "Sent {} sessions ({} new or changed on the server, {} unchanged here).",
        r.sent, r.written, r.unchanged
    );
    Ok(())
}

/// While the local app runs and this machine is joined, push changes every
/// few minutes on a thread of its own. Errors are kept for `team status`
/// and the Outbox page; they never stop the app.
fn spawn_team_sync(db: PathBuf) {
    if agentcam_team::client::load_link(&db).is_none() {
        return;
    }
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
            if let (Ok(idx), Ok(home)) = (Index::open(&db), home()) {
                if let Err(e) =
                    agentcam_team::client::sync(idx.conn(), &db, &home, env!("CARGO_PKG_VERSION"))
                {
                    tracing::warn!("team sync: {e}");
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(4 * 60));
        }
    });
}

/// Run as a Claude Code `PreToolUse` hook. Returns the exit code: 2 blocks
/// the call (the reason goes to stderr, which Claude Code shows the agent),
/// 0 lets it run. Any problem here lets the call run: a broken guard must
/// never stop the agent.
fn guard(db: Option<&std::path::Path>) -> i32 {
    use std::io::Read;
    let Some(db) = db else { return 0 };
    let Ok(rules) = agentcam_core::guard::load(&agentcam_core::guard::guard_path(db)) else {
        return 0;
    };
    if rules.is_empty() {
        return 0;
    }
    let mut input = String::new();
    if std::io::stdin()
        .take(4 << 20)
        .read_to_string(&mut input)
        .is_err()
    {
        return 0;
    }
    let Ok(call) = serde_json::from_str::<serde_json::Value>(&input) else {
        return 0;
    };
    let tool = call.get("tool_name").and_then(|v| v.as_str()).unwrap_or("");
    let args = call.get("tool_input").cloned().unwrap_or_default();
    match agentcam_core::guard::check(&rules, tool, &args) {
        Some(rule) => {
            eprintln!("Blocked by agentcam ({}): {}", rule.id, rule.reason);
            2
        }
        None => 0,
    }
}

fn export_otlp(
    db: &std::path::Path,
    range: &str,
    project: Option<String>,
    out: Option<PathBuf>,
    endpoint: Option<String>,
    headers: &[String],
) -> anyhow::Result<()> {
    let mut idx = open_index(db)?;
    idx.update(&agentcam_core::default_log_sources(), false)?;
    let days: Option<i64> = range.strip_suffix('d').and_then(|d| d.parse().ok());
    let f = agentcam_core::query::Filter {
        since: days.map(|d| {
            agentcam_core::util::format_ts_ms(agentcam_core::util::now_ms() - d * 86_400_000)
        }),
        project,
        ..Default::default()
    };
    let (body, sessions) = agentcam_core::otel::export(idx.conn(), &f)?;
    let spans = body["resourceSpans"][0]["scopeSpans"][0]["spans"]
        .as_array()
        .map_or(0, Vec::len);
    match endpoint {
        Some(url) => {
            let mut req = ureq::post(&url).set("Content-Type", "application/json");
            for h in headers {
                let (k, v) = h.split_once('=').context("headers look like name=value")?;
                req = req.set(k.trim(), v.trim());
            }
            req.send_string(&body.to_string())
                .map_err(|e| anyhow::anyhow!("sending to {url}: {e}"))?;
            println!("Sent {spans} spans from {sessions} sessions to {url}");
        }
        None => {
            let path = out.unwrap_or_else(|| PathBuf::from("agentcam-traces.json"));
            std::fs::write(&path, serde_json::to_string(&body)?)?;
            println!(
                "Wrote {spans} spans from {sessions} sessions to {}",
                path.display()
            );
            println!(
                "Send it with: curl -X POST -H 'Content-Type: application/json' --data @{} http://localhost:4318/v1/traces",
                path.display()
            );
        }
    }
    Ok(())
}

/// Built-in rules plus `rules.yaml` next to the index, if present.
fn load_rules(db: &std::path::Path) -> anyhow::Result<agentcam_core::audit::Rules> {
    let path = agentcam_core::index::user_rules_path(db);
    agentcam_core::audit::Rules::with_user_file(&path)
        .with_context(|| format!("loading {}", path.display()))
}

fn open_index(db: &std::path::Path) -> anyhow::Result<Index> {
    let mut idx = Index::open(db).with_context(|| format!("opening {}", db.display()))?;
    idx.set_rules(load_rules(db)?);
    Ok(idx)
}

fn scan(db: &std::path::Path, min: &str, range: &str, all: bool) -> anyhow::Result<()> {
    use agentcam_core::query::{self, Filter};
    let mut idx = open_index(db)?;
    let r = idx.update(&agentcam_core::default_log_sources(), false)?;
    let days: Option<i64> = range.strip_suffix('d').and_then(|d| d.parse().ok());
    let f = Filter {
        since: days.map(|d| {
            agentcam_core::util::format_ts_ms(agentcam_core::util::now_ms() - d * 86_400_000)
        }),
        ..Default::default()
    };
    let rank = |s: &str| match s {
        "critical" => 4,
        "high" => 3,
        "medium" => 2,
        _ => 1,
    };
    let c = idx.conn();
    let rules: Vec<_> = query::audit_rules(c, &f)?
        .into_iter()
        .filter(|x| rank(&x.severity) >= rank(min))
        .collect();
    println!(
        "Scanned {} logs with {} rules ({} re-read). Showing {min} and above, {range}.\n",
        r.files_seen,
        idx.rules().len(),
        r.files_parsed
    );
    if rules.is_empty() {
        println!("Nothing found.");
        return Ok(());
    }
    for rule in &rules {
        println!(
            "{:<9} {:<38} {:>5}× in {} sessions",
            rule.severity.to_uppercase(),
            rule.title,
            rule.count,
            rule.sessions
        );
        let shown = if all { 500 } else { 3 };
        for h in query::findings(c, &f, Some(&rule.rule_id), None, shown)? {
            let when =
                h.ts.as_deref()
                    .map(|t| t.get(..10).unwrap_or(t))
                    .unwrap_or("?");
            println!("    {when}  {}  {}#{}", h.excerpt, h.session_id, h.seq);
        }
    }
    println!("\nOpen a step in the UI: agentcam, then /sessions/<id>?step=<n>");
    Ok(())
}

fn index(db: &std::path::Path, full: bool) -> anyhow::Result<()> {
    let mut idx = open_index(db)?;
    let r = idx.update(&agentcam_core::default_log_sources(), full)?;
    println!(
        "Indexed {} of {} log files in {:.1}s ({} events, {} unreadable, {} bad lines)",
        r.files_parsed,
        r.files_seen,
        r.elapsed_ms as f64 / 1000.0,
        r.events_written,
        r.files_failed,
        r.bad_lines
    );
    println!("Index: {}\n", db.display());

    let c = idx.conn();
    let table = |title: &str, sql: &str| -> anyhow::Result<()> {
        println!("{title}");
        let mut stmt = c.prepare(sql)?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        for row in rows {
            let (name, sessions, events) = row?;
            println!("  {name:<52} {sessions:>6} sessions {events:>8} events");
        }
        println!();
        Ok(())
    };
    table(
        "By agent",
        "SELECT agent, count(*), sum(event_count) FROM sessions GROUP BY agent ORDER BY 2 DESC",
    )?;
    table(
        "By month",
        "SELECT coalesce(substr(started_at, 1, 7), 'no date'), count(*), sum(event_count)
         FROM sessions GROUP BY 1 ORDER BY 1 DESC LIMIT 12",
    )?;
    table(
        "Top projects",
        "SELECT coalesce(project_root, project_path, 'no folder'), count(*), sum(event_count)
         FROM sessions GROUP BY 1 ORDER BY 3 DESC LIMIT 10",
    )?;
    Ok(())
}

fn search(db: &std::path::Path, query: &str, limit: usize) -> anyhow::Result<()> {
    let idx = Index::open(db)?;
    for h in idx.search(query, limit)? {
        println!("{}  #{:<5} {:<9} {}", h.session_id, h.seq, h.kind, h.title);
        println!("    {}", h.snippet.replace('\n', " "));
    }
    Ok(())
}

async fn serve(db: &std::path::Path, port: u16, no_open: bool) -> anyhow::Result<()> {
    let state =
        agentcam_server::AppState::open(db, agentcam_core::default_log_sources(), load_rules(db)?)
            .with_context(|| format!("opening {}", db.display()))?;
    // Index in the background; the UI shows progress and refetches.
    state.spawn_index(false);
    spawn_team_sync(db.to_path_buf());
    if !state.watch() {
        tracing::warn!("live updates are off; use the re-index button to pick up new sessions");
    }

    let listener = match agentcam_server::bind(port).await {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            tracing::warn!("port {port} is taken, picking a free one");
            agentcam_server::bind(0).await?
        }
        Err(e) => return Err(e).context("binding to localhost"),
    };

    let url = format!("http://{}", listener.local_addr()?);
    println!("agentcam is running at {url}  (Ctrl-C to stop)");

    if !no_open && let Err(e) = open::that(&url) {
        tracing::warn!("couldn't open the browser: {e}");
    }

    agentcam_server::serve(listener, state).await?;
    Ok(())
}
