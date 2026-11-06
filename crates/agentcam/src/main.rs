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
    /// Team server and joining one (metadata only; see the Outbox page).
    Team {
        #[command(subcommand)]
        command: TeamCommand,
    },
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
enum ExportFormat {
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
    let cli = Cli::parse();
    // The guard hook must print nothing but its verdict, and be fast.
    if matches!(cli.command, Some(Command::Guard)) {
        let db = cli.db.or_else(agentcam_core::default_db_path);
        std::process::exit(guard(db.as_deref()));
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
        },
        Command::Search { query, limit } => search(&db, &query.join(" "), limit),
        Command::Team { command } => team(&db, command).await,
    }
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
            let when = h.ts.as_deref().map(|t| &t[..10]).unwrap_or("?");
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
        "Indexed {} of {} log files in {:.1}s ({} events, {} failed, {} bad lines)",
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
        "SELECT coalesce(substr(started_at, 1, 7), '?'), count(*), sum(event_count)
         FROM sessions GROUP BY 1 ORDER BY 1 DESC LIMIT 12",
    )?;
    table(
        "Top projects",
        "SELECT coalesce(project_path, '?'), count(*), sum(event_count)
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
