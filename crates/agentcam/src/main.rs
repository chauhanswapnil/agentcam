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
    /// Full-text search across all indexed sessions.
    Search {
        /// Words to look for (SQLite FTS5 syntax).
        query: Vec<String>,
        #[arg(long, short, default_value_t = 20)]
        limit: usize,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
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
        Command::Search { query, limit } => search(&db, &query.join(" "), limit),
    }
}

fn index(db: &std::path::Path, full: bool) -> anyhow::Result<()> {
    let mut idx = Index::open(db).with_context(|| format!("opening {}", db.display()))?;
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
    let state = agentcam_server::AppState::open(db, agentcam_core::default_log_sources())
        .with_context(|| format!("opening {}", db.display()))?;
    // Index in the background; the UI shows progress and refetches.
    state.spawn_index(false);

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
