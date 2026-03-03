use clap::{Parser, Subcommand};

mod server;
mod client;
mod sync;
mod error;

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
        /// Port to listen on
        #[arg(short, long, default_value = "9876")]
        port: u16,

        /// Authentication token
        #[arg(short, long, env = "SYNCAI_TOKEN")]
        token: String,

        /// Directory to receive files into
        #[arg(short, long, default_value = ".")]
        dir: String,
    },

    /// Push a local directory to a remote syncai server
    Push {
        /// Local directory to sync
        source: String,

        /// Remote target (host:port)
        target: String,

        /// Authentication token
        #[arg(short, long, env = "SYNCAI_TOKEN")]
        token: String,

        /// Force full sync (skip incremental diff)
        #[arg(long)]
        full: bool,
    },

    /// Pull a directory from a remote syncai server
    Pull {
        /// Remote source (host:port)
        source: String,

        /// Local directory to sync into
        target: String,

        /// Authentication token
        #[arg(short, long, env = "SYNCAI_TOKEN")]
        token: String,
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
        Commands::Push { source, target, token, full } => {
            client::push(&source, &target, &token, full).await?;
        }
        Commands::Pull { source, target, token } => {
            client::pull(&source, &target, &token).await?;
        }
    }

    Ok(())
}
