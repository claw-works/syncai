use clap::{Parser, Subcommand};

mod server;
mod client;
mod sync;
mod error;
mod watcher;

#[derive(Parser)]
#[command(
    name = "syncai",
    about = "Peer-to-peer file sync tool for OpenClaw nodes",
    version,
    author = "小爆弹 <klee@openclaw.ai>"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start sync server (run on the target/test machine)
    Server {
        #[arg(short, long, default_value = "9876")]
        port: u16,
        #[arg(short, long, env = "SYNCAI_TOKEN")]
        token: String,
        #[arg(short, long, default_value = ".")]
        dir: String,
    },

    /// Push a local directory to one or more remote syncai servers
    Push {
        source: String,
        /// Primary target (host:port) — required for backward compatibility
        target: String,
        #[arg(short, long, env = "SYNCAI_TOKEN")]
        token: String,
        #[arg(long)]
        full: bool,
        /// Additional targets (comma-separated host:port list, e.g. node2:9876,node3:9876)
        #[arg(long, value_delimiter = ',')]
        targets: Vec<String>,
    },

    /// Pull a directory from a remote syncai server
    Pull {
        source: String,
        target: String,
        #[arg(short, long, env = "SYNCAI_TOKEN")]
        token: String,
    },

    /// Watch a local directory and auto-push changes to one or more remote syncai servers
    Watch {
        /// Local directory to watch
        source: String,
        /// Primary target (host:port)
        target: String,
        #[arg(short, long, env = "SYNCAI_TOKEN")]
        token: String,
        /// Debounce delay in ms (wait for changes to settle before syncing)
        #[arg(long, default_value = "500")]
        debounce: u64,
        /// Additional targets (comma-separated host:port list, e.g. node2:9876,node3:9876)
        #[arg(long, value_delimiter = ',')]
        targets: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("syncai=info".parse()?)
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Server { port, token, dir } => {
            server::run(port, token, dir).await?;
        }
        Commands::Push { source, target, token, full, targets } => {
            let mut all_targets = vec![target];
            all_targets.extend(targets);
            client::push_multi(&source, &all_targets, &token, full).await?;
        }
        Commands::Pull { source, target, token } => {
            client::pull(&source, &target, &token).await?;
        }
        Commands::Watch { source, target, token, debounce, targets } => {
            let mut all_targets = vec![target];
            all_targets.extend(targets);
            watcher::watch(&source, &all_targets, &token, debounce).await?;
        }
    }

    Ok(())
}
