use anyhow::Context;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

const DEFAULT_PORT: u16 = 4747;

/// A dashcam for your coding agents.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let command = Cli::parse().command.unwrap_or(Command::Serve {
        port: DEFAULT_PORT,
        no_open: false,
    });

    match command {
        Command::Serve { port, no_open } => serve(port, no_open).await,
    }
}

async fn serve(port: u16, no_open: bool) -> anyhow::Result<()> {
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

    agentcam_server::serve(listener).await?;
    Ok(())
}
